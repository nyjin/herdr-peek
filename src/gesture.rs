//! Mouse gestures: the viewer's mouse reports (press, release, move, wheel) told apart as clicks, drags,
//! double-clicks and hover. Platform-neutral; the viewer decides what each gesture does.
use crate::term::MouseKind;

/// What a mouse report amounts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Nothing to do.
    None,
    /// The left button went down: a selection may start here.
    Press,
    /// The left button went down a second time in the same cell, in time: select the word.
    Word,
    /// The pointer entered a new cell with the button held: extend the selection to it.
    Extend,
    /// The selection is final (button released after a drag or a double-click, or its release was lost): copy it.
    Copy,
    /// Press and release in one cell: a click.
    Click,
    /// The pointer entered a new cell with no button held.
    Hover,
}

/// The press in progress.
#[derive(Clone, Copy)]
struct Held {
    word: bool,
    dragging: bool,
}

/// The state that tells gestures apart.
pub struct Gestures {
    double_click_ms: f64,
    /// The last press that ended as a click: its cell and time.
    last_click: Option<((u16, u16), f64)>,
    /// The cell and time of the press in progress.
    press: Option<((u16, u16), f64)>,
    held: Option<Held>,
    /// The last cell the pointer was reported in.
    pointer: Option<(u16, u16)>,
}

impl Gestures {
    pub fn new(double_click_ms: f64) -> Gestures {
        Gestures { double_click_ms, last_click: None, press: None, held: None, pointer: None }
    }

    /// Whether the left button is down (a press has no release yet).
    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    /// The gesture a report makes. `button` is the report's `b & 3`: for `Move`, `0` while the left button is
    /// held and `3` with none. `cell` is the 1-based terminal cell, `t_ms` the time of the report.
    pub fn on(&mut self, kind: MouseKind, button: u8, cell: (u16, u16), t_ms: f64) -> Gesture {
        match (kind, button) {
            (MouseKind::Press, 0) => {
                let double = self
                    .last_click
                    .is_some_and(|(at, t)| at == cell && t_ms - t < self.double_click_ms);
                self.last_click = None;
                self.press = Some((cell, t_ms));
                self.held = Some(Held { word: double, dragging: false });
                self.pointer = Some(cell);
                if double { Gesture::Word } else { Gesture::Press }
            }
            (MouseKind::Release, 0) => {
                self.pointer = Some(cell);
                let Some(held) = self.held.take() else { return Gesture::None };
                let press = self.press.take();
                if held.word || held.dragging {
                    return Gesture::Copy;
                }
                match press {
                    Some((at, t)) if at == cell => {
                        self.last_click = Some((at, t));
                        Gesture::Click
                    }
                    _ => Gesture::None,
                }
            }
            (MouseKind::Move, 0) => {
                let moved = self.pointer != Some(cell);
                self.pointer = Some(cell);
                match &mut self.held {
                    Some(held) if moved => {
                        held.dragging = true;
                        Gesture::Extend
                    }
                    _ => Gesture::None,
                }
            }
            (MouseKind::Move, 3) => {
                // No button although a press is held: its release was lost.
                let lost = self.held.take();
                self.press = None;
                let moved = self.pointer != Some(cell);
                self.pointer = Some(cell);
                match lost {
                    Some(held) if held.word || held.dragging => Gesture::Copy,
                    _ if moved => Gesture::Hover,
                    _ => Gesture::None,
                }
            }
            _ => Gesture::None,
        }
    }
}

/// The page point (CSS pixels, from the page view's top left) under the terminal pixel `(x_px, y_px)`. The view
/// starts at 1-based terminal row `first_row`, is `view_rows` rows of `cell_h` pixels tall and is drawn at `scale`
/// device pixels per CSS pixel (the device pixel ratio times the page zoom). A point above or below the view lands
/// on its first or last pixel row.
pub fn page_point(x_px: u32, y_px: u32, cell_h: u32, first_row: u16, view_rows: u32, scale: f64) -> (f64, f64) {
    let top = (first_row as u32).saturating_sub(1) * cell_h;
    let y_in_view = y_px.saturating_sub(top).min((view_rows * cell_h).saturating_sub(1));
    (x_px as f64 / scale, y_in_view as f64 / scale)
}

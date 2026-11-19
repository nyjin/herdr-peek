//! The selection's overlay: its colour, and its rectangles drawn into strips, one transparent RGBA image per
//! document cell row, that lie over the page tiles. Herdr places every image in whole cells, so a strip is a whole
//! row; the rectangle's pixels are coloured inside it. Platform-neutral.
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::kitty;
use crate::theme::Rgb;

/// A rectangle in device pixels, `y` from the document's top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// WCAG relative luminance, 0 (black) to 1 (white): sRGB channels linearised, weighted 0.2126 / 0.7152 / 0.0722.
fn relative_luminance(c: Rgb) -> f64 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

/// Herdr's selection colour as an overlay: white at 28 % alpha over a dark background (relative luminance below
/// 0.5, herdr's rule), black over a light one, which gives the background mixed 28 % toward white or black, like herdr's own selection. Non-premultiplied RGBA.
pub fn selection_color(page_bg: Rgb) -> [u8; 4] {
    let v = if relative_luminance(page_bg) < 0.5 { 255 } else { 0 };
    [v, v, v, 71]
}

/// `[x, y, w, h]` rectangles in CSS px as device-pixel `Rect`s, at `scale` device px per CSS px (the device pixel
/// ratio times the page zoom).
pub fn rects_from_css(css: &[[f64; 4]], scale: f64) -> Vec<Rect> {
    css.iter().map(|r| Rect { x: r[0] * scale, y: r[1] * scale, w: r[2] * scale, h: r[3] * scale }).collect()
}

/// The pixel columns or rows `start..end` (exclusive) of a span whose centres lie inside `from..from + len`,
/// clamped to `0..limit`.
fn covered(from: f64, len: f64, limit: u32) -> std::ops::Range<u32> {
    let lo = (from - 0.5).ceil().clamp(0.0, limit as f64) as u32;
    let hi = (from + len - 0.5).ceil().clamp(0.0, limit as f64) as u32;
    lo..hi.max(lo)
}

/// Document row `row` as `width` x `cell_h` RGBA: `color` where a pixel's centre lies inside a rect, transparent
/// elsewhere (set, not blended, so overlapping rects never darken). `None` when no rect colours a pixel of the row.
pub fn strip(rects: &[Rect], row: u64, width: u32, cell_h: u32, color: [u8; 4]) -> Option<Vec<u8>> {
    let top = (row * cell_h as u64) as f64;
    let mut px = vec![0u8; width as usize * cell_h as usize * 4];
    let mut any = false;
    for r in rects {
        let cols = covered(r.x, r.w, width);
        for y in covered(r.y - top, r.h, cell_h) {
            for x in cols.clone() {
                let i = (y as usize * width as usize + x as usize) * 4;
                px[i..i + 4].copy_from_slice(&color);
                any = true;
            }
        }
    }
    any.then_some(px)
}

/// The image id of a document row's strip: in `1..1 << 15`, below every tile id (`kitty::image_id`).
pub fn strip_id(row: u64) -> u32 {
    (row % 32_767) as u32 + 1
}

/// What the strips show: the document rows `top..top + rows`, from terminal row `first_screen_row`, in strips
/// `width` x `cell_h` px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripView {
    pub top: u64,
    pub rows: u32,
    pub first_screen_row: u16,
    pub width: u32,
    pub cell_h: u32,
}

/// The strips the terminal holds: per document row sent, the geometry it was made for and a fingerprint of its
/// pixels.
#[derive(Default)]
pub struct Strips {
    sent: BTreeMap<u64, ((u32, u32), u64)>,
}

impl Strips {
    /// The Kitty commands that make the terminal show the selection `rects` on `view`: the strips of rows no longer
    /// shown or touched are deleted, strips whose pixels or geometry changed are transmitted, and every strip is
    /// placed (moving it if the view scrolled).
    pub fn update(&mut self, rects: &[Rect], view: StripView, color: [u8; 4]) -> String {
        let geometry = (view.width, view.cell_h);
        let mut wanted = Vec::new();
        for row in view.top..view.top + view.rows as u64 {
            if let Some(px) = strip(rects, row, view.width, view.cell_h, color) {
                let mut h = DefaultHasher::new();
                px.hash(&mut h);
                wanted.push((row, px, h.finish()));
            }
        }
        let mut out = String::new();
        // deletes first: two rows far apart can share an id, and the new one must survive
        let gone: Vec<u64> = self.sent.keys().copied().filter(|row| !wanted.iter().any(|(r, ..)| r == row)).collect();
        for row in gone {
            self.sent.remove(&row);
            out.push_str(&kitty::delete_image(strip_id(row)));
        }
        for (row, px, fingerprint) in &wanted {
            if self.sent.get(row) != Some(&(geometry, *fingerprint)) {
                out.push_str(&kitty::transmit_direct(strip_id(*row), view.width, view.cell_h, px));
                self.sent.insert(*row, (geometry, *fingerprint));
            }
        }
        for (row, ..) in &wanted {
            let screen = view.first_screen_row as u64 + (row - view.top);
            out.push_str(&kitty::place_at(strip_id(*row), screen as u16, view.width, view.cell_h, 1));
        }
        out
    }

    /// Forget what was sent, after the terminal lost its images (a cleared screen, `delete_all`).
    pub fn forget(&mut self) {
        self.sent.clear();
    }
}

//! Gestures from mouse reports: clicks, drags, double-clicks and hover.
use peek::gesture::*;
use peek::term::MouseKind;

fn press(g: &mut Gestures, cell: (u16, u16), t: f64) -> Gesture {
    g.on(MouseKind::Press, 0, cell, t)
}
fn release(g: &mut Gestures, cell: (u16, u16), t: f64) -> Gesture {
    g.on(MouseKind::Release, 0, cell, t)
}
/// A move with the left button held.
fn drag(g: &mut Gestures, cell: (u16, u16), t: f64) -> Gesture {
    g.on(MouseKind::Move, 0, cell, t)
}
/// A move with no button.
fn moved(g: &mut Gestures, cell: (u16, u16), t: f64) -> Gesture {
    g.on(MouseKind::Move, 3, cell, t)
}

#[test]
fn click_is_press_then_release_in_the_same_cell() {
    let mut g = Gestures::new(500.0);
    assert_eq!(press(&mut g, (3, 5), 0.0), Gesture::Press);
    assert!(g.held());
    assert_eq!(drag(&mut g, (3, 5), 40.0), Gesture::None);
    assert!(g.held());
    assert_eq!(release(&mut g, (3, 5), 80.0), Gesture::Click);
    assert!(!g.held());
}

#[test]
fn drag_extends_on_each_new_cell_and_copies_on_release() {
    let mut g = Gestures::new(500.0);
    assert_eq!(press(&mut g, (3, 5), 0.0), Gesture::Press);
    assert_eq!(drag(&mut g, (4, 5), 10.0), Gesture::Extend);
    assert_eq!(drag(&mut g, (4, 5), 20.0), Gesture::None);
    assert_eq!(drag(&mut g, (9, 7), 30.0), Gesture::Extend);
    assert_eq!(release(&mut g, (9, 7), 40.0), Gesture::Copy);
    assert!(!g.held());
}

#[test]
fn second_press_in_time_and_place_is_a_word() {
    let mut g = Gestures::new(500.0);
    assert_eq!(press(&mut g, (3, 5), 0.0), Gesture::Press);
    assert_eq!(release(&mut g, (3, 5), 50.0), Gesture::Click);
    assert_eq!(press(&mut g, (3, 5), 300.0), Gesture::Word);
    assert_eq!(release(&mut g, (3, 5), 350.0), Gesture::Copy);
    // after a double-click the next press counts from scratch
    assert_eq!(press(&mut g, (3, 5), 400.0), Gesture::Press);
}

#[test]
fn second_press_too_late_or_elsewhere_is_a_press() {
    let mut g = Gestures::new(500.0);
    press(&mut g, (3, 5), 0.0);
    release(&mut g, (3, 5), 50.0);
    assert_eq!(press(&mut g, (3, 5), 600.0), Gesture::Press);
    release(&mut g, (3, 5), 650.0);

    let mut g = Gestures::new(500.0);
    press(&mut g, (3, 5), 0.0);
    release(&mut g, (3, 5), 50.0);
    assert_eq!(press(&mut g, (4, 5), 100.0), Gesture::Press);

    // a drag is not a click: a press in its end cell is not a double-click
    let mut g = Gestures::new(500.0);
    press(&mut g, (3, 5), 0.0);
    drag(&mut g, (4, 5), 10.0);
    release(&mut g, (4, 5), 20.0);
    assert_eq!(press(&mut g, (4, 5), 100.0), Gesture::Press);
}

#[test]
fn lost_release_copies() {
    let mut g = Gestures::new(500.0);
    press(&mut g, (3, 5), 0.0);
    assert_eq!(drag(&mut g, (6, 5), 10.0), Gesture::Extend);
    assert_eq!(moved(&mut g, (7, 5), 20.0), Gesture::Copy);
    assert!(!g.held());
    assert_eq!(moved(&mut g, (8, 5), 30.0), Gesture::Hover);
}

#[test]
fn lost_release_without_a_drag_is_dropped() {
    let mut g = Gestures::new(500.0);
    press(&mut g, (3, 5), 0.0);
    // no Click, no Copy: the press ends quietly and the move is an ordinary one
    assert_eq!(moved(&mut g, (3, 5), 10.0), Gesture::None);
    assert!(!g.held());

    let mut g = Gestures::new(500.0);
    press(&mut g, (3, 5), 0.0);
    assert_eq!(moved(&mut g, (4, 5), 10.0), Gesture::Hover);
    assert!(!g.held());
    // the dropped press is not a click: no double-click follows it
    assert_eq!(press(&mut g, (4, 5), 20.0), Gesture::Press);
}

#[test]
fn hover_only_on_a_new_cell() {
    let mut g = Gestures::new(500.0);
    assert_eq!(moved(&mut g, (3, 5), 0.0), Gesture::Hover);
    assert_eq!(moved(&mut g, (3, 5), 10.0), Gesture::None);
    assert_eq!(moved(&mut g, (4, 5), 20.0), Gesture::Hover);
}

#[test]
fn other_buttons_and_wheel_change_nothing() {
    let mut g = Gestures::new(500.0);
    assert_eq!(g.on(MouseKind::Press, 2, (3, 5), 0.0), Gesture::None);
    assert_eq!(g.on(MouseKind::WheelDown, 0, (3, 5), 10.0), Gesture::None);
    assert!(!g.held());
}

#[test]
fn page_point_maps_like_a_click_and_clamps() {
    // pixel row 82 is terminal row 3, the page's second row
    assert_eq!(page_point(20, 2 * 36 + 10, 36, 2, 20, 2.0), (10.0, 23.0));
    // above the page (the bar row)
    assert_eq!(page_point(20, 5, 36, 2, 20, 2.0).1, 0.0);
    // below the last row
    assert_eq!(page_point(20, 40 * 36, 36, 2, 20, 2.0).1, (20.0 * 36.0 - 1.0) / 2.0);
}

#[test]
fn page_point_takes_the_page_zoom_in_its_scale() {
    // dpr 2 at page zoom 150 %: 3 device px per CSS px
    assert_eq!(page_point(30, 2 * 36 + 10, 36, 2, 20, 3.0), (10.0, 46.0 / 3.0));
}

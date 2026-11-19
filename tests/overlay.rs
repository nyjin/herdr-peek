//! The selection's colour, and rectangles drawn into one-row strips.
use peek::overlay::*;
use peek::theme::Rgb;

const C: [u8; 4] = [255, 255, 255, 71];

fn hex(s: &str) -> Rgb {
    peek::theme::parse_color(s).unwrap()
}

#[test]
fn selection_color_is_herdrs() {
    // herdr: dark when the WCAG relative luminance is below 0.5 (white overlay), else light (black overlay)
    let white = [255, 255, 255, 71];
    let black = [0, 0, 0, 71];
    for (base, want) in [
        ("#1e1e2e", white),
        ("#303446", white),
        ("#000000", white),
        ("#808080", white), // L = 0.216: dark by WCAG, although its weighted-sum brightness is 128
        ("#c0c0c0", black), // L = 0.527
        ("#eff1f5", black),
        ("#ffffff", black),
    ] {
        assert_eq!(selection_color(hex(base)), want, "{base}");
    }
    // over the base, the overlay gives herdr's `b + (t - b) * 0.28`, within 1 per channel
    for base in ["#1e1e2e", "#303446", "#eff1f5", "#000000", "#ffffff", "#808080", "#c0c0c0"] {
        let b = hex(base);
        let c = selection_color(b);
        let target = c[0] as f64;
        for base_ch in [b.0, b.1, b.2] {
            let ours = ((base_ch as u32 * 184 + c[0] as u32 * 71) as f64 / 255.0).round();
            let herdr = (base_ch as f64 + (target - base_ch as f64) * 0.28).round();
            assert!((ours - herdr).abs() <= 1.0, "{base}: {ours} vs {herdr}");
        }
    }
}

#[test]
fn css_rects_scale_by_dpr() {
    assert_eq!(
        rects_from_css(&[[10.0, 100.5, 20.0, 18.0]], 2.0),
        vec![Rect { x: 20.0, y: 201.0, w: 40.0, h: 36.0 }]
    );
}

fn coloured(px: &[u8], width: u32, cell_h: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for y in 0..cell_h {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            if px[i..i + 4] != [0, 0, 0, 0] {
                assert_eq!(px[i..i + 4], C);
                out.push((x, y));
            }
        }
    }
    out
}

#[test]
fn strip_colours_only_the_rect() {
    let rects = [Rect { x: 10.0, y: 36.0, w: 5.0, h: 36.0 }];
    assert_eq!(strip(&rects, 0, 40, 36, C), None);
    assert_eq!(strip(&rects, 2, 40, 36, C), None);
    let px = strip(&rects, 1, 40, 36, C).unwrap();
    assert_eq!(px.len(), 40 * 36 * 4);
    let want: Vec<(u32, u32)> = (0..36).flat_map(|y| (10..15).map(move |x| (x, y))).collect();
    assert_eq!(coloured(&px, 40, 36), want);
}

#[test]
fn rect_across_rows_and_past_the_edge() {
    let rects = [Rect { x: 30.0, y: 20.0, w: 50.0, h: 40.0 }];
    let row0 = strip(&rects, 0, 40, 36, C).unwrap();
    let want0: Vec<(u32, u32)> = (20..36).flat_map(|y| (30..40).map(move |x| (x, y))).collect();
    assert_eq!(coloured(&row0, 40, 36), want0);
    let row1 = strip(&rects, 1, 40, 36, C).unwrap();
    let want1: Vec<(u32, u32)> = (0..24).flat_map(|y| (30..40).map(move |x| (x, y))).collect();
    assert_eq!(coloured(&row1, 40, 36), want1);
}

#[test]
fn overlapping_rects_set_rather_than_blend() {
    let rects = [Rect { x: 0.0, y: 0.0, w: 10.0, h: 36.0 }, Rect { x: 5.0, y: 0.0, w: 10.0, h: 36.0 }];
    let px = strip(&rects, 0, 20, 36, C).unwrap();
    assert_eq!(coloured(&px, 20, 36).len(), 15 * 36);
}

#[test]
fn strip_ids_stay_below_the_tiles() {
    assert_eq!(strip_id(2), 3);
    for row in [0u64, 1, 32_766, 32_767, 1_000_000] {
        let id = strip_id(row);
        assert!(id >= 1 && id < 1 << 15, "{id}");
    }
}

fn count(s: &str, needle: &str) -> usize {
    s.matches(needle).count()
}

#[test]
fn strips_send_once_and_follow_the_view() {
    let view = StripView { top: 0, rows: 10, first_screen_row: 2, width: 40, cell_h: 36 };
    let r = |row: u64, x: f64| Rect { x, y: row as f64 * 36.0, w: 5.0, h: 36.0 };
    let mut s = Strips::default();

    let out = s.update(&[r(2, 10.0), r(3, 10.0)], view, C);
    assert_eq!(count(&out, "a=t"), 2);
    assert_eq!(count(&out, "a=p"), 2);
    assert!(out.contains("\x1b[4;1H") && out.contains("\x1b[5;1H"), "rows 2 and 3 at first_screen_row + 2, + 3");
    assert_eq!(count(&out, "z=1"), 2);

    let out = s.update(&[r(2, 10.0), r(3, 10.0)], view, C);
    assert_eq!((count(&out, "a=t"), count(&out, "a=p")), (0, 2));

    let out = s.update(&[r(2, 10.0), r(3, 12.0)], view, C);
    assert_eq!(count(&out, "a=t"), 1);
    assert!(out.contains("i=4,"), "only row 3 (id 4) is sent: {out}");
    assert_eq!(count(&out, "a=p"), 2);

    // scrolling to top 3: row 2 leaves, row 3 stands at the first screen row
    let scrolled = StripView { top: 3, ..view };
    let out = s.update(&[r(2, 10.0), r(3, 12.0)], scrolled, C);
    assert!(out.contains("a=d,d=I,i=3,"), "{out}");
    assert_eq!(count(&out, "a=t"), 0);
    assert_eq!(count(&out, "a=p"), 1);
    assert!(out.contains("\x1b[2;1H"), "{out}");

    // a new width sends every strip again
    let wider = StripView { width: 48, ..scrolled };
    let out = s.update(&[r(3, 12.0)], wider, C);
    assert_eq!(count(&out, "a=t"), 1);
    let taller = StripView { cell_h: 40, ..wider };
    let out = s.update(&[Rect { x: 12.0, y: 120.0, w: 5.0, h: 40.0 }], taller, C);
    assert_eq!(count(&out, "a=t"), 1);

    // no rects: every sent strip is deleted
    let out = s.update(&[], taller, C);
    assert_eq!(count(&out, "a=d,d=I"), 1);
    assert_eq!(count(&out, "a=p"), 0);

    // forget(): the terminal's images are gone, so the next update sends again
    s.update(&[r(3, 12.0)], wider, C);
    s.forget();
    let out = s.update(&[r(3, 12.0)], wider, C);
    assert_eq!(count(&out, "a=t"), 1);
}

#[test]
fn every_sent_strip_is_deleted_when_the_rects_go() {
    let view = StripView { top: 0, rows: 10, first_screen_row: 2, width: 40, cell_h: 36 };
    let rects: Vec<Rect> = (2..5).map(|row| Rect { x: 0.0, y: row as f64 * 36.0, w: 5.0, h: 36.0 }).collect();
    let mut s = Strips::default();
    s.update(&rects, view, C);
    let out = s.update(&[], view, C);
    assert_eq!(count(&out, "a=d,d=I"), 3);
}

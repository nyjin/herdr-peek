//! Page colours from the terminal's colour answers, source highlighting in its colours, and code fonts from Ghostty.
use peek::theme::*;

fn rgb(hex: u32) -> Rgb {
    Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The answers herdr gave under Ghostty's Catppuccin Mocha (measured 2026-11-18).
fn mocha() -> TermColors {
    let ansi = [
        0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de, 0x585b70, 0xf7aec2, 0xc2ecbf,
        0xfcd682, 0xaeccfc, 0xf398da, 0xb1eae1, 0xa6adc8,
    ];
    TermColors { fg: Some(rgb(0xcdd6f4)), bg: Some(rgb(0x1e1e2e)), ansi: ansi.map(|c| Some(rgb(c))) }
}

#[test]
fn colour_forms() {
    for s in ["rgb:cdcd/d6d6/f4f4", "rgb:cd/d6/f4", "#CDD6F4"] {
        assert_eq!(parse_color(s), Some(rgb(0xcdd6f4)), "{s}");
    }
    assert_eq!(parse_color("rgb:f/0/8"), Some(Rgb(255, 0, 136)));
    assert_eq!(parse_color("rgb:FFFF/0000/8080"), Some(Rgb(255, 0, 128)));
    for s in ["rgba:1/2/3/4", "rgb:12/34", "rgb:12/34/zz", "rgb:12345/0/0", "#12345", "nonsense", ""] {
        assert_eq!(parse_color(s), None, "{s}");
    }
}

#[test]
fn answers_fill_the_colours() {
    let mut c = TermColors::default();
    assert!(take_answer(&mut c, "10;rgb:cdcd/d6d6/f4f4"));
    assert!(take_answer(&mut c, "11;#1e1e2e"));
    assert!(take_answer(&mut c, "4;15;rgb:a6/ad/c8"));
    assert!(!take_answer(&mut c, "4;16;rgb:a6/ad/c8"));
    assert!(!take_answer(&mut c, "12;rgb:a6/ad/c8"));
    assert!(!take_answer(&mut c, "10;?"));
    assert_eq!((c.fg, c.bg, c.ansi[15]), (Some(rgb(0xcdd6f4)), Some(rgb(0x1e1e2e)), Some(rgb(0xa6adc8))));
    assert_eq!(c.answered(), 3);
    assert!(!c.complete());
    assert!(mocha().complete());
}

#[test]
fn query_asks_eighteen_colours_then_attributes() {
    let q = query();
    assert!(q.starts_with("\x1b]10;?\x1b\\\x1b]11;?\x1b\\"), "{q:?}");
    assert_eq!(q.matches("\x1b]4;").count(), 16);
    assert!(q.contains("\x1b]4;15;?\x1b\\"));
    assert!(q.ends_with("\x1b[c"), "{q:?}");
}

#[test]
fn light_or_dark() {
    assert!(!is_light(rgb(0x1e1e2e)));
    assert!(!is_light(rgb(0x303446))); // Catppuccin Frappe: Ghostty's "light" theme here, a dark background
    assert!(is_light(rgb(0xeff1f5)));
}

#[test]
fn mocha_answers_give_about_todays_page() {
    let t = PageTheme::from_terminal(&mocha(), false, vec![]);
    assert!(t.dark);
    let got = [
        t.base, t.text, t.mantle, t.surface0, t.surface1, t.overlay0, t.subtext0, t.blue, t.find, t.current, t.current_text,
    ]
    .map(Rgb::hex);
    assert_eq!(
        got,
        ["#1e1e2e", "#cdd6f4", "#181825", "#333446", "#45465a", "#6d7187", "#a7aec8", "#89b4fa", "#f9e2af", "#f9e2af", "#1e1e2e"]
    );
}

#[test]
fn light_background_mixes_toward_the_text() {
    let c = TermColors { fg: Some(rgb(0x4c4f69)), bg: Some(rgb(0xeff1f5)), ..TermColors::default() };
    let t = PageTheme::from_terminal(&c, true, vec![]);
    assert!(!t.dark);
    assert_eq!((t.mantle.hex(), t.surface0.hex()), ("#e7e9ee".to_string(), "#dbdee4".to_string()));
}

#[test]
fn fg_and_bg_without_palette() {
    let c = TermColors { fg: mocha().fg, bg: mocha().bg, ..TermColors::default() };
    let t = PageTheme::from_terminal(&c, false, vec![]);
    let builtin = PageTheme::builtin(true);
    assert_eq!((t.blue, t.find, t.current), (builtin.blue, builtin.find, builtin.current));
    assert_eq!(t.base, rgb(0x1e1e2e));
}

#[test]
fn nothing_answered_is_builtin() {
    assert_eq!(PageTheme::from_terminal(&TermColors::default(), false, vec![]), PageTheme::builtin(false));
    assert_eq!(PageTheme::from_terminal(&TermColors::default(), true, vec![]), PageTheme::builtin(true));
}

/// One line of Rust highlighted with `theme`, as the source page renders it.
fn highlight_rust(theme: &syntect::highlighting::Theme, line: &str) -> String {
    use syntect::easy::HighlightLines;
    use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
    use syntect::parsing::SyntaxSet;
    let ss = SyntaxSet::load_defaults_newlines();
    let mut h = HighlightLines::new(ss.find_syntax_by_extension("rs").unwrap(), theme);
    let regions = h.highlight_line(line, &ss).unwrap();
    styled_line_to_highlighted_html(&regions, IncludeBackground::No).unwrap()
}

#[test]
fn syntax_theme_uses_the_terminal_colours() {
    let t = PageTheme::from_terminal(&mocha(), false, vec![]);
    let html = highlight_rust(t.syntax.as_ref().expect("a theme from the 16 colours"), "fn main() { let s = \"x\"; let n = 42; // note\n");
    assert!(html.contains("color:#f5c2e7;\">fn"), "keyword: {html}");
    assert!(html.contains("color:#f5c2e7;\">let"), "keyword: {html}");
    assert!(html.contains("color:#a6e3a1;\">&quot;x&quot;"), "string: {html}");
    assert!(html.contains("color:#f9e2af;\">42"), "number: {html}");
    assert!(html.contains("color:#89b4fa;\">main"), "function: {html}");
    assert!(html.contains("font-style:italic;color:#6d7187;\">// note"), "comment: {html}");
}

#[test]
fn no_palette_no_syntax_theme() {
    let c = TermColors { fg: mocha().fg, bg: mocha().bg, ..TermColors::default() };
    assert_eq!(PageTheme::from_terminal(&c, false, vec![]).syntax, None);
    let mut partial = mocha();
    partial.ansi[6] = None;
    assert_eq!(PageTheme::from_terminal(&partial, false, vec![]).syntax, None);
}

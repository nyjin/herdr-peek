//! Input parsing, pane geometry and the top bar.
use peek::term::*;

#[test]
fn parses_keys_and_arrows() {
    let (inputs, used) = parse(b"jk \x1b[A\x1b[B\x1b[5~\x1b[6~\x7f\r\x1bc");
    assert_eq!(used, 21);
    assert_eq!(
        inputs,
        vec![
            Input::Key(Key::Char('j')),
            Input::Key(Key::Char('k')),
            Input::Key(Key::Char(' ')),
            Input::Key(Key::Up),
            Input::Key(Key::Down),
            Input::Key(Key::PageUp),
            Input::Key(Key::PageDown),
            Input::Key(Key::Backspace),
            Input::Key(Key::Enter),
            Input::Key(Key::Alt('c')),
        ]
    );
}

#[test]
fn parses_utf8_text() {
    // a Korean syllable typed into the find field arrives as three UTF-8 bytes
    let (inputs, used) = parse("\u{d55c}".as_bytes());
    assert_eq!((inputs, used), (vec![Input::Key(Key::Char('\u{d55c}'))], 3));
}

#[test]
fn parses_sgr_cell_mouse() {
    // peek asks for SGR reports in cells (no ?1016h): the numbers are a 1-based column and row
    let (inputs, _) = parse(b"\x1b[<0;12;34M\x1b[<0;12;34m\x1b[<35;194;69M\x1b[<64;1;1M\x1b[<65;3;2M");
    let kinds: Vec<_> = inputs
        .iter()
        .map(|i| match i {
            Input::Mouse(m) => (m.kind, m.col, m.row),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (MouseKind::Press, 12, 34),
            (MouseKind::Release, 12, 34),
            (MouseKind::Move, 194, 69),
            (MouseKind::WheelUp, 1, 1),
            (MouseKind::WheelDown, 3, 2),
        ]
    );
}

#[test]
fn a_reported_cell_stands_for_the_pixel_at_its_centre() {
    // herdr 0.9.4's `cell_center_px` rule, up to integer rounding; a 194 x 69 pane of 8 x 18 px cells
    assert_eq!(cell_center(1, 1, 8, 18), (4, 9));
    assert_eq!(cell_center(194, 69, 8, 18), (1548, 1233));
    // a report never names column or row 0; if one did, it would stand for the first cell
    assert_eq!(cell_center(0, 0, 8, 18), (4, 9));
}

#[test]
fn mouse_reports_are_asked_for_in_cells() {
    // herdr before 0.9.4 answers ?1016h (pixels) with cell numbers, which cannot be told apart from pixels
    for mode in ["?1000h", "?1003h", "?1006h"] {
        assert!(MOUSE_ON.contains(mode), "{mode}");
    }
    assert!(!MOUSE_ON.contains("1016") && !MOUSE_OFF.contains("1016"));
    for mode in ["?1000l", "?1003l", "?1006l"] {
        assert!(MOUSE_OFF.contains(mode), "{mode}");
    }
}

#[test]
fn keeps_partial_sequence() {
    let (inputs, used) = parse(b"j\x1b[<0;53");
    assert_eq!((inputs, used), (vec![Input::Key(Key::Char('j'))], 1));
    // a lone escape at the end may be the start of a sequence: left for the next read
    let (inputs, used) = parse(b"k\x1b");
    assert_eq!((inputs, used), (vec![Input::Key(Key::Char('k'))], 1));
}

#[test]
fn parses_kitty_reply() {
    let (inputs, used) = parse(b"\x1b_Gi=1;ENOMEM: out of memory\x1b\\j");
    assert_eq!(used, 31);
    assert_eq!(inputs, vec![Input::Reply("i=1;ENOMEM: out of memory".into()), Input::Key(Key::Char('j'))]);
}

#[test]
fn geometry_without_pixels_falls_back() {
    // under cargo test stdout is not a terminal: no size at all
    let g = geometry();
    assert_eq!((g.cell_w, g.cell_h), (8, 18));
    assert!(g.cols > 0 && g.rows > 0);
}

#[test]
fn parses_shift_enter() {
    let (inputs, _) = parse(b"\x1b[13;2u\x1b[27;2;13~");
    assert_eq!(inputs, vec![Input::Key(Key::ShiftEnter), Input::Key(Key::ShiftEnter)]);
}

#[test]
fn bar_order_icons_and_words() {
    let actions: Vec<&str> = bar_items(false, false).iter().map(|i| i.action).collect();
    assert_eq!(actions, ["reload", "find", "toggle", "browser", "more"]);
    let (text, ranges) = render_bar("plan.md", false, false, 120, None);
    assert_eq!(ranges.len(), 5);
    // ⟳ and ⌕ draw wider than their cell in common fonts: two spaces keep them off the word
    for cell in ["\u{27f3}  Reload", "\u{2315}  Find", "</> Source", "\u{2197} Browser", "\u{2026} More"] {
        assert!(text.contains(cell), "{cell} missing from {text:?}");
    }
    assert!(text.contains("plan.md") && !text.contains('%'));
    assert!(ranges.last().unwrap().1 <= 120);
    // hovering an item shows its tooltip (name and key) and marks the item
    let (hover, _) = render_bar("plan.md", false, false, 120, Some(0));
    assert!(hover.contains("Reload (r)") && hover.contains("\x1b[7m"));
}

#[test]
fn the_back_button_stands_first_only_with_somewhere_to_go_back_to() {
    // without history the bar is as it was
    assert!(bar_items(false, false).iter().all(|i| i.action != "back"));
    let (text, ranges) = render_bar("plan.md", false, false, 120, None);
    assert!(!text.contains("Back") && !text.contains('\u{2190}') && ranges.len() == 5);
    // after following a link: Back first, with a tooltip like the others
    let items = bar_items(false, true);
    let actions: Vec<&str> = items.iter().map(|i| i.action).collect();
    assert_eq!(actions, ["back", "reload", "find", "toggle", "browser", "more"]);
    assert_eq!(tooltip(&items[0]), "Back (h)");
    let (text, ranges) = render_bar("plan.md", false, true, 120, None);
    assert_eq!(ranges.len(), 6);
    let back = text.find("\u{2190} Back").expect("the Back item");
    assert!(back < text.find("Reload").unwrap(), "{text:?}");
    // the hovered Back shows its tooltip
    let (hover, _) = render_bar("plan.md", false, true, 120, Some(0));
    assert!(hover.contains("Back (h)"));
    // the source view has it too
    assert_eq!(bar_items(true, true)[0].action, "back");
}

#[test]
fn the_back_button_follows_the_bar_rules() {
    // one column apart, like the others
    let (_, ranges) = render_bar("plan.md", false, true, 120, None);
    let gaps: Vec<u16> = ranges.windows(2).map(|w| w[1].0 - w[0].1).collect();
    assert!(gaps.iter().all(|g| *g == 2), "{ranges:?}");
    // a narrow pane keeps the symbol without the word
    let (text, ranges) = render_bar("a-very-long-document-name-that-does-not-fit.md", false, true, 40, None);
    assert!(ranges.last().unwrap().1 <= 40, "{ranges:?}");
    assert!(text.contains('\u{2190}') && !text.contains("Back"));
}

#[test]
fn the_source_button_turns_into_view() {
    let (rendered, _) = render_bar("plan.md", false, false, 120, None);
    let (source, _) = render_bar("plan.md", true, false, 120, None);
    assert!(rendered.contains("</> Source") && !rendered.contains("View"));
    assert!(source.contains("\u{25c9} View") && !source.contains("Source"));
    let tip = |showing_source: bool| tooltip(&bar_items(showing_source, false)[2]);
    assert_eq!((tip(false).as_str(), tip(true).as_str()), ("Show source (v)", "Show rendered (v)"));
}

#[test]
fn items_stand_an_even_distance_apart() {
    for (_, ranges) in [render_bar("plan.md", false, false, 120, None), render_bar("plan.md", true, false, 120, None)] {
        let gaps: Vec<u16> = ranges.windows(2).map(|w| w[1].0 - w[0].1).collect();
        assert!(gaps.iter().all(|g| *g == 2), "{ranges:?}");
    }
}

#[test]
fn a_symbol_takes_one_column() {
    let (_, ranges) = render_bar("plan.md", false, false, 120, None);
    // " ⟳  Reload " is eleven columns although "⟳" is three bytes
    assert_eq!(ranges[0].1 - ranges[0].0 + 1, 11);
}

#[test]
fn a_narrow_bar_keeps_the_icons() {
    let (text, ranges) = render_bar("a-very-long-document-name-that-does-not-fit.md", false, false, 40, None);
    assert!(ranges.last().unwrap().1 <= 40, "{ranges:?}");
    assert!(text.contains("\u{27f3}") && !text.contains("Reload"));
}

#[test]
fn tooltips_name_the_key() {
    let items = bar_items(false, false).into_iter().chain(bar_items(true, false)).chain(more_items()).chain(find_items());
    let tips: Vec<String> = items.map(|i| tooltip(&i)).collect();
    for tip in [
        "Open in browser (o)",
        "More: zoom, screenshot",
        "Zoom in (+)",
        "Zoom out (-)",
        "Screenshot (s)",
        "Next match (Enter)",
        "Close (Esc)",
    ] {
        assert!(tips.contains(&tip.to_string()), "{tip} missing from {tips:?}");
    }
}

#[test]
fn more_row_holds_zoom_and_screenshot() {
    let (text, ranges) = render_more("plan.md", 110, 120, None);
    assert_eq!(ranges.len(), more_items().len());
    assert!(text.contains("110%") && text.contains("\u{2399} Screenshot"));
    // the percentage stands between zoom out and zoom in
    let plain = text.replace("\x1b[1;1H\x1b[2K", "");
    let between: String = plain.chars().skip(ranges[0].1 as usize).take((ranges[1].0 - ranges[0].1 - 1) as usize).collect();
    assert_eq!(between.trim(), "110%");
    assert!(ranges.last().unwrap().1 <= 120);
}

#[test]
fn find_row_shows_the_query_options_position_and_caret() {
    use peek::script::FindQuery;
    let q = FindQuery { text: "agent".into(), case: true, word: false, regex: true };
    let row = render_find(&q, 22, 2, 100, None);
    assert!(row.text.contains("agent") && row.text.contains("3/22"));
    // an option that is on is drawn inverted; one that is off is not
    assert!(row.text.contains("\x1b[7m Aa \x1b[27m") && row.text.contains("\x1b[7m .* \x1b[27m"));
    assert!(!row.text.contains("\x1b[7m ab \x1b[27m"));
    for glyph in ["\u{2315}", "\u{2191}", "\u{2193}", "\u{2715}"] {
        assert!(row.text.contains(glyph), "{glyph}");
    }
    assert_eq!(row.buttons.len(), find_items().len());
    for w in row.buttons.windows(2) {
        assert!(w[0].1 < w[1].0, "{:?}", row.buttons);
    }
    assert_eq!(row.buttons.last().unwrap().1, 100);
    // the caret stands right after the query: " ⌕ " is three columns, so "agent" ends at column 8
    assert_eq!(row.caret, Some(9));
    assert!(render_find(&q, 0, 0, 100, None).text.contains("No results"));
    assert!(render_find(&q, -1, 0, 100, None).text.contains("Bad regex"));
}

#[test]
fn an_empty_find_field_says_find_and_takes_the_caret() {
    let row = render_find(&peek::script::FindQuery::default(), 0, 0, 80, None);
    assert!(row.text.contains("\x1b[2mFind"));
    assert_eq!(row.caret, Some(4));
}

#[test]
fn hovering_a_find_button_shows_its_tooltip_in_the_field() {
    let q = peek::script::FindQuery { text: "agent".into(), ..Default::default() };
    let next = find_items().iter().position(|i| i.action == "next").unwrap();
    let row = render_find(&q, 3, 0, 100, Some(next));
    assert!(row.text.contains("Next match (Enter)"));
    assert_eq!(row.caret, None);
}

#[test]
fn a_long_query_shows_its_end() {
    let q = peek::script::FindQuery { text: format!("{}tail", "x".repeat(200)), ..Default::default() };
    let row = render_find(&q, 1, 0, 80, None);
    assert!(row.text.contains("\u{2026}") && row.text.contains("tail"));
    let caret = row.caret.unwrap();
    assert!(caret < row.buttons[0].0, "caret {caret} past the field: {:?}", row.buttons);
}

#[test]
fn caret_shows_or_hides_the_cursor() {
    assert_eq!(caret(Some(9)), "\x1b[1;9H\x1b[5 q\x1b[?25h");
    assert_eq!(caret(None), "\x1b[?25l");
}

#[test]
fn a_toast_stands_centred_in_a_pill_on_the_last_row() {
    let row = render_toast("Screenshot saved", 24, 40);
    assert!(row.starts_with("\x1b[24;1H\x1b[2K"), "{row:?}");
    let plain = row.replace("\x1b[24;1H\x1b[2K", "");
    let lead = plain.chars().take_while(|c| *c == ' ').count();
    // " Screenshot saved " is 18 columns: (40 - 18) / 2 = 11 spaces before it
    assert_eq!(lead, 11, "{row:?}");
    assert!(row.contains("\x1b[7m Screenshot saved \x1b[27m"));
}

#[test]
fn a_long_path_keeps_its_start_and_file_name() {
    let path = "/tmp/peek-501/-Users-nyjin-src-herdr-skills-peek/18b74fa6-f5db-42ec/screenshots/20261117-101500-plan.md.png";
    let short = shorten_path(path, 50);
    assert!(display_width(&short) <= 50, "{short}");
    assert!(short.starts_with("/tmp/") && short.ends_with("/20261117-101500-plan.md.png"), "{short}");
    assert!(short.contains('\u{2026}'));
    assert_eq!(shorten_path("/tmp/a.png", 50), "/tmp/a.png");
    // too narrow even for the file name: its end
    assert!(display_width(&shorten_path(path, 12)) <= 12);
}

#[test]
fn parses_colour_answers() {
    let input = b"\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]4;1;rgb:f3/8b/a8\x07j";
    let (inputs, used) = parse(input);
    assert_eq!(used, input.len());
    assert_eq!(
        inputs,
        vec![
            Input::Osc("10;rgb:cdcd/d6d6/f4f4".into()),
            Input::Osc("4;1;rgb:f3/8b/a8".into()),
            Input::Key(Key::Char('j')),
        ]
    );
}

#[test]
fn colour_answer_split_across_reads() {
    assert_eq!(parse(b"\x1b]11;rgb:1e1e/1e"), (vec![], 0));
    let whole = b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\";
    assert_eq!(parse(whole), (vec![Input::Osc("11;rgb:1e1e/1e1e/2e2e".into())], whole.len()));
}

#[test]
fn device_attributes_and_scheme_reports_are_not_keys() {
    // A colour-scheme report (appearance switched while the viewer is open) is dropped: no live switching.
    let input = b"\x1b[?62;22c\x1b[?997;2nr";
    let (inputs, used) = parse(input);
    assert_eq!(used, input.len());
    assert_eq!(inputs, vec![Input::Attributes, Input::Key(Key::Char('r'))]);
}

#[test]
fn unterminated_osc_gives_way_to_later_input() {
    // Alt+] (sent as ESC ]) and then typing: the OSC never ends, so after a while the viewer gives up on it.
    let input = b"\x1b]jkr\x1b[<35;10;20M";
    assert_eq!(parse(input), (vec![], 0));
    let (inputs, used) = give_up(input);
    assert_eq!(used, input.len());
    let mut expected = vec![
        Input::Key(Key::Alt(']')),
        Input::Key(Key::Char('j')),
        Input::Key(Key::Char('k')),
        Input::Key(Key::Char('r')),
    ];
    expected.extend(parse(b"\x1b[<35;10;20M").0);
    assert_eq!(inputs, expected);
}

#[test]
fn status_shows_a_file_as_its_path() {
    assert_eq!(status_text("file:///Users/me/docs/a%20b.md", 80), "/Users/me/docs/a b.md");
    let long = format!("file:///{}/notes.md", "dir".repeat(40));
    let shown = status_text(&long, 30);
    assert!(shown.ends_with("notes.md"), "{shown}");
    assert!(shown.contains('\u{2026}'), "{shown}");
    assert!(display_width(&shown) <= 30, "{shown}");
}

#[test]
fn status_shows_anchors_and_addresses_as_written() {
    assert_eq!(status_text("#install", 80), "#install");
    assert_eq!(status_text("https://example.com/a?b=c", 80), "https://example.com/a?b=c");
    let long = format!("https://example.com/{}", "x".repeat(180));
    let shown = status_text(&long, 40);
    assert!(display_width(&shown) <= 40, "{shown}");
    assert!(shown.ends_with('\u{2026}'), "{shown}");
}

#[test]
fn status_pill_is_inverted_at_the_left() {
    let row = render_status("x", 24);
    assert!(row.starts_with("\x1b[24;1H\x1b[2K\x1b[7m x "), "{row:?}");
    assert!(row.ends_with("\x1b[27m"), "{row:?}");
}

#[test]
fn status_never_carries_control_characters() {
    // a link can name a file `%1b]0;x%07`: decoded, that is a terminal escape sequence
    for href in ["file:///a/%1b]0;x%07", "file:///a/b%0a%0dc", "#a\u{1b}b", "https://x/\u{7}y\u{9b}z"] {
        for width in [80, 12] {
            let shown = status_text(href, width);
            assert!(!shown.chars().any(char::is_control), "{href:?} -> {shown:?}");
            assert!(display_width(&shown) <= width, "{shown:?}");
        }
    }
    assert_eq!(status_text("#a\u{1b}b", 80), "#a\u{fffd}b");
    assert_eq!(status_text("file:///a/%1b]0;x%07", 80), "/a/\u{fffd}]0;x\u{fffd}");
}

#[test]
fn percent_decoding_never_splits_a_character() {
    // A `%` before a multi-byte character (U+AC00, a Korean syllable: three UTF-8 bytes) is no escape: it stays as
    // written instead of slicing the string inside that character. A hovered link can carry one (`file://%\u{ac00}/x`
    // comes back from the hit test as written).
    assert_eq!(percent_decode("%\u{ac00}"), "%\u{ac00}");
    assert_eq!(percent_decode("a%2"), "a%2");
    assert_eq!(percent_decode("%"), "%");
    assert_eq!(percent_decode("a%20b%2"), "a b%2");
    assert_eq!(percent_decode("%\u{ac00}%20"), "%\u{ac00} ");
    assert_eq!(status_text("file://%\u{ac00}/x", 40), "%\u{ac00}/x");
}

#[test]
fn status_never_carries_bidi_controls() {
    // U+202E (right-to-left override) would make a link to `https://a<U+202E>gpj.exe` read `https://aexe.jpg`: the
    // status row would show another destination than a click opens.
    let shown = status_text("https://a\u{202e}gpj.exe", 80);
    assert!(!shown.contains('\u{202e}'), "{shown:?}");
    assert_eq!(shown, "https://a\u{fffd}gpj.exe");
    // the other bidi format characters: marks, embeddings, overrides, isolates
    let bidi = ['\u{61c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}'];
    for c in bidi.into_iter().chain(['\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}']) {
        assert_eq!(status_text(&format!("#a{c}b"), 80), "#a\u{fffd}b", "{c:?}");
    }
    // decoded from a file URL as well: %E2%80%AE is U+202E
    assert_eq!(status_text("file:///a/%E2%80%AEb", 80), "/a/\u{fffd}b");
}

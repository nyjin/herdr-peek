//! Terminal side of the viewer: input parsing (keys, SGR pixel mouse, Kitty replies), pane geometry, and row 1:
//! the top bar, the zoom row behind its `…`, and the find row.
use crate::kitty::Geometry;
use crate::script::FindQuery;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Backspace,
    Esc,
    Enter,
    /// Shift+Enter, from terminals that tell it apart (`CSI 13;2u` or `CSI 27;2;13~`).
    ShiftEnter,
    Tab,
    Alt(char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Press,
    Release,
    Move,
    WheelUp,
    WheelDown,
}

/// A mouse report: what happened, the button (`b & 3`) and the 1-based cell (`cell_center` gives its pixel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub kind: MouseKind,
    pub button: u8,
    pub col: u32,
    pub row: u32,
}

/// The pixel (from the pane's top left) at the centre of the 1-based cell `(col, row)` of `cell_w` x `cell_h` px: the
/// pointer a cell report stands for, as herdr 0.9.4 and later turn a cell into pixels (`cell_center_px`).
pub fn cell_center(col: u32, row: u32, cell_w: u32, cell_h: u32) -> (u32, u32) {
    // saturating: the numbers come from the terminal
    let centre = |n: u32, size: u32| n.saturating_sub(1).saturating_mul(size).saturating_add(size / 2);
    (centre(col, cell_w), centre(row, cell_h))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Key(Key),
    Mouse(Mouse),
    /// A Kitty graphics reply (`ESC _G ... ESC \`), without the framing.
    Reply(String),
    /// An OSC answer (`ESC ] ... ESC \` or BEL), without the framing: the terminal's colours (`10;rgb:...`).
    Osc(String),
    /// The terminal's primary device attributes (`CSI ? ... c`): the end of the answers to peek's colour query.
    Attributes,
}

/// Inputs in `buf` and the number of bytes consumed; an incomplete sequence at the end (a lone escape included) is
/// left for the next read. The caller turns a lone escape that stays alone into `Key::Esc`.
pub fn parse(buf: &[u8]) -> (Vec<Input>, usize) {
    let mut out = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        let rest = &buf[i..];
        let used = match rest[0] {
            0x1b => match escape(rest) {
                Some((input, n)) => {
                    out.extend(input);
                    n
                }
                None => break,
            },
            0x7f | 0x08 => {
                out.push(Input::Key(Key::Backspace));
                1
            }
            b'\r' | b'\n' => {
                out.push(Input::Key(Key::Enter));
                1
            }
            b'\t' => {
                out.push(Input::Key(Key::Tab));
                1
            }
            b => {
                let n = utf8_len(b);
                if rest.len() < n {
                    break;
                }
                if let Some(c) = std::str::from_utf8(&rest[..n]).ok().and_then(|s| s.chars().next()) {
                    if !c.is_control() {
                        out.push(Input::Key(Key::Char(c)));
                    }
                }
                n
            }
        };
        i += used;
    }
    (out, i)
}

/// `parse` for input whose head has waited too long as an incomplete escape sequence (an `ESC ]` that never ends,
/// such as Alt+] sent as `ESC ]`): its first two bytes become `Alt(<second byte>)` and the rest is parsed as usual.
pub fn give_up(buf: &[u8]) -> (Vec<Input>, usize) {
    if buf.len() < 2 || buf[0] != 0x1b {
        return parse(buf);
    }
    let mut out = vec![Input::Key(Key::Alt(buf[1] as char))];
    let (rest, used) = parse(&buf[2..]);
    out.extend(rest);
    (out, 2 + used)
}

fn utf8_len(b: u8) -> usize {
    match b {
        0xf0..=0xf7 => 4,
        0xe0..=0xef => 3,
        0xc0..=0xdf => 2,
        _ => 1,
    }
}

/// One escape sequence at the start of `s`: `None` when it is incomplete.
fn escape(s: &[u8]) -> Option<(Option<Input>, usize)> {
    match *s.get(1)? {
        b'[' => {
            if s.get(2) == Some(&b'<') {
                let end = s.iter().position(|&b| b == b'M' || b == b'm')?;
                return Some((mouse(&s[3..end], s[end] == b'M'), end + 1));
            }
            let end = 2 + s[2..].iter().position(|b| (0x40..=0x7e).contains(b))?;
            if s[2] == b'?' && s[end] == b'c' {
                return Some((Some(Input::Attributes), end + 1));
            }
            let key = match (&s[2..end], s[end]) {
                (b"", b'A') => Some(Key::Up),
                (b"", b'B') => Some(Key::Down),
                (b"", b'H') | (b"1", b'~') => Some(Key::Home),
                (b"", b'F') | (b"4", b'~') => Some(Key::End),
                (b"5", b'~') => Some(Key::PageUp),
                (b"6", b'~') => Some(Key::PageDown),
                (b"13;2", b'u') | (b"27;2;13", b'~') => Some(Key::ShiftEnter),
                _ => None,
            };
            Some((key.map(Input::Key), end + 1))
        }
        b'_' => {
            let end = s.windows(2).position(|w| w == b"\x1b\\")?;
            let body = String::from_utf8_lossy(&s[2..end]);
            Some((body.strip_prefix('G').map(|b| Input::Reply(b.to_string())), end + 2))
        }
        b']' => {
            // Ended by ST (`ESC \`) or BEL, whichever comes first.
            let st = s.windows(2).position(|w| w == b"\x1b\\");
            let bel = s.iter().position(|&b| b == 0x07);
            let (end, len) = match (st, bel) {
                (Some(e), Some(b)) if b < e => (b, 1),
                (Some(e), _) => (e, 2),
                (None, Some(b)) => (b, 1),
                (None, None) => return None,
            };
            Some((Some(Input::Osc(String::from_utf8_lossy(&s[2..end]).into_owned())), end + len))
        }
        b'\x1b' => Some((Some(Input::Key(Key::Esc)), 1)),
        b => Some((Some(Input::Key(Key::Alt(b as char))), 2)),
    }
}

fn mouse(params: &[u8], press: bool) -> Option<Input> {
    let text = std::str::from_utf8(params).ok()?;
    let mut n = text.split(';').map(|v| v.parse::<u32>().ok());
    let (b, x, y) = (n.next()??, n.next()??, n.next()??);
    let kind = if b & 64 != 0 {
        if b & 1 == 0 { MouseKind::WheelUp } else { MouseKind::WheelDown }
    } else if b & 32 != 0 {
        MouseKind::Move
    } else if press {
        MouseKind::Press
    } else {
        MouseKind::Release
    };
    Some(Input::Mouse(Mouse { kind, button: (b & 3) as u8, col: x, row: y }))
}

/// Presses, releases, moves (drags and hover) and wheel, as SGR reports in cells. Not in pixels (`?1016h`): herdr
/// before 0.9.4 answers that with cell numbers, which cannot be told apart from pixels; `cell_center` does what herdr
/// 0.9.4 and later do with a cell.
pub const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1003h\x1b[?1006h";
pub const MOUSE_OFF: &str = "\x1b[?1006l\x1b[?1003l\x1b[?1000l";

/// The pane's cells and pixels from `TIOCGWINSZ`; without pixels, 8 x 18 px cells; without a size, 80 x 24.
pub fn geometry() -> Geometry {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
    let (cols, rows) = if ok && ws.ws_col > 0 && ws.ws_row > 0 { (ws.ws_col, ws.ws_row) } else { (80, 24) };
    let (cell_w, cell_h) = if ok && ws.ws_xpixel > 0 && ws.ws_ypixel > 0 {
        (ws.ws_xpixel as u32 / cols as u32, ws.ws_ypixel as u32 / rows as u32)
    } else {
        (8, 18)
    };
    Geometry { cols, rows, cell_w: cell_w.max(1), cell_h: cell_h.max(1) }
}

/// Whether the pane reports its size in pixels yet. herdr sends it a moment after the pane starts; until then
/// `geometry` falls back to 8 x 18 px cells.
pub fn pixel_size_known() -> bool {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    (unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0) && ws.ws_xpixel > 0 && ws.ws_ypixel > 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarItem {
    /// The name in the tooltip.
    pub label: &'static str,
    /// The key that does the same, `""` when there is none.
    pub key: &'static str,
    /// One of `link::ACTIONS`, or a row's own: `more` (open the zoom row), `close` (leave the row),
    /// `case`, `word`, `regex`, `prev`, `next` (find row).
    pub action: &'static str,
    /// The item's symbol, `""` for a word alone.
    pub glyph: &'static str,
    /// The word after the symbol, `""` for a symbol alone.
    pub text: &'static str,
}

const fn item(label: &'static str, key: &'static str, action: &'static str, glyph: &'static str, text: &'static str) -> BarItem {
    BarItem { label, key, action, glyph, text }
}

/// The top bar's commands, left to right: `Back` first while there is a page to go back to (`can_go_back`, after
/// following a link to a file). The source button names what it switches to: `Source` while the page is rendered,
/// `View` while the source shows. Zoom and screenshot sit behind `…` (`more_items`).
pub fn bar_items(showing_source: bool, can_go_back: bool) -> Vec<BarItem> {
    let toggle = if showing_source {
        item("Show rendered", "v", "toggle", "\u{25c9}", "View")
    } else {
        item("Show source", "v", "toggle", "</>", "Source")
    };
    let back = can_go_back.then_some(item("Back", "h", "back", "\u{2190}", "Back"));
    back.into_iter()
        .chain([
            item("Reload", "r", "reload", "\u{27f3}", "Reload"),
            item("Find", "/", "find", "\u{2315}", "Find"),
            toggle,
            item("Open in browser", "o", "browser", "\u{2197}", "Browser"),
            item("More: zoom, screenshot", "", "more", "\u{2026}", "More"),
        ])
        .collect()
}

/// The row `…` opens in place of the bar: zoom out, zoom in (the percentage between them), reset, screenshot, close.
pub fn more_items() -> Vec<BarItem> {
    vec![
        item("Zoom out", "-", "zoom-out", "\u{2212}", ""),
        item("Zoom in", "+", "zoom-in", "+", ""),
        item("Reset zoom", "0", "zoom-reset", "", "Reset"),
        item("Screenshot", "s", "screenshot", "\u{2399}", "Screenshot"),
        item("Close", "Esc", "close", "\u{2715}", ""),
    ]
}

/// The find row's buttons, after the query field: the options, then previous, next and close.
pub fn find_items() -> Vec<BarItem> {
    vec![
        item("Match case", "Alt-c", "case", "Aa", ""),
        item("Whole word", "Alt-w", "word", "ab", ""),
        item("Regular expression", "Alt-r", "regex", ".*", ""),
        item("Previous match", "Shift-Enter", "prev", "\u{2191}", ""),
        item("Next match", "Enter", "next", "\u{2193}", ""),
        item("Close", "Esc", "close", "\u{2715}", ""),
    ]
}

/// "Reload (r)": an item's name and key, shown while the pointer is over it.
pub fn tooltip(item: &BarItem) -> String {
    if item.key.is_empty() {
        item.label.to_string()
    } else {
        format!("{} ({})", item.label, item.key)
    }
}

/// Symbols that common terminal fonts draw wider than their cell, so one space leaves them touching the word.
const WIDE_DRAWN: [&str; 2] = ["\u{27f3}", "\u{2315}"];

/// What an item shows, padded by a space on each side: symbol and word, or the symbol alone in a narrow pane.
fn cell(item: &BarItem, narrow: bool) -> String {
    match (item.glyph, item.text) {
        ("", text) => format!(" {text} "),
        (glyph, "") => format!(" {glyph} "),
        (glyph, _) if narrow => format!(" {glyph} "),
        (glyph, text) => {
            let gap = if WIDE_DRAWN.contains(&glyph) { "  " } else { " " };
            format!(" {glyph}{gap}{text} ")
        }
    }
}

/// Terminal columns a string takes: East Asian wide characters (Hangul, CJK, full-width forms) count two.
pub fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| match c as u32 {
            0x1100..=0x115f | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xff60 | 0xffe0..=0xffe6 => 2,
            _ => 1,
        })
        .sum()
}

/// `s` cut to at most `width` columns, with `~` when cut.
fn fit(s: &str, width: usize) -> String {
    cut(s, width, '~')
}

/// `s` cut to at most `width` columns, with `mark` at the end when cut.
fn cut(s: &str, width: usize, mark: char) -> String {
    if display_width(s) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        if display_width(&out) + display_width(&c.to_string()) + 1 > width {
            break;
        }
        out.push(c);
    }
    if width > 0 {
        out.push(mark);
    }
    out
}

/// The end of `s` in at most `width` columns, with `…` in front when cut: what a text field shows of a long entry.
fn fit_tail(s: &str, width: usize) -> String {
    if display_width(s) <= width {
        return s.to_string();
    }
    let mut tail: Vec<char> = Vec::new();
    for c in s.chars().rev() {
        if display_width(&tail.iter().collect::<String>()) + display_width(&c.to_string()) + 1 > width {
            break;
        }
        tail.push(c);
    }
    let kept: String = tail.into_iter().rev().collect();
    if width > 0 {
        format!("\u{2026}{kept}")
    } else {
        kept
    }
}

/// `items` laid out right-aligned on a `cols`-wide row after `left` (the title, or the hovered item's tooltip),
/// one column apart, with `extra` text (not an item) standing before item `extra.0`: the row text and each item's
/// 1-based column range.
fn render_row(
    items: &[BarItem],
    left: &str,
    cols: u16,
    hover: Option<usize>,
    extra: Option<(usize, &str)>,
) -> (String, Vec<(u16, u16)>) {
    const GAP: usize = 1;
    let cols = cols as usize;
    // what stands on the right: (item index or none, text)
    let segments = |narrow: bool| -> Vec<(Option<usize>, String)> {
        let mut out = Vec::new();
        for (n, it) in items.iter().enumerate() {
            if let Some((before, e)) = extra {
                if before == n {
                    out.push((None, e.to_string()));
                }
            }
            out.push((Some(n), cell(it, narrow)));
        }
        out
    };
    let width = |segs: &[(Option<usize>, String)]| {
        segs.iter().map(|(_, t)| display_width(t)).sum::<usize>() + GAP * segs.len().saturating_sub(1)
    };
    let wide = segments(false);
    let segs = if width(&wide) + 12 > cols { segments(true) } else { wide };
    let right = width(&segs);
    let left = match hover.and_then(|h| items.get(h)) {
        Some(item) => tooltip(item),
        None => left.to_string(),
    };
    let left = fit(&left, cols.saturating_sub(right + 2));
    let mut text = format!("\x1b[1;1H\x1b[2K {left}");
    let mut col = 2 + display_width(&left);
    let start = cols.saturating_sub(right) + 1;
    text.push_str(&" ".repeat(start.saturating_sub(col)));
    col = start.max(col);
    let mut ranges = Vec::new();
    for (i, (n, seg)) in segs.iter().enumerate() {
        if i > 0 {
            text.push_str(&" ".repeat(GAP));
            col += GAP;
        }
        let w = display_width(seg);
        if n.is_some() {
            ranges.push((col as u16, (col + w - 1) as u16));
        }
        if n.is_some() && *n == hover {
            text.push_str(&format!("\x1b[7m{seg}\x1b[27m"));
        } else {
            text.push_str(seg);
        }
        col += w;
    }
    (text, ranges)
}

/// Row-1 text (with its escape sequences) and the 1-based column range `(start, end)` of each bar item, for
/// hit-testing. The title (or, while hovering, the hovered item's tooltip) sits left; the items sit right.
pub fn render_bar(title: &str, showing_source: bool, can_go_back: bool, cols: u16, hover: Option<usize>) -> (String, Vec<(u16, u16)>) {
    render_row(&bar_items(showing_source, can_go_back), title, cols, hover, None)
}

/// The row `…` opens: `more_items`, with the zoom percentage between zoom out and zoom in.
pub fn render_more(title: &str, zoom: u32, cols: u16, hover: Option<usize>) -> (String, Vec<(u16, u16)>) {
    let pct = format!("{zoom}%");
    render_row(&more_items(), title, cols, hover, Some((1, &pct)))
}

/// A toast on row `row` (the pane's last): `text` in an inverted pill in the middle, like herdr's own "copied to
/// clipboard".
pub fn render_toast(text: &str, row: u16, cols: u16) -> String {
    let cols = cols as usize;
    let pill = format!(" {} ", fit(text, cols.saturating_sub(2)));
    let lead = cols.saturating_sub(display_width(&pill)) / 2;
    format!("\x1b[{row};1H\x1b[2K{}\x1b[7m{pill}\x1b[27m", " ".repeat(lead))
}

/// A link's destination for the status row in at most `width` columns: a `file://` URL as the path it names
/// (decoded, `shorten_path` when too long), anything else (`#anchor`, web and mail addresses) as written, its end
/// cut with `…` when too long.
pub fn status_text(href: &str, width: usize) -> String {
    // The text comes from the page and is drawn raw: a control character (`%1b` decodes to ESC) would be a terminal
    // command, so each becomes U+FFFD before anything measures or draws it.
    let plain = |s: &str| s.chars().map(|c| if c.is_control() { '\u{fffd}' } else { c }).collect::<String>();
    match href.strip_prefix("file://") {
        Some(rest) => shorten_path(&plain(&percent_decode(rest)), width),
        None => cut(&plain(href), width, '\u{2026}'),
    }
}

/// The status row: `text` in an inverted pill at the left of row `row` (the pane's last), the rest of the row
/// cleared.
pub fn render_status(text: &str, row: u16) -> String {
    format!("\x1b[{row};1H\x1b[2K\x1b[7m {text} \x1b[27m")
}

/// `s` with `%XX` escapes decoded (invalid ones stay as they are).
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `path` in at most `width` columns: whole when it fits, else its start, `…` and the file name
/// (`/tmp/peek-501/…/screenshots/a.png`), or only the file name's end when even that is too long.
pub fn shorten_path(path: &str, width: usize) -> String {
    if display_width(path) <= width {
        return path.to_string();
    }
    let (dir, file) = path.rsplit_once('/').map_or(("", path), |(d, f)| (d, f));
    let tail = format!("\u{2026}/{file}");
    let room = width.saturating_sub(display_width(&tail));
    if dir.is_empty() || room < 2 {
        return fit_tail(file, width);
    }
    let mut head = String::new();
    for c in dir.chars() {
        if display_width(&head) + display_width(&c.to_string()) > room {
            break;
        }
        head.push(c);
    }
    format!("{head}{tail}")
}

/// The find row: its text, the column range of each `find_items` button, and where the caret goes (`None` while
/// a tooltip fills the field).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindRow {
    pub text: String,
    pub buttons: Vec<(u16, u16)>,
    pub caret: Option<u16>,
}

/// The find row: `⌕`, the query field (underlined; `Find` in dim when empty; the hovered button's tooltip in its
/// place while hovering), the options (inverted when on), the position (`3/22`, `No results`, `Bad regex`), and
/// previous, next and close.
pub fn render_find(q: &FindQuery, count: i64, index: i64, cols: u16, hover: Option<usize>) -> FindRow {
    let items = find_items();
    let status = match count {
        c if c < 0 => "Bad regex".to_string(),
        0 if q.text.is_empty() => String::new(),
        0 => "No results".to_string(),
        c => format!("{}/{c}", index.rem_euclid(c) + 1),
    };
    let on = |action: &str| match action {
        "case" => q.case,
        "word" => q.word,
        "regex" => q.regex,
        _ => false,
    };
    // " ⌕ " + field + " " + options (3 x 4) + " " + status (10) + " " + buttons (3 x 3)
    let cols_n = cols as usize;
    let right_w = 12 + 1 + 10 + 1 + 9;
    let field_w = cols_n.saturating_sub(3 + 1 + right_w).max(1);
    let (field, caret) = match hover.and_then(|h| items.get(h)) {
        Some(item) => (format!("\x1b[2m{}\x1b[22m", pad(&fit(&tooltip(item), field_w), field_w)), None),
        None if q.text.is_empty() => (format!("\x1b[2m{}\x1b[22m", pad(&fit("Find", field_w), field_w)), Some(4)),
        None => {
            let shown = fit_tail(&q.text, field_w.saturating_sub(1));
            let caret = 4 + display_width(&shown) as u16;
            (pad(&shown, field_w), Some(caret))
        }
    };
    let mut text = format!("\x1b[1;1H\x1b[2K \u{2315} \x1b[4m{field}\x1b[24m ");
    let mut col = 3 + field_w + 2;
    let mut buttons = Vec::new();
    for (n, item) in items.iter().enumerate() {
        if n == 3 {
            text.push_str(&format!(" {status:<10} "));
            col += 12;
        }
        let word = format!(" {} ", item.glyph);
        let w = display_width(&word);
        buttons.push((col as u16, (col + w - 1) as u16));
        let lit = on(item.action) || hover == Some(n);
        text.push_str(&if lit { format!("\x1b[7m{word}\x1b[27m") } else { word });
        col += w;
    }
    FindRow { text, buttons, caret }
}

/// `s` padded with spaces to `width` columns.
fn pad(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(display_width(s))))
}

/// The terminal cursor for row 1: a blinking bar at `caret` (the find field), or hidden.
pub fn caret(caret: Option<u16>) -> String {
    match caret {
        Some(col) => format!("\x1b[1;{col}H\x1b[5 q\x1b[?25h"),
        None => "\x1b[?25l".to_string(),
    }
}

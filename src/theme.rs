//! The page's colours and code font, from the terminal the viewer runs in.
//!
//! Colours: the viewer asks the terminal (`query`) for its foreground, background and 16 palette colours; herdr
//! answers with the outer terminal's. `PageTheme::from_terminal` turns the answers into the page palette, with
//! today's built-in Catppuccin Mocha / Latte for whatever is not answered. Design:
//! `docs/superpowers/specs/2026-11-18-peek-terminal-theme-and-fonts-design.md`.

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// `a` moved `pct` percent of the way to `b`, per channel, rounded.
pub fn mix(a: Rgb, b: Rgb, pct: u32) -> Rgb {
    let m = |x: u8, y: u8| ((x as u32 * (100 - pct) + y as u32 * pct + 50) / 100) as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// herdr's rule for a light background.
pub fn is_light(c: Rgb) -> bool {
    299 * c.0 as u32 + 587 * c.1 as u32 + 114 * c.2 as u32 >= 128_000
}

/// A colour as terminals answer it: `rgb:R/G/B` with 1 to 4 hex digits per channel, or `#rrggbb`.
pub fn parse_color(s: &str) -> Option<Rgb> {
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return Some(Rgb(ch(0)?, ch(2)?, ch(4)?));
    }
    let parts: Vec<&str> = s.strip_prefix("rgb:")?.split('/').collect();
    let channel = |p: &str| -> Option<u8> {
        if p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let max = (1u32 << (4 * p.len())) - 1;
        let v = u32::from_str_radix(p, 16).ok()?;
        Some(((v * 255 + max / 2) / max) as u8)
    };
    match parts[..] {
        [r, g, b] => Some(Rgb(channel(r)?, channel(g)?, channel(b)?)),
        _ => None,
    }
}

/// What the terminal answered.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TermColors {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub ansi: [Option<Rgb>; 16],
}

impl TermColors {
    pub fn answered(&self) -> usize {
        usize::from(self.fg.is_some()) + usize::from(self.bg.is_some()) + self.ansi.iter().flatten().count()
    }

    /// Every colour `query` asks for.
    pub fn complete(&self) -> bool {
        self.answered() == 18
    }
}

/// Records one OSC answer (`10;<colour>`, `11;<colour>`, `4;<n>;<colour>`); false for anything else.
pub fn take_answer(colors: &mut TermColors, osc_body: &str) -> bool {
    let Some((code, rest)) = osc_body.split_once(';') else { return false };
    let (slot, colour) = match code {
        "10" => (&mut colors.fg, rest),
        "11" => (&mut colors.bg, rest),
        "4" => {
            let Some((n, colour)) = rest.split_once(';') else { return false };
            match n.parse::<usize>() {
                Ok(n) if n < 16 => (&mut colors.ansi[n], colour),
                _ => return false,
            }
        }
        _ => return false,
    };
    match parse_color(colour) {
        Some(c) => {
            *slot = Some(c);
            true
        }
        None => false,
    }
}

/// Foreground, background and the 16 palette colours, then the primary device attributes, which every terminal
/// answers: an answer to it with no colour answer means none will come.
pub fn query() -> String {
    let mut q = String::from("\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
    for n in 0..16 {
        q.push_str(&format!("\x1b]4;{n};?\x1b\\"));
    }
    q.push_str("\x1b[c");
    q
}

/// Everything a page's look needs.
#[derive(Clone, Debug, PartialEq)]
pub struct PageTheme {
    pub dark: bool,
    /// Page background.
    pub base: Rgb,
    /// Code block background.
    pub mantle: Rgb,
    /// Inline code, borders, table lines.
    pub surface0: Rgb,
    /// The marked source line.
    pub surface1: Rgb,
    pub text: Rgb,
    /// Quotes, footnotes, the fetch line.
    pub subtext0: Rgb,
    /// Line numbers, the quote bar.
    pub overlay0: Rgb,
    /// Links and headings.
    pub blue: Rgb,
    /// Find marks: every match (translucent), the current one, and the current one's text.
    pub find: Rgb,
    pub current: Rgb,
    pub current_text: Rgb,
    /// Source highlighting; `None` uses syntect's `base16-ocean` for `dark`.
    pub syntax: Option<syntect::highlighting::Theme>,
    /// Font families put before peek's own for code and source text.
    pub code_fonts: Vec<String>,
}

impl PageTheme {
    /// Catppuccin Mocha (dark) or Latte (light), https://github.com/catppuccin/palette.
    pub fn builtin(dark: bool) -> PageTheme {
        let c = |hex: u32| Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        let [base, mantle, surface0, surface1, text, subtext0, overlay0, blue, find, current_text] = if dark {
            [0x1e1e2e, 0x181825, 0x313244, 0x45475a, 0xcdd6f4, 0xa6adc8, 0x6c7086, 0x89b4fa, 0xf9e2af, 0x11111b]
        } else {
            [0xeff1f5, 0xe6e9ef, 0xccd0da, 0xbcc0cc, 0x4c4f69, 0x6c6f85, 0x9ca0b0, 0x1e66f5, 0xdf8e1d, 0xeff1f5]
        }
        .map(c);
        PageTheme {
            dark,
            base,
            mantle,
            surface0,
            surface1,
            text,
            subtext0,
            overlay0,
            blue,
            find,
            current: find,
            current_text,
            syntax: None,
            code_fonts: Vec::new(),
        }
    }

    /// The page palette from the terminal's answers; `system_dark` decides dark or light when the background was not
    /// answered, and with neither foreground nor background the built-in palette is used whole.
    pub fn from_terminal(colors: &TermColors, system_dark: bool, code_fonts: Vec<String>) -> PageTheme {
        let dark = colors.bg.map_or(system_dark, |bg| !is_light(bg));
        let builtin = PageTheme { code_fonts, ..PageTheme::builtin(dark) };
        if colors.fg.is_none() && colors.bg.is_none() {
            return builtin;
        }
        let bg = colors.bg.unwrap_or(builtin.base);
        let fg = colors.fg.unwrap_or(builtin.text);
        let (find, current, current_text) = match colors.ansi[3] {
            Some(yellow) => (yellow, yellow, bg),
            None => (builtin.find, builtin.current, builtin.current_text),
        };
        PageTheme {
            dark,
            base: bg,
            mantle: if dark { mix(bg, Rgb(0, 0, 0), 20) } else { mix(bg, fg, 5) },
            surface0: mix(bg, fg, 12),
            surface1: mix(bg, fg, 22),
            text: fg,
            subtext0: mix(bg, fg, 78),
            overlay0: mix(bg, fg, 45),
            blue: colors.ansi[4].unwrap_or(builtin.blue),
            find,
            current,
            current_text,
            ..builtin
        }
    }
}

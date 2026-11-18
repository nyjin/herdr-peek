# Page colours and fonts from the terminal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** peek's pages take their colours from the terminal (OSC 10/11/4 answers through herdr) and, under Ghostty,
their code font from Ghostty's configuration; both are read at start and on reload only.

**Architecture:** A new platform-neutral module `src/theme.rs` turns colour answers and `ghostty +show-config` text
into a `PageTheme` (today's Mocha/Latte when nothing is answered). `src/page.rs` builds pages from a `PageTheme`
instead of its fixed palettes. `src/term.rs` recognises the answers so they never become key presses, and the viewer
asks at start and on reload, waiting at most 300 ms.

**Tech Stack:** Rust 2021, syntect 5 (themes built in code), pulldown-cmark, objc2 / dispatch2 (viewer, macOS).

**Spec:** `docs/superpowers/specs/2026-11-18-peek-terminal-theme-and-fonts-design.md`

## Global Constraints

- Everything in the repository is English (code, comments, UI strings, docs, commit messages); Korean only in `*.ko.md`
  and in tests where it does a job, with an English comment saying why. `cargo test` (with `tests/language.rs`) passes.
- Platform-neutral code lives in `src/` and builds on Linux CI; macOS-only code in `src/mac/`.
- No new dependencies.
- Colour answers are awaited at most 300 ms; `ghostty +show-config` gets at most 1 s.
- Dark or light: `299 R + 587 G + 114 B >= 128000` is light (herdr's rule).
- Mixes, in percent of the second colour, rounded as `(a * (100 - p) + b * p + 50) / 100` per channel:
  `mantle` = bg toward black 20 (dark) or toward fg 5 (light); `surface0` 12, `surface1` 22, `overlay0` 45,
  `subtext0` 78 (bg toward fg).
- Find marks: `ansi[3]` at alpha `.35` (dark) or `.3` (light); current match `ansi[3]` on `bg`.
- Highlighting: comments `overlay0` italic; strings `ansi[2]`; numbers and constants `ansi[3]`; keywords and storage
  `ansi[5]`; functions `ansi[4]`; types `ansi[6]`; tags and invalid `ansi[1]`.
- Ghostty fonts go in front of `"JetBrains Mono", Menlo, monospace` for `code, pre, body.source` only; body text and
  size unchanged.
- No live switching: `CSI ? 997 ; n n` is dropped; peek never turns on mode 2031.
- Commits follow Conventional Commits; nothing is pushed.

## Review Focus

- A terminal that answers foreground and background but no palette (OSC 4 unsupported; herdr skips host palette
  queries under WSL): page colours from fg/bg, links and find marks from the built-in palette, highlighting from
  `base16-ocean`. Test in Task 2 and Task 3.
- Colour answers in other forms: `rgb:ff/ff/ff`, `rgb:f/f/f`, `rgb:FFFF/0000/8080`, `#RRGGBB`, and forms to ignore
  (`rgba:…`, garbage): parsed or ignored, never a panic. Test in Task 2.
- An unsolicited `CSI ? 997 ; 2 n` (appearance switched while the viewer is open) and answers split across reads or
  mixed with typing: no key is produced, the page does not change. Test in Task 1.
- Font names from Ghostty with `"`, `\` or `</style>` in them: escaped, the page's style block stays whole. Test in
  Task 4.
- `ghostty` missing, failing or hanging at `GHOSTTY_BIN_DIR`: an empty list within the 1 s limit. Test in Task 4.

---

### Task 1: Terminal answers in the input parser

**Files:**
- Modify: `src/term.rs` (`Input` at :42, `escape` at :104)
- Test: `tests/term.rs`

**Interfaces:**
- Produces: `Input::Osc(String)` — the body between `ESC ]` and `ESC \` or BEL; `Input::Attributes` — a DA1 answer
  (`CSI ? … c`).

- [ ] **Step 1: Write the failing tests** in `tests/term.rs`:
  - `parses_colour_answers`: `parse(b"\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]4;1;rgb:f3/8b/a8\x07j")` returns
    `[Osc("10;rgb:cdcd/d6d6/f4f4"), Osc("4;1;rgb:f3/8b/a8"), Key(Char('j'))]` and uses every byte.
  - `colour_answer_split_across_reads`: `parse(b"\x1b]11;rgb:1e1e/1e")` returns no inputs and `used == 0`; the full
    sequence then parses to one `Osc`.
  - `device_attributes_and_scheme_reports_are_not_keys`: `parse(b"\x1b[?62;22c\x1b[?997;2nr")` returns
    `[Attributes, Key(Char('r'))]` (the scheme report dropped).
- [ ] **Step 2: Run** `cargo test --test term` — expected: the three new tests fail.
- [ ] **Step 3: Implement** in `escape`: a `b']'` arm that ends at `ESC \` (`end + 2`) or BEL (`end + 1`),
  whichever comes first, `None` (incomplete) when neither is in; in the `b'['` arm, a body starting with `?` and final
  byte `c` gives `Input::Attributes`. Update the `Input` doc comments.
- [ ] **Step 4: Run** `cargo test --test term` — expected: all pass. Run `cargo build` — the viewer's `match input`
  (`src/mac/viewer.rs:1082`) fails to compile until Task 6; add arms that ignore `Osc` and `Attributes` for now.
- [ ] **Step 5: Commit** `fix(viewer): read terminal answers as answers, not key presses`.

### Task 2: Terminal colours and the page palette

**Files:**
- Create: `src/theme.rs`; Modify: `src/lib.rs` (`pub mod theme;`)
- Test: `tests/theme.rs`

**Interfaces:**
- Produces:
  - `pub struct Rgb(pub u8, pub u8, pub u8)` (Copy, Eq, Debug) with `pub fn hex(self) -> String` (`"#rrggbb"`).
  - `pub fn mix(a: Rgb, b: Rgb, pct: u32) -> Rgb` (the Global Constraints formula).
  - `pub fn is_light(c: Rgb) -> bool`.
  - `pub fn parse_color(s: &str) -> Option<Rgb>`: `rgb:R/G/B` with 1 to 4 hex digits per channel, scaled to 8 bits
    (`v * 255 / (16^n - 1)`), or `#rrggbb`; anything else `None`.
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct TermColors { pub fg: Option<Rgb>, pub bg: Option<Rgb>,
    pub ansi: [Option<Rgb>; 16] }` with `pub fn answered(&self) -> usize` and `pub fn complete(&self) -> bool`
    (all 18).
  - `pub fn take_answer(colors: &mut TermColors, osc_body: &str) -> bool`: records `10;…`, `11;…`, `4;n;…` (n < 16);
    false for anything else.
  - `pub fn query() -> String`: OSC 10 `?`, OSC 11 `?`, OSC 4 `;n;?` for n = 0..15 (each ended by `ESC \`), then
    `ESC [ c`.
  - `#[derive(Clone, Debug, PartialEq)] pub struct PageTheme { pub dark: bool, pub base: Rgb, pub mantle: Rgb,
    pub surface0: Rgb, pub surface1: Rgb, pub text: Rgb, pub subtext0: Rgb, pub overlay0: Rgb, pub blue: Rgb,
    pub find: Rgb, pub current: Rgb, pub current_text: Rgb, pub syntax: Option<syntect::highlighting::Theme>,
    pub code_fonts: Vec<String> }`.
  - `PageTheme::builtin(dark: bool) -> PageTheme`: today's Mocha / Latte values from `src/page.rs:76-106`, `syntax`
    `None`, `code_fonts` empty.
  - `PageTheme::from_terminal(colors: &TermColors, system_dark: bool, code_fonts: Vec<String>) -> PageTheme`.

- [ ] **Step 1: Write the failing tests** in `tests/theme.rs`, with a helper `mocha() -> TermColors` holding fg
  `#cdd6f4`, bg `#1e1e2e` and Ghostty's Catppuccin Mocha palette (0 `#45475a`, 1 `#f38ba8`, 2 `#a6e3a1`, 3 `#f9e2af`,
  4 `#89b4fa`, 5 `#f5c2e7`, 6 `#94e2d5`, 7 `#bac2de`, 8 `#585b70`, 9 `#f7aec2`, 10 `#c2ecbf`, 11 `#fcd682`,
  12 `#aeccfc`, 13 `#f398da`, 14 `#b1eae1`, 15 `#a6adc8`):
  - `colour_forms`: `rgb:cdcd/d6d6/f4f4`, `rgb:cd/d6/f4` and `#CDD6F4` give `Rgb(0xcd,0xd6,0xf4)`; `rgb:f/0/8` gives
    `Rgb(255,0,136)`; `rgba:1/2/3/4`, `rgb:12/34`, `nonsense` give `None`.
  - `answers_fill_the_colours`: `take_answer` with `10;rgb:cdcd/d6d6/f4f4`, `11;#1e1e2e`, `4;15;rgb:a6/ad/c8` sets
    fg, bg, `ansi[15]`; `4;16;…` and `12;…` return false; `answered() == 3`, not `complete()`.
  - `query_asks_eighteen_colours_then_attributes`: `query()` has 16 `\x1b]4;` and ends with `\x1b[c`.
  - `light_or_dark`: `is_light(#1e1e2e)` and `is_light(#303446)` false, `is_light(#eff1f5)` true.
  - `mocha_answers_give_about_todays_page`: `from_terminal(&mocha(), false, vec![])` has `dark` true, base
    `#1e1e2e`, text `#cdd6f4`, mantle `#181825`, surface0 `#333446`, surface1 `#45465a`, overlay0 `#6d7187`,
    subtext0 `#a7aec8`, blue `#89b4fa`, find and current `#f9e2af`, current_text `#1e1e2e`.
  - `fg_and_bg_without_palette`: only fg/bg of `mocha()` → blue and find from `builtin(true)`.
  - `nothing_answered_is_builtin`: `from_terminal(&TermColors::default(), false, vec![])` equals `builtin(false)`.
- [ ] **Step 2: Run** `cargo test --test theme` — expected: compile failure (no `peek::theme`).
- [ ] **Step 3: Implement** `src/theme.rs` with the items above. `dark` is `!is_light(bg)` when bg is answered, else
  `system_dark`; each missing colour comes from `builtin(dark)`; mixes use the resolved bg and fg.
- [ ] **Step 4: Run** `cargo test --test theme` — expected: pass.
- [ ] **Step 5: Commit** `feat(theme): page palette from the terminal's colour answers`.

### Task 3: Source highlighting in the terminal's colours

**Files:**
- Modify: `src/theme.rs`
- Test: `tests/theme.rs`

**Interfaces:**
- Consumes: `TermColors`, `Rgb`, `PageTheme::from_terminal` (Task 2).
- Produces: `pub fn syntax_theme(colors: &TermColors, text: Rgb, comment: Rgb) -> Option<syntect::highlighting::Theme>`
  — `None` unless `ansi[1]` to `ansi[6]` are all answered; `from_terminal` stores it in `PageTheme::syntax` (with
  `text` = the page text, `comment` = `overlay0`).

- [ ] **Step 1: Write the failing tests**:
  - `syntax_theme_uses_the_terminal_colours`: highlight `fn main() { let s = "x"; // note\n}` as Rust (syntect's
    default syntaxes, `HighlightLines`, `styled_line_to_highlighted_html` with `IncludeBackground::No`) with
    `from_terminal(&mocha(), …).syntax`: the HTML contains `color:#f5c2e7;">fn` (keyword), `color:#a6e3a1;` (string),
    and `font-style:italic;color:#6d7187;` (comment).
  - `no_palette_no_syntax_theme`: fg/bg only → `syntax` is `None`.
- [ ] **Step 2: Run** `cargo test --test theme` — expected: the new tests fail.
- [ ] **Step 3: Implement** `syntax_theme`: `ThemeSettings { foreground: text, .. }` and one `ThemeItem` per row of the
  Global Constraints highlighting list, scopes `comment`; `string`; `constant`; `keyword, storage`;
  `entity.name.function, support.function`; `entity.name.type, entity.name.class, support.type, support.class`;
  `entity.name.tag, invalid` (`ScopeSelectors::from_str`). No `storage.type` in the types row: syntect's Rust grammar
  scopes `fn` as `storage.type.function.rust` and `let` as `storage.type.rust`, which must stay keyword-coloured.
- [ ] **Step 4: Run** `cargo test --test theme` — expected: pass.
- [ ] **Step 5: Commit** `feat(theme): source highlighting in the terminal's sixteen colours`.

### Task 4: Code fonts from Ghostty

**Files:**
- Modify: `src/theme.rs`
- Test: `tests/theme.rs`

**Interfaces:**
- Produces:
  - `pub fn ghostty_fonts(show_config: &str) -> Vec<String>`: the values of `font-family = …` lines in order (not
    `font-family-bold` and the like), trimmed, surrounding quotes removed; an empty value clears the list so far.
  - `pub fn fonts_from(term_program: Option<&str>, bin_dir: Option<&Path>, limit: Duration) -> Vec<String>`: when
    `term_program` is `ghostty` in any case and `bin_dir` is given, runs `<bin_dir>/ghostty +show-config` (stdout
    piped, stderr null), kills it past `limit`, parses stdout; else or on failure an empty list.
  - `pub fn read_code_fonts() -> Vec<String>`: `fonts_from(env TERM_PROGRAM, env GHOSTTY_BIN_DIR, 1 s)`.
  - `pub fn css_code_fonts(families: &[String]) -> String`: each family as a CSS string (`\` → `\\`, `"` → `\"`,
    `<` → `\3c `), joined by `, `, followed by `"JetBrains Mono", Menlo, monospace`.

- [ ] **Step 1: Write the failing tests**:
  - `fonts_from_show_config`: text with `font-family = Jetendard`, `font-family-bold = X`,
    `font-family = JetBrains Mono Hangul`, `font-size = 14` → `["Jetendard", "JetBrains Mono Hangul"]`; a later
    `font-family = ` clears; `font-family = "Fira Code"` → `Fira Code`.
  - `css_font_list_is_escaped`: `css_code_fonts(&["A\"B".into(), "</style>".into()])` contains `"A\"B"` and
    `"\3c /style>"` and no `</style>`, and ends with `"JetBrains Mono", Menlo, monospace`;
    `css_code_fonts(&[])` is `"JetBrains Mono", Menlo, monospace`.
  - `fonts_from_ghostty_binary`: a temp dir with an executable `ghostty` shell script printing
    `font-family = Jetendard` → `fonts_from(Some("Ghostty"), Some(dir), 1 s)` is `["Jetendard"]`;
    `fonts_from(Some("WezTerm"), Some(dir), …)` and `fonts_from(Some("ghostty"), None, …)` are empty.
  - `hanging_ghostty_is_cut_off`: the script sleeps 5 s first → empty list, returned in under 2 s.
  - `missing_ghostty_is_empty`: a `bin_dir` without the file → empty.
- [ ] **Step 2: Run** `cargo test --test theme` — expected: the new tests fail.
- [ ] **Step 3: Implement** the four functions; the limit is a `try_wait` loop (10 ms steps) with `kill` on time-out.
- [ ] **Step 4: Run** `cargo test --test theme` — expected: pass.
- [ ] **Step 5: Commit** `feat(theme): code font from Ghostty's configuration`.

### Task 5: Pages built from a `PageTheme`

**Files:**
- Modify: `src/page.rs` (remove `Palette`, `MOCHA`, `LATTE`, `palette()` at :58-115; `PageOptions` at :34;
  `template`, `common_css`, `find_css`, `html_file_page`, `source_page`)
- Modify: `src/mac/viewer.rs` (`page_options` at :265, `background` at :271) — only enough to compile:
  `theme: Arc::new(PageTheme::builtin(v.dark))`
- Test: `tests/page.rs` (helpers `opts` at :11 and the literals at :292 and :306)

**Interfaces:**
- Consumes: `PageTheme`, `css_code_fonts` (Tasks 2-4).
- Produces: `#[derive(Clone, Debug)] pub struct PageOptions { pub theme: std::sync::Arc<PageTheme>, pub font_px: f32,
  pub mode: Mode, pub line: Option<u32> }` (no longer `Copy`).

- [ ] **Step 1: Write the failing tests** in `tests/page.rs` (helpers build `PageOptions` with
  `Arc::new(PageTheme::builtin(dark))`; `theme_follows_flag` keeps its assertions):
  - `terminal_colours_and_font_reach_the_page`: a markdown and a source page with
    `PageTheme::from_terminal(&<Mocha answers>, false, vec!["Jetendard".into()])` contain `--surface0: #333446`, a
    `code, pre, body.source { font-family: "Jetendard", "JetBrains Mono"` rule, and a `body {` rule without
    `Jetendard`.
  - `source_page_uses_the_terminal_highlighting`: a `.rs` page with that theme contains `color:#a6e3a1;` for
    `let s = "x";`.
  - `html_file_find_marks_follow_the_theme`: an `.html` page with that theme carries `rgba(249, 226, 175, .35)`.
- [ ] **Step 2: Run** `cargo test --test page` — expected: compile failure, then the new tests fail.
- [ ] **Step 3: Implement**: `template`, `common_css`, `find_css` and `html_file_page` read `opts.theme` (CSS
  variables from `Rgb::hex`, `color-scheme` from `dark`, find colour as `rgba(r, g, b, .35|.3)`); the code rule uses
  `css_code_fonts(&theme.code_fonts)`; `source_page` uses `theme.syntax` or `THEMES` `base16-ocean.dark` / `.light`.
- [ ] **Step 4: Run** `cargo test` — expected: everything passes (`tests/page.rs`, `tests/theme.rs`, the rest).
- [ ] **Step 5: Commit** `refactor(page): build pages from a PageTheme`.

### Task 6: The viewer asks at start and on reload

**Files:**
- Modify: `src/mac/viewer.rs` (`Viewer` fields, `run` at :113, `page_options`, `background`, `open_when_measured`
  at :293, `act("reload")` at :618, `read_input` at :1051)
- Modify: `README.md`, `README.ko.md` (a short "Colours and fonts" note under "Keys and the top bar"),
  `docs/superpowers/specs/2026-11-15-peek-webkit-viewer-design.md` (the 2026-11-15 decision "Theme follows the system"
  gets "(since 2026-11-18: the terminal's colours, see 2026-11-18-peek-terminal-theme-and-fonts-design.md)"; the
  page section names `src/theme.rs`)

**Interfaces:**
- Consumes: `theme::{query, take_answer, TermColors, PageTheme, read_code_fonts}`, `Input::{Osc, Attributes}`.
- Produces (private): `Viewer.system_dark: bool` (was `dark`), `Viewer.theme: Arc<PageTheme>`,
  `Viewer.asking: Option<Asking>` with `struct Asking { colors: TermColors, attributes: bool, fonts: Option<Vec<String>>,
  then: Vec<Box<dyn FnOnce()>> }`; `fn ask_terminal(then: impl FnOnce() + 'static)`, `fn on_osc(body: String)`,
  `fn on_attributes()`, `fn finish_asking(force: bool)`.

- [ ] **Step 1: Implement asking**: `ask_terminal` pushes `then` onto a running `Asking` or starts one: `out(&query())`,
  a thread running `read_code_fonts()` that hands its result to the main queue, and `after(300, || finish_asking(true))`.
  `on_osc` → `take_answer`; `on_attributes` sets `attributes`; each then calls `finish_asking(false)`, which finishes
  when fonts are in and (`complete()` or `attributes && answered() == 0`), or always when forced (fonts not in yet:
  the previous theme's `code_fonts`). Finishing sets `theme = Arc::new(PageTheme::from_terminal(..))`, logs
  `{"ev": "theme", "answered": n, "dark": d, "fonts": [...]}` and runs every `then` in order.
- [ ] **Step 2: Wire it**: `run` calls `ask_terminal(|| {})` after `terminal_on()`; `open_when_measured` also waits
  while `asking.is_some()` (same 50 ms steps, same 20-try limit); `act("reload")` runs today's reload inside
  `ask_terminal(move || …)`; `read_input` routes `Osc` and `Attributes`; `page_options` passes `v.theme.clone()`;
  `background` returns `v.theme.base`.
- [ ] **Step 3: Run** `cargo build` and `cargo test` — expected: no warnings, all pass.
- [ ] **Step 4: Docs**: the README notes (English and Korean, same content): colours follow the terminal, code text
  uses Ghostty's font, a theme change shows after `r`. Update the 2026-11-15 design doc as listed. Run
  `cargo test --test language`.
- [ ] **Step 5: Commit** `feat(viewer): colours and code font from the terminal, read again on reload`.
- [ ] **Step 6: By hand, with the user** (herdr ≥ 0.9.3 under Ghostty; the user's herdr is 0.9.0, so a separate
  0.9.5 session as in `evals/results.md`): dark page matches Mocha; switch macOS to light, page unchanged, `r` →
  Frappe colours; code in Jetendard; `r` pressed twice quickly reloads once per press without stray scrolling;
  `viewer.log` shows the `theme` lines. Record the result in `evals/results.md` and commit
  `docs(evals): terminal colours and fonts checked by hand`.

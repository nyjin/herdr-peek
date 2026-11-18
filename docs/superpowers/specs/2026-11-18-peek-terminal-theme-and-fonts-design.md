# peek: page colours and fonts from the terminal — Design

> 2026-11-18. Builds on the current design (`2026-11-15-peek-webkit-viewer-design.md`); once built, that document's
> theme and font lines are updated to match.

## Intent

The viewer page should look like the terminal it sits in: the user's terminal colours and, under Ghostty, the
terminal's font. Today peek has two fixed palettes (Catppuccin Mocha and Latte, `src/page.rs`), picks one from macOS's
appearance when the viewer starts, and uses its own font list. With a terminal whose light theme is Catppuccin Frappe
(the user's Ghostty: `theme = dark:Catppuccin Mocha,light:Catppuccin Frappe`, font `Jetendard`, then
`JetBrains Mono Hangul`), the light page (Latte) and the terminal beside it differ.

## User decisions (2026-11-18)

- Colours come from the terminal through escape-sequence queries; Ghostty's configuration is read for the font only
  (approach 1 below).
- No live switching: a theme change while a viewer is open is not followed. Reload (`r`, the bar's Reload, the herdr
  action `peek: reload`) reads the colours and the font again and applies them.
- herdr's `[theme]` is not used.

## Goals, non-goals, anti-goals

Goals:
- Page background, text, links, headings, borders and find highlights from the terminal's colours.
- Source highlighting in the terminal's 16 colours.
- Under Ghostty, code and source text in Ghostty's `font-family`.
- Without answers or without Ghostty: today's look, unchanged.

Non-goals:
- Following a theme change while the viewer is open.
- Fonts from other terminals (kitty, WezTerm): their configuration is not read.
- herdr's `[theme]` (it colours herdr's sidebar and bars; it has no page background).
- HTML files' own styles (they keep them) and the top bar (terminal text already in the terminal's colours).
- Body text font: the prose font stays as it is (Ghostty's font is a fixed-width terminal font).

Anti-goals:
- A colour reply read as key presses (today it would be: see Findings).
- A first page that is visibly slower to appear.
- Breaking when the terminal answers nothing.

## Findings this rests on

- herdr (since before 0.9.0) answers a pane's OSC 10 / OSC 11 queries with the host terminal's default colours and
  forwards OSC 4 palette queries to the host terminal (herdr #1759); `CSI ? 996 n` is answered with
  `CSI ? 997 ; 1 n` (dark) or `; 2 n` (light). Measured in a pane of herdr 0.9.0 under Ghostty 1.3.1 in dark mode:
  foreground `#cdd6f4`, background `#1e1e2e`, palette 0, 1, 4 and 15 equal to Ghostty's `Catppuccin Mocha` theme
  file; all answers, then the DA1 answer (`CSI ? 62 ; 22 c`), arrived at once and in the order asked. The pane was
  opened without focus (`herdr pane split --no-focus`), as the viewer is, and the answers came back to it.
- herdr's client asks the host terminal for its colours again when the host reports a colour-scheme change
  (`HostColorSchemeChanged` sets `query_host_theme` in herdr's `src/client/shell/input.rs`), and for the appearance
  when the outer window gains focus. So after an appearance change, a later query is answered with the new colours
  without restarting herdr (read in herdr 0.9.5's source, not measured). Inside herdr a pane always gets colour
  answers: without host colours, herdr's engine answers with its own defaults, which are also what the pane shows.
- herdr's own terminal engine answers XTVERSION (`libghostty`), so a query cannot name the outer terminal. Pane
  environments carry `TERM_PROGRAM=ghostty`, `GHOSTTY_RESOURCES_DIR` and `GHOSTTY_BIN_DIR`, inherited from where the
  herdr server was started (stale if a client later attaches from another terminal). Seen in a shell pane; plugin
  panes are started with extra variables on top of the same environment (`plugin_pane_launch_env` in herdr's
  `src/app/api/plugins/panes.rs`), not seen in one yet.
- `ghostty +show-config` prints the effective configuration (about 20 ms), with one `font-family = <name>` line per
  configured family. Its colours are not resolved for the current appearance (it showed the light theme's colours
  while macOS was dark), so colours do not come from it.
- `term::parse` (`src/term.rs`) reads `ESC ]` as `Alt(']')` and the rest of an OSC reply as key presses: `r` reloads,
  `g` and `b` scroll.
- Reload of a file goes through `open` and rebuilds the page (`src/mac/viewer.rs`, `act("reload")`); the first page
  already waits up to a second for the pane's pixel size (`open_when_measured`).

## Approaches

1. **Colours from the terminal, Ghostty's configuration for the font (chosen).** Exactly the colours on screen, under
   any terminal that answers the queries. Costs: the input parser must handle replies, and building waits briefly for
   them at start and on reload.
2. Colours and font from Ghostty's configuration files. No terminal round trip, but Ghostty only, and peek would have
   to resolve `theme = dark:…,light:…` and find theme files as Ghostty does; `+show-config` cannot be used for that
   (above).
3. herdr's `[theme]` mapped to built-in palettes. herdr's theme colours its own sidebar and bars and has no page
   background; peek would carry 17 palettes; the user's herdr configuration does not set a theme.

## Design

### Querying the terminal

- One query: OSC 10 `?`, OSC 11 `?`, OSC 4 `;n;?` for n = 0..15, then DA1 (`CSI c`). OSC strings end with `ESC \`.
- Sent when the viewer starts (before the first page) and on reload.
- The answers are collected until all 18 colour answers are in, or 300 ms have passed. DA1 shortens the wait only for
  a terminal that answers no colour at all: when the DA1 answer comes and no colour answer has come, stop. (herdr
  forwards OSC 4 to the host and answers DA1 itself, so DA1 may come before late palette answers; the measured order
  was the asked order.)
- At start this wait runs alongside the existing wait for the pixel size, not after it.
- Answers accepted: `ESC ] 10 ; <colour>`, `ESC ] 11 ; <colour>`, `ESC ] 4 ; <n> ; <colour>`, ended by `ESC \` or BEL,
  where `<colour>` is `rgb:R/G/B` with 1 to 4 hex digits per channel (scaled to 8 bits) or `#rrggbb`.

### Input parser (`src/term.rs`)

- `ESC ]` … `ESC \` or BEL becomes `Input::Osc(<body>)`; incomplete, it waits for more bytes like other sequences.
- A DA1 answer (`CSI ? … c`) becomes `Input::Attributes`.
- `CSI ? 997 ; n n` (a colour-scheme report herdr may send) is dropped: no live switching. peek does not turn on mode
  2031.
- No reply produces a key.

### Page colours (`src/theme.rs`, platform-neutral, new)

`TermColors { fg, bg, ansi: [Option<Rgb>; 16] }` from the answers, and the page palette derived from it:

| Page colour (today's name) | Used for | From |
|---|---|---|
| `base` | page background | `bg` |
| `text` | text | `fg` |
| `mantle` | code block background | `bg` mixed with a little `fg` |
| `surface0` | inline code, borders, table lines | `bg` mixed with more `fg` |
| `surface1` | the marked source line | `bg` mixed with more `fg` |
| `overlay0` | line numbers, quote bar | about half way |
| `subtext0` | quotes, footnotes, fetch line | mostly `fg` |
| `blue` | links, headings | `ansi[4]` |
| `find`, `current`, `current_text` | find marks | `ansi[3]` translucent, `ansi[3]`, `bg` |
| `color-scheme` | form controls, scroll bars | dark or light, below |

- Dark or light: from the background's luminance, herdr's rule (`299 R + 587 G + 114 B >= 128000` is light).
  Without a background answer: macOS's appearance, as today.
- A missing answer takes that colour from today's palette for the chosen side; without `fg` and `bg`, today's palette
  whole.
- Mix ratios are set by eye so that Mocha answers give about today's Mocha page.

### Source highlighting

A syntect `Theme` built in code from the 16 colours (in the spirit of bat's `ansi` theme, with exact RGB):
foreground `fg`; comments `overlay0` (the page's mix of `bg` and `fg`), italic; strings `ansi[2]`; numbers and
constants `ansi[3]`; keywords and storage `ansi[5]`; functions `ansi[4]`; types `ansi[6]`; tags `ansi[1]`; invalid
`ansi[1]`. Without the 16 colours: today's `base16-ocean` themes.

Comments do not use `ansi[8]`: in the user's Mocha it is `#585b70`, too dark to read on `#1e1e2e` (user decision,
2026-11-18).

### Font (Ghostty)

- Ghostty when `TERM_PROGRAM` is `ghostty` (any case) and `GHOSTTY_BIN_DIR` is set: run
  `$GHOSTTY_BIN_DIR/ghostty +show-config` with a 1 s limit and take the `font-family = ` values in order; an empty
  value clears the list so far, as in Ghostty's configuration.
- The families go in front of today's code list: `code, pre, body.source { font-family: "<family>", …, "JetBrains
  Mono", Menlo, monospace }`, each name quoted and escaped for CSS.
- Body text and font size stay as they are (size from the cell height, and zoom).
- Not Ghostty, the command failing or no family: today's list.

### Where it applies

- Every page of peek's own template: markdown, source, image, fetched PR and error pages, and the copy opened in the
  browser (`o`).
- HTML files keep their own styles; only the find highlight colours change.
- The screenshot background (`background()` in the viewer) is the page background.

### When it is read

- At start and on reload only. Rebuilds for other reasons (the file saved, back, source toggle, zoom, resize) use the
  last values.

## Tests

- `term::parse`: OSC answers ended by `ESC \` and by BEL, split across reads, mixed with keys and mouse reports; DA1;
  `CSI ? 997 ; 1 n` dropped; none of them yields a key.
- `theme`: colour formats (`rgb:` with 1 to 4 digits, `#rrggbb`); the derived palette; the dark and light rule; missing
  answers falling back; the syntect theme colouring a short Rust snippet with the expected colours.
- Font: `+show-config` text with several `font-family` lines, an empty value, names with quotes; CSS escaping.
- `page`: a page built with given colours and font carries them; without them it is today's page.
- By hand, in herdr under Ghostty: dark (Mocha) and light (Frappe); switch the appearance, see the page unchanged, press
  `r`, see it follow; code and source text in Ghostty's font (Jetendard); a setting that answers no colours falls back
  to today's look.
- `viewer.log` records, at start and on each reload, which colours were answered and which font families were read,
  so the hand test (and later reports) can tell a fallback from a match.

## Left to the builder

- The exact mix ratios and syntect scope selectors.
- How the colours and font reach `PageOptions` (it is `Copy` today).
- Whether `+show-config` runs off the main thread (it takes about 20 ms).

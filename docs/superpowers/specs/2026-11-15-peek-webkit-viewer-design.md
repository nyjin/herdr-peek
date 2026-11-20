# peek: WebKit viewer in one Rust binary — Design

> Current design for peek from 2026-11-15 (this repository; until 2026-11-17 the plugin lived in nyjin/herdr-skills under
> `herdr-plugins/peek`, where the versions 0.1.0 to 0.6.0 named below were released; releases here start again at 0.1.0). It replaces the glow/bat design
> (`2026-11-13-glow-preview-design.md`, kept as the record of the earlier versions). Measurements behind it:
> `evals/spike/README.md`.

## Purpose

A click on a path or link in a Claude Code pane shows it, rendered like a web page, in the pane to the right of that
Claude pane: markdown, source code, images, HTML, and GitHub PR content (Jira and Confluence too until 2026-11-18:
"Jira and Confluence removed" below). Nothing has to be installed besides the plugin itself (the old version needed glow, bat, acli, jira-cli, cme and gh).

## User decisions (2026-11-15)

- Render inside the herdr pane; a window outside the pane is not a plugin. A light browser engine (macOS's own WebKit,
  hidden) renders; the frames are shown with Kitty graphics. Not Electron, not a terminal-text renderer.
- The whole plugin is one Rust binary. Python, glow and bat go.
- Text must stay sharp: frames use the pane's native pixels.
- Controls: keys in the viewer, herdr actions, and a top bar; hovering a top bar item shows a tooltip with its
  shortcut.
- Find works like an editor's: match case, whole word, regular expression.
- Remote images in local documents always load.
- A changed file is redrawn automatically (file change events, not polling).
- Links inside a rendered page: a local file opens in peek (with back); a web address opens in the default browser.
- File types: markdown, source, images, SVG and HTML. PDF only if it renders well (left out of 0.5.0).
- Jira, Confluence and GitHub PR content is fetched through each service's API with a token that the user registers
  with `peek auth`, and shown in the pane. (Jira and Confluence were removed on 2026-11-18.)
- Screenshots go under `/tmp` (layout below).
- Theme follows the system (dark/light); the page's font size follows the terminal's; zoom 50-200 % in 10 % steps,
  remembered. (Since 2026-11-18 the colours are the terminal's and the code font Ghostty's:
  `2026-11-18-peek-terminal-theme-and-fonts-design.md`.)
- Page scripts are off; HTML inside markdown is shown but never runs. Mermaid and math show as code blocks for now.
- Scope of this round: implementation, tests and commits. No PR, merge or release.

## What goes, what stays

Goes (from the 2026-11-13 design and the README): glow, bat and the `brew install` step; acli, jira-cli, cme, gh and
the `claude -p` MCP fallback; the "do not open a browser" non-goal (there is now an open-in-browser command); the less
menu and its keys (`s`, `f`, `m`); `~/.cache/peek` documents as markdown; Python and its tests.

Stays: one viewer per Claude pane, opened with `herdr plugin pane open --placement split --target-pane <source>
--direction right --no-focus`; the state file per source pane (`~/.local/state/peek/<source>.json`) and the
`.viewer` file; nothing opens without a click (0.6.0: or a selection); the herdr action that toggles rendered/source; path resolution
(below, moved here unchanged).

## Requirements

- macOS (WebKit through objc2). herdr ≥ 0.9.3 (`link_handlers`, `plugin pane open --env`; standard Kitty graphics
  in panes since 0.9.2). Kitty graphics are on by default in herdr 0.9.5 (`[terminal] kitty_graphics`).
- A terminal with Kitty graphics outside herdr (Ghostty).
- No other tool. A GitHub token is needed only for PR links.

## Binary and plugin layout

```
herdr-peek/                (repository root)
  herdr-plugin.toml        panes, actions and link handlers run bin/peek
  bin/peek                 launcher: runs target/release/peek; if it is missing, prints how to build it
  Cargo.toml, src/         the crate
  tests/                   cargo integration tests (platform-neutral)
  evals/                   spike and screenshots (not shipped behaviour)
```

Subcommands: `peek link` (link handler action), `peek action <name>` (herdr actions: toggle, reload, find, zoom-in,
zoom-out, zoom-reset, screenshot, browser, back; and select-to-open, the switch), `peek viewer` (the pane), `peek auth
github` (token registration); since 0.6.0 `peek watch [--spawn]` (the select-to-open watcher, or start it detached)
and `peek select <text>` (one selection through select to open, as the watcher would; for tests and by hand).

Modules. Platform-neutral ones build and test on Linux (CI is ubuntu); macOS-only ones sit behind
`#[cfg(target_os = "macos")]` with their crates under `[target.'cfg(target_os = "macos")'.dependencies]`:

| Module | Job | Platform |
|---|---|---|
| `resolve` | clicked text → candidates (rules below) | neutral |
| `select` | select to open: clipboard text → source pane → files ("Select to open") | neutral |
| `watcher` | the select-to-open watcher: clipboard polling, lock, lifetime; the pasteboard reader is in `mac/clipboard` | neutral loop, macOS pasteboard |
| `target` | classify: PR, path with `:line` | neutral |
| `theme` | the terminal's colour answers and Ghostty's font → page colours, source highlighting, code font | neutral |
| `state` | state files, viewer bookkeeping, zoom preference, `/tmp` layout | neutral |
| `page` | markdown/source/image/HTML/API content → page HTML (template, theme, CSP) | neutral |
| `fetch` | GitHub REST calls, link cache | neutral (HTTP client) |
| `herdr` | calls to the herdr CLI | neutral |
| `render` | hidden `WKWebView`: load, readiness, capture, zoom, find, hit-test | macOS |
| `kitty` | tiles, shared memory transmission, placements | neutral encoding, macOS shared memory |
| `term` | raw mode, keys, SGR mouse (cells), resize, top bar text | neutral |
| `viewer` | the event loop tying them together | macOS |
| `watch` | file change events (FSEvents) | macOS |
| `keychain` | token store | macOS |

Release download (a `peek-v<version>` tag never marked latest, checksum pinned in the repo) is future work; for now
the binary is built in the checkout with `cargo build --release`.

## Flow

1. A click matches a link handler → herdr runs `peek link` with `HERDR_PLUGIN_CLICKED_URL` and `HERDR_PANE_ID`.
2. `peek link` classifies the text, resolves a path (below) or keeps the URL, writes the state file and opens the
   viewer pane if the source pane has none. Focus stays on Claude.
3. The viewer sees the state change, shows `Loading <name> ...` as terminal text, builds the page, loads it in the
   hidden WebKit view, captures and shows it. Keys and clicks that arrive before the page is ready wait for it.

### Path resolution (on a link click; unchanged from the 2026-11-13 design)

The clicked string is judged after stripping `file://`, a trailing `:line[:col]`, and quotes, brackets and punctuation.

1. Absolute path or `~/`: open it right away if the file exists.
2. Any other path: collect candidates from the four sources below all at once. The order does not affect the result.
   - Working folder: join onto the `foreground_cwd` of the clicked pane (not `cwd`; in a session that moved into a
     worktree, `cwd` points at the old location).
   - scratchpad: join onto `/private/tmp/claude-<uid>/*/<session id>/scratchpad`. The session id is the pane's
     `agent_session.value`. A leading `scratchpad/` is dropped before joining.
   - Transcript: absolute paths found in `~/.claude/projects/*/<session id>.jsonl` whose tail matches.
   - git: files (tracked and untracked, ignored files excluded) of the repository that `foreground_cwd` belongs to
     whose tail matches.
   - A string that starts with `…/` is used only for tail matching.
3. Remove duplicates by real file (realpath). One candidate opens; several show a selection list in the viewer (as
   terminal text, the most recent in the transcript first); none gives a herdr notification "file not found".

## The page

- Markdown: pulldown-cmark with tables, strikethrough, task lists and footnotes. Source: syntect with the onig engine
  (line numbers; the clicked line marked and scrolled to). Images and SVG: an image element on the page background.
  HTML files: loaded as they are, scripts still off.
- Template: the terminal's colours and, under Ghostty, its font for code (`theme`; asked at start and on reload),
  Catppuccin Mocha (dark) or Latte (light) when the terminal does not answer; the base font size follows the
  terminal cell height in CSS pixels; the find highlight styles (`::highlight(peek-find)`, `::highlight(peek-cur)`)
  are in the template from the start (inserted later, the first capture misses them).
- CSP: `default-src 'none'; img-src * data: file:; style-src 'unsafe-inline'; font-src data: file:`. Page scripts
  off through `WKWebpagePreferences.allowsContentJavaScript = false`; peek's own scripts run through
  `evaluateJavaScript`.
- Page files go to `/tmp/peek-<uid>/<project>/<session>/pages/`. The read-access root given to `loadFileURL` must
  cover both the page and the document's folder (relative images fail otherwise): use `/`.
- Ready when the navigation's own marker (a query on the page URL) is current and `document.readyState` is
  `complete`. Images that finish after the first capture trigger a re-capture of the screen.

## Showing it in the pane

- Kitty graphics written to the viewer's own terminal; herdr renders them. Transmission through POSIX shared memory
  (`t=s`). Temp files are accepted only from `$TMPDIR`; inline base64 is far too slow for whole screens.
- Native pixels (scale 1.0) always.
- Byte budget per image, not a fixed size: one image costs about 91 ms on screen at 12 MB and 318 ms at 21.5 MB
  (two data points; 67 MB was refused). So the document is captured in tiles of at most 12 MB (tile height =
  budget / (pane pixel width × 4)), possibly less than a screen. The visible screen is shown as one placement per
  visible tile, each with a source rect inside its tile (a source rect past an image's end stretches the picture).
- Scrolling moves placements only (measured 34-40 ms key-to-screen, the same as terminal text). Tiles around the view
  are captured ahead while idle; tiles far away are deleted (`a=d`).
- Resize: re-measure (cells and pixels from `TIOCGWINSZ`), re-lay out, keep the reading position by the element at
  the top of the screen, drop and rebuild the tiles.
- The top row is terminal text (the top bar); images start on the second row.

## Input

Changed 2026-11-20 (text selection and link hover; design: `2026-11-19-peek-text-selection-and-link-hover-design.md`).

- Raw mode. Mouse reports in SGR cells: `?1000h ?1003h ?1006h` (presses, releases, drags and moves, wheel); a
  report's cell stands for the pixel at its centre (`term::cell_center`), so the pointer moves in steps of a cell.
  Until 2026-11-20 peek asked for pixels (`?1016h`), but herdr before 0.9.4 answers that with cell numbers, which
  cannot be told apart from pixels (found in the hand test); herdr 0.9.4 and later take a cell's centre the same way.
  `gesture` tells a click, a drag, a double-click (the system's double-click interval) and hover apart.
- Wheel scrolls.
- A click (press and release in one cell) follows a link on release, so a drag that starts on a link does not open
  it. The pixel becomes a document point (CSS px: device px / (device pixel ratio × page zoom), plus the view's top);
  peek's hit script scrolls that point into the web view and takes `elementFromPoint(...).closest('a')`. A local file
  opens in the same viewer and pushes history; a web address goes to `open`; `#anchor` scrolls.
- A drag selects from the press to the pointer; a double-click selects a word (`Intl.Segmenter`, Korean included; at
  a word's end, that word). Releasing copies the rendered text as plain text, as Safari copies it (the source view:
  the file's lines without line numbers), with a pasteboard mark that select to open skips, and a toast
  `● Copied to clipboard` stands for 2 s. The selection is drawn as translucent strips over the tiles in herdr's own
  selection colour; `Esc` (when neither the find row nor the `…` row is open), a click and another page take it away;
  zoom and a pane resize keep it.
- Hovering a link shows where a click would go on the last row (the status row: a file's path, a web address, an
  `#anchor`), in an inverted pill like the toast's, at the row's left end; the page draws one row shorter meanwhile.
  A toast wins over it; while the button is held it gives way to the page; while the pointer is on that row it does
  not change, and a click there follows the link it shows.
- Pointer scripts (hit tests, selection) run one at a time, and never between a capture's or a screenshot's scroll
  and its snapshot.
- Default keys: `j`/`k`/↑/↓ a few lines, `space`/`b`/PgDn/PgUp a screen, `g`/`G` top/bottom, `/` find (keys on the
  find row: "Top bar"), `r` reload, `v` rendered/source (was `t` until 2026-11-17), `+`/`-`/`0` zoom in/out/reset,
  `s` screenshot, `o` open in browser, `h`/Backspace back, `q` close.
- The same commands exist as herdr actions, so they work from the Claude pane too.

## Top bar

One text row: file name, then `⟳ Reload` · `⌕ Find` · `</> Source` · `↗ Browser` · `… More`, one column apart
(changed 2026-11-17 at the user's request: a symbol and a word per item, this order, zoom and screenshot behind
`…`). The source button names what it switches to: `</> Source` while the page is rendered, `◉ View` while the
source shows. `… More` turns the row into: `−` · percentage · `+` · `Reset` · `⎙ Screenshot` · `✕`; `Esc` or `✕`
returns to the bar, and zooming leaves it open for the next click. Hovering an item shows a tooltip with its name and
key (`Reload (r)`). Clicking runs it. A narrow pane shows the symbols without the words. A dropdown under `…` was not built: text over the page picture needs the
picture below the cell layer, which herdr's rendering of Kitty placements was not checked for.

While finding, the row becomes the find row, laid out after editors' find widgets (VS Code, Chrome): `⌕`, an
underlined field with the terminal cursor (a blinking bar) after the query and a dim `Find` while empty, `Aa` `ab`
`.*`, the position (`3/22`, `No results`, `Bad regex`), `↑` `↓` `✕`. Hovering a button shows its tooltip in the
field. While the row is open, keys go to the field as in an editor's focused find box: `Enter` and Down step to the
next match, `Shift-Enter` (`CSI 13;2u` or `CSI 27;2;13~`) and Up to the previous one, Backspace edits, `Esc` closes;
PgUp, PgDn, Home, End and the wheel scroll the page. Placing tiles moves the cursor, so every draw puts it back.

## Find

App-injected script, so it works with page scripts off. Matches are marked with the CSS Custom Highlight API (the
DOM is untouched): all matches lightly, the current one strongly. Options: match case, whole word, regular
expression (JavaScript syntax). Whole word and regular expression exclude each other, as in IntelliJ's and Xcode's
find (a pattern sets its own word bounds); match case goes with either (`FindQuery::toggle`). Measured 11-23 ms for a search, about 2 ms to step.

## Zoom

`pageZoom`, 50-200 % in 10 % steps, remembered in the state folder; the element at the top of the screen stays at the
top. Measured re-layout 38-40 ms plus capture.

At zoom Z % one CSS px is Z / 100 view points, while scroll positions, the document's height and client rects stay in
CSS px: every conversion between device px (or rows) and CSS px uses the device pixel ratio × the zoom (fixed
2026-11-20; before, tiles after the first left gaps or overlapped at any zoom but 100 %). WebKit stops a scroll at
whole CSS px and whole points, short of a fractional position, so at zooms other than 100 % the web view is 4 pt
taller than a tile and a capture keeps the whole tile in view.

## Screenshot

The current screen as PNG, also copied to the clipboard. A toast in the middle of the pane's last row (an inverted
pill, like herdr's own "copied to clipboard") gives the path for 5 s, the middle of a long path cut so the file name
stays; a click on it takes it away sooner. Row 1 would hide the bar, and the page picture covers text, so the page
draws one row shorter while the toast stands. No herdr notification (user requests, 2026-11-17).

```
/tmp/peek-<uid>/                         0700, created and checked to be ours (not a link, owned by the user)
└── <project>/                           Claude's project name for the folder (as in ~/.claude/projects)
    └── <session>/                       the Claude session id of the source pane; pane-<id> if it has none
        ├── screenshots/<YYYYMMDD-HHMMSS>-<document>-L<line>.png
        ├── browser/<document>.html      the page handed to the browser by "open in browser"
        └── pages/                       pages the viewer loads
```

`/tmp` may be emptied on restart; that is accepted.

## Open in browser

A local document: write its page with absolute `file://` references to `browser/` and run `open`. A PR view:
open the original URL.

## File changes

Subscribe to file change events (FSEvents) for the shown file's folder; react to the file (writes, atomic renames).
Debounce 300 ms; reload and keep the reading position.

## GitHub PR

| Link | Call | Page |
|---|---|---|
| `https://github.com/<o>/<r>/pull/<n>` | `GET /repos/<o>/<r>/pulls/<n>`, issue and review comments, `Accept: application/vnd.github.html+json` | title, state, `body_html`, comments |

- Token: `peek auth github` (token, Bearer), stored in the macOS Keychain. peek does not read other tools'
  credentials.
- Fetched pages are cached in `~/.cache/peek/links/` with the fetch time; `r` fetches again.
- Failures show a page with the cause and the fix: no token (`peek auth ...`), 401/403, 404, timeout.

## Tests

- `cargo test` covers the platform-neutral modules: resolution (the old Python cases ported), classification, page
  building, find query building, tile and placement arithmetic, Kitty command encoding, REST requests against a local
  fake server, state and `/tmp` layout.
- `cargo test` runs everything, the language rule included (`tests/language.rs`); CI on ubuntu
  builds the neutral modules only.
- The viewer is checked by hand and with the spike's harness in a herdr 0.9.5 session (key-to-screen, two viewers,
  resize, mouse); results go to `evals/`.

## Left to the implementer

The exact keys for the find toggles, tooltip placement, tile budget tuning, the font size formula, error page wording,
and the HTTP client crate.

## Select to open (0.6.0, decided 2026-11-16)

herdr hands plugins no click on a path printed as plain text (see "What shipped" below), and no selection or copy
event either (`src/api/schema/events.rs` lists pane, tab, workspace and worktree events only). What herdr does do: when
a mouse selection ends, by drag or double-click, it copies the text to the system clipboard (`[ui] copy_on_select`, on
by default; `pbcopy` on macOS). peek uses that copy as its signal.

User decisions: web addresses and hyperlinks keep Ctrl+click; paths open by selecting them (drag or double-click).
Nothing changes what agents or programs print, and nothing sits between herdr and the pane's shell. If herdr starts
matching plugin link handler patterns against plain text (asked in herdr discussions #4465, #4771 and #5010; #609
asks for plain-text paths in general), paths open by Ctrl+click through the existing link handlers, and select to
open is reconsidered then (off by default or removed).

Checked by hand in a herdr 0.9.5 session (2026-11-16): a drag copied `2026-11-15-repo-layout-and-language-design.md`
from a shell pane; a double-click copied `docs/superpowers/specs/` plus the Korean particle U+C5D0 written right after
it, from a Claude Code pane (herdr's word selection splits at spaces, and Korean attaches particles without one). In
both cases the system clipboard changed and the text was on that pane's screen. herdr did not move focus to the pane
the selection was made in. `copy_on_select` and `[[startup]]` hooks exist in herdr 0.9.3, so the minimum stays.

### The watcher

`peek watch` is a background process, one per herdr server.

- Started detached by a `[[startup]]` hook (`peek watch --spawn`, which forks and returns) and, so that a fresh
  `herdr plugin link` needs no restart, by every `peek link`, `peek action` and `peek viewer` run when none is running.
  One per server: a lock file in `~/.local/state/peek/` keyed by `HERDR_SOCKET_PATH`. It exits when the server's
  socket stops answering twice in a row, 5 s apart (a live handoff reruns the startup hook under the new server).
- Every 150 ms it reads the pasteboard's change count (`NSPasteboard.changeCount`), not its contents. Only when the
  count changed does it read the string, and only when select to open is on.
- A change that carries peek's own mark (pasteboard type `local.peek.copy`, written with the viewer's text selection
  copies since 2026-11-20) is skipped, so a path copied in the viewer never opens through select to open. herdr's
  next copy clears the mark, as any write to the pasteboard does.
- The text is kept in memory for the checks below and never written anywhere; the watcher's `watch.log` in the state
  folder records what it opened and its errors only.

### Which pane, which file

1. The text is one line, 1 to 1024 characters, without control characters. A path the program itself broke over
   two lines (Claude Code wraps long lines with real line breaks) is therefore not opened; selecting the part on one
   line, such as the file name, still works.
2. The panes of the tab shown in the focused workspace (`herdr pane list`), except peek's viewer panes, are read
   (`herdr pane read --source visible`). A pane matches when the text is in one of its rows, or in its rows joined
   without line breaks (a long line the terminal wrapped; to be checked in the integration test). The focused pane wins when several match, otherwise the first.
   No match: the copy came from elsewhere (a browser, an editor) and nothing happens.
3. The text is resolved for the matched pane like a click (path resolution above), with two additions:
   - **Trailing characters.** When the text as selected resolves to nothing, up to 4 characters are dropped from its
     end, one at a time, and the longest form that resolves wins. Nothing is judged by script, so Korean folder and
     file names open as selected; a path followed by a one- or two-syllable particle opens without it (`kitty.rs:58`
     plus a particle opens line 58); a file whose name really ends in what looks like a particle still opens as
     selected.
   - **Path shape.** The form that resolves must contain a `/`, a file extension (`.md`, `.rs` ...) or a `:line`.
     Selection has no link handler pattern in front of it, so without this a double-click on a word in prose would
     open whatever file the word names: `peek` matches `bin/peek` by its tail, `peeking` would
     trim to it, `LICENSE` exists. Those stay no-ops; a bare extensionless name opens by Ctrl+click on a hyperlink
     only.
   Folders open nothing in this round (TODO below).
   Binary files other than images (a NUL byte in the first 8 KiB) are skipped. Nothing found: nothing happens, no
   notification (most selections are not paths).
4. One result opens in the viewer beside the matched pane without taking focus, as a click does; several show the
   numbered list.

### Turning it off

On by default. The herdr action `peek: select to open on/off` (`select-to-open`) toggles it; the choice is kept with
the zoom in `~/.local/state/peek/`. The README says what it relies on and what it cannot see:

- `copy_on_select` must stay on.
- Selecting the same text twice in a row opens nothing the second time: herdr does not copy text the clipboard
  already holds.
- A client attached over SSH copies to its own machine's clipboard, which peek cannot see.
- Selecting a path only to copy it also opens it (without focus).

Open item: newer macOS versions may ask the user before an app reads the pasteboard without a paste. macOS 15.6 asked
nothing in the hand check; later versions are not checked.

Checked with a throwaway plugin in the separate session (2026-11-16): herdr leaves a child of an action running after
the action ends (it is reparented to launchd), and records the action as succeeded, but only when the child does not
hold the action's stdout and stderr: herdr reads both pipes to the end (`finish_plugin_child` in
`src/app/api/plugins/runtime.rs`), so a child that keeps them leaves the action "running" until it exits. `peek watch
--spawn` therefore starts the watcher with stdin, stdout and stderr on `/dev/null` in a process group of its own, and
returns. Startup hooks run through the same code.

TODO (not in 0.6.0, user decision 2026-11-16): a selected folder opens a folder page (folders first, then files, by
name, with size and time; `..`; capped; entries open in peek; redrawn on change). Left out because everything else
peek shows is a file.

### Tests

- Neutral (`select` module): the text filter; pane matching (row, wrapped rows, several panes, focus preference, no
  match); trailing-character trimming; the path-shape gate (`peek`, `peeking`, `LICENSE` open nothing); resolution
  against temporary folders with Korean folder and file names in both NFC and NFD form, particles after files and
  lines, a selected folder opening nothing; binary skip; the watcher's decision loop driven by
  a fake clipboard (change count and string) and fake screens.
- macOS: the pasteboard reader; the lock and the exit on a dead socket.
- Integration in a separate herdr 0.9.5 session: a selection by drag and by double-click opens the viewer beside the
  pane it was made in (made by hand, as no API makes a herdr selection), plus `pbcopy` of on-screen and off-screen
  text for the timing (copy to first change) and the no-op cases.

## What shipped (0.5.0, 2026-11-15)

Built as designed, in the order of `docs/superpowers/plans/2026-11-15-peek-webkit-viewer-plan.md` (Tasks 2-4 by
three workers in parallel, the viewer in one stream). Integration test and timings: `evals/results.md`
(key to screen 44-52 ms against 38 ms for terminal text; click to the viewer's first change about 120 ms).

Differences from the design above:

- HTML files get `script-src 'none'; object-src 'none'` instead of the page CSP, so their own stylesheets, images and
  fonts load; scripts stay off (also in the browser copy).
- Markdown headings get GitHub-style ids (`#the-end`), so in-page links work.
- Jira comments come from a second call (`/rest/api/3/issue/<KEY>/comment?expand=renderedBody`), paged; GitHub
  comments are paged too. GitHub needs a token for public repositories as well.
- Images on the same Atlassian site are downloaded with the token, attachments or not.
- The first page waits for the pane's pixel size (herdr reports it a moment after the pane starts); the page's font
  size is the cell height in CSS px / 1.35.
- The viewer checks the state file's modification time every 30 ms; a command already in the file when the viewer
  starts is skipped.
- The file watch reacts only when the file's modification time or size differs.
- In a narrow pane the top bar shows the keys instead of the words. `Enter` in the find field also steps to the next
  match. `=` zooms in as well as `+`.
- Each viewer writes `viewer.log` (timings, errors) in its session folder.
- PDF is not handled (it shows as text); no time was spent on it in this round.
- Clicks reach peek only as herdr hands them over (checked against herdr 0.9.5 on 2026-11-16, `src/app/actions.rs`
  `url_from_link_target`): with Control held, on OSC 8 hyperlinks (any target) and on `http(s)` addresses in plain
  text. A path printed as plain text, which is how Claude Code prints paths in a herdr pane, is not clickable; herdr
  does not detect paths in plain text (herdr discussion #609). Ways around it that change what agents print or put a
  relay between herdr and the shell were rejected: the first changes the user's output, the second hides the pane's
  foreground process from herdr's agent detection and working-folder tracking. Paths open by selection instead:
  "Select to open" above.

## What shipped (0.6.0, 2026-11-17)

Select to open as designed above, plus the bar and find changes asked for while trying it (sections "Top bar",
"Find" and "Screenshot" describe the result). Test record: `evals/results.md`.

Differences from the design:

- `peek select <text>` runs one selection the way the watcher does, for the integration tests (through the fake
  herdr) and by hand.
- The watcher also exits when the peek binary is replaced (checked with the server, every 5 s), so a rebuild or a
  plugin update does not leave an old watcher running; the next peek run starts the new one. `$PEEK_WATCH_CHECK_MS`
  shortens the check for the lifetime test.
- The rendered/source key is `v` (was `t`); the screenshot toast stands on the last row, which the page gives up
  while it shows.

## Own repository (2026-11-17)

peek moved out of nyjin/herdr-skills into this repository, with the history of `herdr-plugins/peek` (the glow-preview
history before the rename stays there). The plugin is the repository root, so `herdr plugin link` takes the checkout.
Checks are `cargo test` alone: the language rule became `tests/language.rs` (no Python). Releases are made by
semantic-release on every push to `main` (`CLAUDE.md`, "Releases"); a `v0.0.0` tag on the initial commit makes the
first release 0.1.0.

## Jira and Confluence removed (2026-11-18)

User decision: take Jira and Confluence out now and review them again later. GitHub PRs stay.

How it came to this: looking at the approach again, the Atlassian path (REST API with a token from
`peek auth atlassian <site>`) had never been run with a real token (`evals/results.md`: "fetching with real tokens
(none registered)"), and no token was registered. After trying a click without a token in a separate herdr session,
the user decided to take it out and review it later.

Now: an Atlassian address is a plain web address. A Ctrl+click on one goes to the browser like any other address;
selecting one opens nothing (select to open takes paths only). `peek auth atlassian` is no longer accepted. A viewer
whose state still holds an Atlassian link shows "… is not a GitHub PR link."

Removed: the `jira` and `confluence` link handlers, `Target::Jira` and `Target::Confluence`, `src/fetch/jira.rs`,
`src/fetch/confluence.rs`, the Atlassian client, token and auth prompt, and the download of images that need the token
(`localize` and its helpers in `src/fetch/mod.rs`, the attachment folder in the link cache), with their tests and
fixtures. All of it is in the history before this change.

For the review:

- Approach as built: Jira `GET /rest/api/3/issue/<KEY>?expand=renderedFields&fields=summary,status,assignee,description,comment`
  plus paged `GET /rest/api/3/issue/<KEY>/comment?expand=renderedBody`; Confluence
  `GET /wiki/api/v2/pages/<id>?body-format=view`. Both answer with rendered HTML, which suits the WebKit page. Basic
  auth with the account email and an API token, per site, in the Keychain (`atlassian:<host>`).
- The earlier way (2026-11-13 design): acli, jira-cli, cme, then the Atlassian MCP through `claude -p`. acli answers
  with ADF (a converter is needed), cme took 1.7-6 s and up to 121 s for a page, MCP 13-30 s and costs usage.
- Link shapes in past sessions (about 400 distinct Atlassian addresses): about 180 `/browse/<KEY>`, about 120
  `/wiki/spaces/<space>/pages/<id>/…`; the old patterns covered these. Not covered: tiny links (`/wiki/x/…`),
  `/wiki/pages/viewpage.action?pageId=<id>` and space home pages, a few each.
- API tokens appear to be allowed on the site in use (another tool there works with one); a site whose admins block
  API tokens would need OAuth (acli, MCP) instead.
- Not checked here: a token created with scopes is said to work only through `api.atlassian.com/ex/...`, not the
  site's own address that peek called; Atlassian API tokens are said to expire (at most a year). The 401 page covered
  an expired token.

## Text selection, copy and link hover (2026-11-20)

Design: `2026-11-19-peek-text-selection-and-link-hover-design.md`; plan:
`docs/superpowers/plans/2026-11-20-peek-text-selection-and-link-hover.md`. Built as designed ("Input" above
describes the result); hand test record: `evals/results.md`.

Differences from that design, and what building it found:

- The hit test and the selection script take a document point and scroll it into the web view themselves. Scrolled
  to the view's top, a point missed near the document's end (the view cannot scroll that far) and in panes taller
  than one tile (the web view is one tile tall).
- Page zoom was wrong in the viewer's arithmetic at any zoom but 100 % (it converted with the device pixel ratio
  alone); fixed as described in "Zoom".
- The status row never moves the page: the scroll limit counts the bar and a toast, not the status row.
- `prefetch` waits while the button is held and starts again after any release.

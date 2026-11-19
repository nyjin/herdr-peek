# Text selection, copy on release and link hover — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the viewer, a drag selects text and a double-click selects a word, letting go copies the rendered text
(with a 2 s toast), the selection is drawn as translucent strips over the page in herdr's selection colour, and
hovering a link shows its destination on the last row.

**Architecture:** Platform-neutral pieces, each tested by `cargo test`: `src/gesture.rs` turns mouse reports into
press, word, extend, copy, click and hover; `src/overlay.rs` draws selection rectangles into one-row RGBA strips and
works out which strips to send, place and delete; `src/script.rs` gains the selection script; `src/term.rs` the
status row; `src/watcher.rs` skips peek's own copies. `src/mac/viewer.rs` wires them: pointer scripts run one at a
time and never during a capture, strips are placed at `z=1` with every draw, link clicks move to the release.

**Tech Stack:** Rust 2021, objc2 / objc2-app-kit / objc2-web-kit, dispatch2 (viewer, macOS); WebKit's
`caretRangeFromPoint`, `Range.getClientRects`, `Selection.toString` and `Intl.Segmenter` inside peek's scripts.

**Spec:** `docs/superpowers/specs/2026-11-19-peek-text-selection-and-link-hover-design.md`

## Global Constraints

- Everything in the repository is English; Korean only in `*.ko.md` and in tests where it does a job, with an English
  comment saying why. `cargo test` (with `tests/language.rs`) passes on Linux and macOS.
- Platform-neutral code in `src/` builds on Linux CI (no objc in `gesture`, `overlay`, `script`, `term`, `watcher`);
  macOS-only code in `src/mac/`. No new dependencies.
- Page scripts stay off. Every script has the shape `(function (a) { ... })(<JSON>)`: a constant body, everything from
  outside in the JSON argument, the value a `JSON.stringify(...)` string.
- Selection colour: white over a dark page background, black over a light one, alpha `71` (28 % of 255),
  non-premultiplied RGBA; dark is WCAG relative luminance `< 0.5` (herdr's `relative_luminance`: sRGB channels
  linearised with `0.03928`, `12.92`, `((v + 0.055) / 1.055)^2.4`, weights `0.2126`, `0.7152`, `0.0722`).
- Strips: one per document cell row, `cols * cell_w` by `cell_h` px, placed at `z=1`; image ids `1..=32767`
  (`(row % 32767) + 1`), below every tile id (`kitty::image_id` is at least `1 << 15`).
- Coordinates: viewport CSS px = device px / `dpr`, and document CSS px = viewport + `scrollY`, exactly as `click()`
  and `capture()` use them today; rects come back in document CSS px.
- Copy toast: `● Copied to clipboard` for 2000 ms. Status pill: inverted, at the left end of the last row.
- Pasteboard mark for peek's copies: type `local.peek.copy`.
- Commits follow Conventional Commits; nothing is pushed.

## Setup

Work on a branch in `.worktrees/<name>` (CLAUDE.md). The spec and this plan are untracked in the main checkout: copy
both into the worktree and commit them first as `docs: text selection and link hover design and plan`.

## Review Focus

- A hover or a drag while `prefetch` captures: a pointer script that scrolls between a capture's scroll and its
  snapshot shifts that tile. Expected: tiles never shift. Pinned by the job gate in Task 6 and its hand check.
- Page zoom other than 100 % on a Retina screen: strips must cover the selected text, not text beside it. Expected:
  strips line up at 50, 100 and 150 %. `rects_from_css` is tested in Task 2; the hand check is in Task 8.
- The source view: a selection over an empty line and over line numbers. Expected: the file's lines exactly, no
  numbers. Pinned by the script's source branch (Task 3) and the copy table in the hand check (Task 8).
- A release lost outside the pane, and a drag onto row 1 or the last row. Expected: the drag ends and copies; points
  are taken at the nearest page row. Tested in Task 1.
- The status row over a link on the last row, and a toast while it stands. Expected: no flicker; the toast wins.
  Hand check in Task 6.

---

### Task 1: Gestures from mouse reports

**Files:**
- Create: `src/gesture.rs`; Modify: `src/lib.rs` (`pub mod gesture;`)
- Test: `tests/gesture.rs`

**Interfaces:**
- Consumes: `term::MouseKind` (`Move` carries `button = b & 3`: `0` while the left button is held, `3` with none).
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Gesture { None, Press, Word, Extend, Copy, Click, Hover }`
  - `pub struct Gestures`, `pub fn new(double_click_ms: f64) -> Gestures`,
    `pub fn on(&mut self, kind: MouseKind, button: u8, cell: (u16, u16), t_ms: f64) -> Gesture`,
    `pub fn held(&self) -> bool`.
  - `pub fn page_point(x_px: u32, y_px: u32, cell_h: u32, first_row: u16, view_rows: u32, dpr: f64) -> (f64, f64)`.

- [ ] **Step 1: Write the failing tests** in `tests/gesture.rs` (`Gestures::new(500.0)`, helpers `press`, `release`,
  `drag` (Move, button 0), `moved` (Move, button 3)):
  - `click_is_press_then_release_in_the_same_cell`: press (3,5)@0 → `Press`; drag (3,5) → `None`; release (3,5)@80
    → `Click`; `held()` true between, false after.
  - `drag_extends_on_each_new_cell_and_copies_on_release`: press (3,5) → `Press`; drag (4,5) → `Extend`; drag (4,5)
    → `None`; drag (9,7) → `Extend`; release (9,7) → `Copy`.
  - `second_press_in_time_and_place_is_a_word`: press/release (3,5)@0/50, press (3,5)@300 → `Word`; release →
    `Copy`; a third press (3,5)@400 → `Press`.
  - `second_press_too_late_or_elsewhere_is_a_press`: press/release (3,5)@0/50 then press (3,5)@600 → `Press`;
    press/release (3,5) then press (4,5)@100 → `Press`; a drag then a press in its end cell within 500 ms → `Press`.
  - `lost_release_copies`: press (3,5), drag (6,5) → `Extend`, moved (7,5) → `Copy`, `held()` false; moved (8,5) →
    `Hover`.
  - `hover_only_on_a_new_cell`: moved (3,5) → `Hover`; moved (3,5) → `None`; moved (4,5) → `Hover`.
  - `other_buttons_and_wheel_change_nothing`: press with button 2 → `None`; `WheelDown` → `None`; `held()` false.
  - `page_point_maps_like_a_click_and_clamps`: `page_point(20, 2 * 36 + 10, 36, 2, 20, 2.0)` == `(10.0, 23.0)`
    (pixel row 82 is terminal row 3, the page's second row); `y_px` on row 1 → `y == 0.0`; `y_px` below row 21 → `y == (20 * 36 - 1) / 2`.
- [ ] **Step 2: Run** `cargo test --test gesture` — expected: compile failure (no `peek::gesture`).
- [ ] **Step 3: Implement** `src/gesture.rs`. State: the last press (cell, time, whether its release was a click),
  the press in progress (cell, `word`, `dragging`), the last pointer cell. A press is a double-click when the previous
  press ended as a click, in the same cell, and `t_ms - previous < double_click_ms`; after a double-click the next
  press counts from scratch. `page_point`: `y_in_view = y_px - (first_row - 1) * cell_h`, clamped to
  `0 ..= view_rows * cell_h - 1` (saturating), both divided by `dpr`.
- [ ] **Step 4: Run** `cargo test --test gesture` — expected: pass.
- [ ] **Step 5: Commit** `feat(gesture): tell clicks, drags and double-clicks apart`.

### Task 2: Selection strips

**Files:**
- Create: `src/overlay.rs`; Modify: `src/lib.rs` (`pub mod overlay;`), `src/kitty.rs`
- Test: `tests/overlay.rs`, `tests/kitty.rs`

**Interfaces:**
- Consumes: `theme::Rgb`.
- Produces in `kitty.rs`:
  - `pub fn transmit_direct(image: u32, w: u32, h: u32, rgba: &[u8]) -> String`: `ESC _G a=t,t=d,f=32,s=w,v=h,
    i=image,q=2,m=1;<base64> ESC \`, the base64 cut into 4096-byte chunks, later chunks `ESC _G m=1;…`, the last `m=0`.
  - `pub fn place_at(image: u32, row: u16, w: u32, h: u32, z: i32) -> String`: `ESC [ row ;1H` then
    `ESC _G a=p,i=image,p=1,x=0,y=0,w=w,h=h,z=z,C=1,q=2 ESC \`.
- Produces in `overlay.rs`:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Rect { pub x: f64, pub y: f64, pub w: f64, pub h: f64 }`
    (device px, `y` from the document's top).
  - `pub fn selection_color(page_bg: Rgb) -> [u8; 4]`.
  - `pub fn rects_from_css(css: &[[f64; 4]], dpr: f64) -> Vec<Rect>`.
  - `pub fn strip(rects: &[Rect], row: u64, width: u32, cell_h: u32, color: [u8; 4]) -> Option<Vec<u8>>`: `None`
    when no rect touches the row; a pixel is coloured when its centre lies inside a rect (set, not blended, so
    overlapping rects never darken).
  - `pub fn strip_id(row: u64) -> u32`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct StripView { pub top: u64, pub rows: u32,
    pub first_screen_row: u16, pub width: u32, pub cell_h: u32 }`.
  - `#[derive(Default)] pub struct Strips` with `pub fn update(&mut self, rects: &[Rect], view: StripView,
    color: [u8; 4]) -> String` and `pub fn forget(&mut self)`.

- [ ] **Step 1: Write the failing tests.** In `tests/kitty.rs`: `direct_transmission_is_chunked` (a 48 x 32 RGBA
  image, 6144 bytes or 8192 base64 characters, gives two `ESC _G` commands whose base64 joins back to the data, the
  first with `a=t,t=d,f=32,s=48,v=32` and `m=1`, the last `m=0`); `placement_over_tiles_has_a_z` (`place_at(7, 4, 800, 36, 1)` contains `\x1b[4;1H` and `z=1`).
  In `tests/overlay.rs`:
  - `selection_color_is_herdrs`: base `#1e1e2e` → `[255, 255, 255, 71]`; `#eff1f5` → `[0, 0, 0, 71]`; for bases
    `#1e1e2e`, `#303446`, `#eff1f5`, `#000000`, `#ffffff`, `#808080`, blending the colour over the base
    (`(b * 184 + t * 71) / 255`, rounded) is within 1 per channel of herdr's `b + (t - b) * 0.28`, rounded.
  - `css_rects_scale_by_dpr`: `rects_from_css(&[[10.0, 100.5, 20.0, 18.0]], 2.0)` == `[Rect { x: 20.0, y: 201.0,
    w: 40.0, h: 36.0 }]`.
  - `strip_colours_only_the_rect`: `strip(&[Rect { x: 10.0, y: 36.0, w: 5.0, h: 36.0 }], 1, 40, 36, c)` colours
    columns 10..15 of every pixel row, nothing else; row 0 and row 2 → `None`.
  - `rect_across_rows_and_past_the_edge`: a rect `x: 30, y: 20, w: 50, h: 40` with width 40, cell_h 36 colours
    columns 30..40 of rows 0 (pixel rows 20..36) and 1 (0..24).
  - `strips_send_once_and_follow_the_view`: first `update` for a view of rows 0..10 with rects on rows 2 and 3 holds
    two `a=t` and two `a=p` with `z=1` at screen rows `first_screen_row + 2` and `+ 3`; the same call again holds no
    `a=t` and two `a=p`; changing the rect on row 3 only sends row 3; scrolling `top` to 3 deletes row 2's strip
    (`a=d,d=I,i=3`) and places row 3 at `first_screen_row`; `update(&[], …)` deletes every sent strip; a new `width`
    or `cell_h` sends every strip again; after `forget()` the next `update` sends again.
- [ ] **Step 2: Run** `cargo test --test overlay --test kitty` — expected: compile failure.
- [ ] **Step 3: Implement** both. `Strips` keeps, per document row sent, a fingerprint of its pixels (std
  `DefaultHasher`) and the geometry it was made for; `update` builds the strips for rows `top..top + rows` that a rect
  touches, sends those whose fingerprint changed, places all of them, and deletes sent rows that are off screen or
  no longer touched.
- [ ] **Step 4: Run** `cargo test --test overlay --test kitty` — expected: pass.
- [ ] **Step 5: Commit** `feat(overlay): selection strips in herdr's selection colour`.

### Task 3: The selection script

**Files:**
- Modify: `src/script.rs`
- Test: `tests/script.rs`

**Interfaces:**
- Produces: `#[derive(Clone, Copy, Debug, PartialEq)] pub enum SelOp { Press { x: f64, y: f64 },
  Word { x: f64, y: f64 }, Extend { x: f64, y: f64 }, Rects, Text, Clear }` and
  `pub fn selection(top_css: f64, op: SelOp) -> String`. JSON argument `{"top": .., "op": "press" | "word" |
  "extend" | "rects" | "text" | "clear", "x": .., "y": ..}` (`x`, `y` only for the three point ops). Replies:
  `Press`, `Clear`: `null`; `Word`, `Extend`, `Rects`: `[[x, y, w, h], …]` in document CSS px (`[]` without a
  selection); `Text`: the text, or `null` without a selection.

- [ ] **Step 1: Write the failing tests** in `tests/script.rs`: `selection_scripts_carry_their_arguments` (each op's
  script passes `assert_replies_json`, its `args` hold `top`, `op` and, for the point ops, `x` and `y` as given) and
  `selection_coordinates_stay_json` (`x: f64::NAN` is not written into the script: `selection` turns a non-finite
  coordinate into `0`).
- [ ] **Step 2: Run** `cargo test --test script` — expected: compile failure.
- [ ] **Step 3: Implement** the constant body `SEL`, keeping `window.peekSel = { anchor, range }`:
  - every call first runs `window.scrollTo(0, a.top)`;
  - a point is `document.caretRangeFromPoint(a.x, a.y)`; `null` leaves the state as it is;
  - `press` sets `anchor` and leaves `range`; `extend` sets `range` from `anchor` to the point in document order
    (`anchor` taken from the point when missing); `word` finds the `Intl.Segmenter(undefined, { granularity: 'word' })`
    segment of the point's text node that holds the offset (the one before it at the node's end) and sets `range` and
    `anchor` to it; `clear` drops both;
  - rects are those of the text only: for each text node `range` touches, `getClientRects()` of a Range over that
    node's selected part, non-empty ones, as `[left + scrollX, top + scrollY, width, height]` (an element's own box is
    never painted);
  - `text`: when `range` lies in `table.source`, the `td.c` cells of the rows from the start's `tr` to the end's,
    each clipped to `range`, joined with `\n` (empty lines kept, `td.n` never read); otherwise the document selection
    is set to `range`, read with `getSelection().toString()` and emptied again in the same script.
  Update the module doc and `tests/script.rs`'s header comment (the bodies are checked by hand in Task 8).
- [ ] **Step 4: Run** `cargo test --test script` — expected: pass.
- [ ] **Step 5: Commit** `feat(script): select by point and word, and read the selection as rendered text`.

### Task 4: The status row's text

**Files:**
- Modify: `src/term.rs`; `src/mac/viewer.rs` (`percent_decode` at :963 moves to `term.rs`)
- Test: `tests/term.rs`

**Interfaces:**
- Produces: `pub fn percent_decode(s: &str) -> String` (moved, unchanged);
  `pub fn status_text(href: &str, width: usize) -> String`; `pub fn render_status(text: &str, row: u16) -> String`.

- [ ] **Step 1: Write the failing tests** in `tests/term.rs`:
  - `status_shows_a_file_as_its_path`: `status_text("file:///Users/me/docs/a%20b.md", 80)` ==
    `"/Users/me/docs/a b.md"`; a 120-column path at width 30 keeps its file name, holds `…`, `display_width <= 30`.
  - `status_shows_anchors_and_addresses_as_written`: `"#install"` → `"#install"`; `"https://example.com/a?b=c"` at
    width 80 unchanged; a 200-character address at width 40 has `display_width <= 40` and ends with `…`.
  - `status_pill_is_inverted_at_the_left`: `render_status("x", 24)` starts with `"\x1b[24;1H\x1b[2K\x1b[7m x "` and
    ends with `"\x1b[27m"`.
- [ ] **Step 2: Run** `cargo test --test term` — expected: compile failure.
- [ ] **Step 3: Implement**: a `file://` href becomes `percent_decode` of the rest and goes through `shorten_path`;
  anything else through `fit`. Move `percent_decode` and point its callers in the viewer (`follow`, `scroll_to_anchor`) at it.
- [ ] **Step 4: Run** `cargo test --test term` and `cargo build` — expected: pass, no warnings.
- [ ] **Step 5: Commit** `feat(term): status row text for a link`.

### Task 5: peek's own copies

**Files:**
- Modify: `src/watcher.rs` (`Clipboard` at :30, `run` loop at :73), `src/mac/clipboard.rs`
- Test: `tests/select.rs` (`FakeClipboard` at :193)

**Interfaces:**
- Produces in `watcher.rs`: `pub const PEEK_COPY_TYPE: &str = "local.peek.copy"`; `Clipboard::from_peek(&self) ->
  bool` with a default of `false`; `pub fn selected_text(poll: &mut Poll, clip: &impl Clipboard, enabled: bool) ->
  Option<String>`: always calls `poll.changed`, then `None` when nothing changed, `enabled` is false or the change is
  peek's own, else `clip.text()`.
- Produces in `mac/clipboard.rs`: `pub fn copy_text(text: &str) -> io::Result<()>` (clear, the string as
  `NSPasteboardTypeString`, and `"1"` under `PEEK_COPY_TYPE`); `Pasteboard::from_peek` true when the pasteboard's
  `types()` hold `PEEK_COPY_TYPE`.

- [ ] **Step 1: Write the failing tests** in `tests/select.rs`: give `FakeClipboard` a `peek: Cell<bool>` (`copy`
  sets it false, a new `copy_from_peek` true) and `from_peek`; `peek_copies_do_not_open_files` (`copy_from_peek
  ("docs/a.md")` → `selected_text` is `None` and `reads == 0`; `copy("docs/a.md")` → `Some("docs/a.md")`);
  `select_to_open_off_reads_nothing` (`enabled` false → `None`, `reads == 0`, and the change is used up: turning it on
  without a new copy still gives `None`).
- [ ] **Step 2: Run** `cargo test --test select` — expected: compile failure.
- [ ] **Step 3: Implement**; `run` uses `selected_text(&mut poll, clip, state::select_enabled())`.
- [ ] **Step 4: Run** `cargo test --test select` and `cargo build` — expected: pass.
- [ ] **Step 5: Commit** `feat(watcher): leave peek's own copies alone`.

### Task 6: Pointer jobs, link hover and clicks on release

**Files:**
- Modify: `src/mac/viewer.rs` (`Viewer` at :38, `view_rows` at :271, `draw` at :509, `capture` and `prefetch`
  callbacks, `click` at :909, `on_mouse` at :1093)

**Interfaces:**
- Consumes: `gesture::{Gestures, Gesture, page_point}`, `term::{status_text, render_status}`, `script::hit`.
- Produces (private): fields `gestures: Gestures` (made with `NSEvent::doubleClickInterval() * 1000.0`),
  `jobs: VecDeque<Job>`, `job_running: bool`, `status: Option<String>` (the href shown);
  `enum Job { Hover { x: f64, y: f64 }, Click { x: f64, y: f64 } }`; `fn job(j: Job)`, `fn next_job()`.

- [ ] **Step 1: Jobs.** `job` appends (a `Hover` replaces a waiting `Hover`) and calls `next_job`; `next_job` runs the
  front job only when `!job_running && !busy && loaded`, as one `eval` scrolled to the current `view_top` (`hit` after
  `window.scrollTo(0, top_css)`; the selection script of Task 7 takes `top` itself), and calls `next_job` again when its reply is handled. Every place that sets
  `busy = false` after a capture (`draw`, `prefetch`) calls `next_job` afterwards.
- [ ] **Step 2: Hover and the status row.** In `on_mouse`, a `Move` with no button on the bar keeps today's tooltip
  logic and takes the status away; elsewhere `Gesture::Hover` queues `Job::Hover` at `page_point(..)`, except while
  the pointer is on the last row and the status stands. Its reply (`script::hit`) sets `status`; a change redraws
  (`Some`↔`None` changes `view_rows`; clear the row with `ESC [ rows ;1H ESC [2K` when it goes). `view_rows` gives up
  the last row when `toast.is_some() || (status.is_some() && !gestures.held())`; `draw` writes the toast, else, when
  the button is not held, `render_status(&status_text(href, cols - 2), rows)`; a press while the status stands redraws
  (the page takes the row back). A press on the last row while the status (not a toast) stands
  follows that href.
- [ ] **Step 3: Clicks on release.** Presses on row 1 and on a toast stay as they are. Page presses and releases go
  through `gestures.on(..)`; `Gesture::Click` queues `Job::Click` at `page_point(..)`, whose reply goes to `follow` as
  `click()` does today; remove `click()`.
- [ ] **Step 4: Run** `cargo build` and `cargo test` — expected: no warnings, all pass.
- [ ] **Step 5: Commit** `feat(viewer): show a link's destination on hover; follow links on release`.
- [ ] **Step 6: Quick check by hand** (`bin/peek viewer` on a Markdown file with links, in a herdr pane): the
  status row follows the pointer over links, does not flicker over a link on the last row, gives way to a toast;
  clicks open links; moving the pointer while the page prefetches leaves the tiles in place.

### Task 7: Selecting and copying

**Files:**
- Modify: `src/mac/viewer.rs` (`Viewer`, `Job`, `draw`, `prefetch` at :595, `message` at :344, `load_page`,
  `resized` at :646, `restore_and_draw`, `on_key`, `on_mouse`, `toast` at :855)

**Interfaces:**
- Consumes: `script::{selection, SelOp}`, `overlay::{Strips, StripView, Rect, rects_from_css, selection_color}`,
  `mac::clipboard::copy_text`.
- Produces (private): fields `sel_rects: Vec<Rect>`, `strips: Strips`; `Job::Sel(SelOp)` (a waiting `Extend` at the
  back of the queue is replaced by a newer one); `fn toast_for(text: String, ms: u64)` (`toast` calls it with
  `TOAST_MS`).

- [ ] **Step 1: Gestures to jobs.** `Press` → `Sel(Press)`; `Word` → `Sel(Word)`; `Extend` → `Sel(Extend)`, each at
  `page_point(..)`; `Copy` → `Sel(Text)`; `Click` → `Sel(Clear)` first when `sel_rects` is not empty, then
  `Job::Click`. Replies to `Word`, `Extend`, `Clear` and `Rects` set `sel_rects` (`rects_from_css(.., dpr)`, empty for
  `null`) and draw. The `Text` reply, when non-empty, goes to `copy_text` and `toast_for("● Copied to clipboard", 2000)`
  (a failed copy: `toast_for("✕ Copy failed: <error>", 2000)`).
- [ ] **Step 2: Strips in every draw.** After placing the tiles, `draw` appends `strips.update(&sel_rects,
  StripView { top: view_top, rows, first_screen_row: FIRST_ROW, width: cols * cell_w, cell_h },
  selection_color(theme.base))`, `rows` being the rows the layout shows. `message` (which deletes every image) calls
  `strips.forget()`.
- [ ] **Step 3: Lifetimes.** A loaded page empties `sel_rects` (`load_page`'s success); after a re-layout (`resized`,
  `restore_and_draw`) a non-empty selection queues `Sel(Rects)`; `Esc`, when neither the find row nor the `…` row
  takes it, queues `Sel(Clear)` if there is a selection; `prefetch` does nothing while `gestures.held()`, and a
  `Copy` schedules `after(40, prefetch)`.
- [ ] **Step 4: Run** `cargo build` and `cargo test` — expected: no warnings, all pass.
- [ ] **Step 5: Commit** `feat(viewer): select text by drag or double-click and copy it on release`.

### Task 8: Docs and the hand test

**Files:**
- Modify: `docs/superpowers/specs/2026-11-15-peek-webkit-viewer-design.md` ("Input": clicks follow on release, drag
  and double-click select and copy, the status row, a pointer to the 2026-11-19 design), `README.md` and
  `README.ko.md` (a short "Select and copy" note beside the keys: drag, double-click, release copies, Esc clears,
  hover shows where a link goes), `evals/results.md`

- [ ] **Step 1: Docs** as listed; run `cargo test --test language`.
- [ ] **Step 2: Commit** `docs: text selection, copy and link hover`.
- [ ] **Step 3: By hand, with the user**, in a separate herdr session with the plugin linked to this branch's
  checkout (as in `evals/results.md`): every row of the spec's copy table checked with `pbpaste`; a double-click on a
  Korean word; the selection's colour beside a selection in the next pane, dark and light theme; the selection follows
  the pointer without visible lag, stays on its text when the wheel scrolls, at zoom 50 and 150 % and after a pane
  resize, and goes after `Esc`, a click and a reload; a drag that starts on a link does not open it; a copied path
  that is also on screen in another pane does not open; the copy toast. Fix what fails (each fix with a test where
  the logic is platform-neutral).
- [ ] **Step 4: Record** the results in `evals/results.md` and commit `docs(evals): text selection checked by hand`.

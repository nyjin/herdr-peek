# peek: text selection, copy on release, and link hover — Design

> 2026-11-19. Builds on the current design (`2026-11-15-peek-webkit-viewer-design.md`); once built, that document's
> "Input" section is updated to match.

## Intent

The page in the viewer is a picture (Kitty graphics), so its text cannot be selected or copied, and nothing shows
that the pointer is over a link. The user wants the page to behave like text in a herdr pane: a drag selects, a
double-click selects a word, and letting go of the button copies the selection (as herdr's `ui.copy_on_select`
does). Moving over a link should show that it is one. Where a choice was open, the user asked for the lighter way.

## User decisions (2026-11-19)

- Drag selects text, double-click selects a word, and releasing the button copies the selection to the clipboard.
- What is copied is the rendered text (what the page shows), not the Markdown source.
- Link hover: the link's destination on the pane's last row (approach B below); the link itself is not redrawn.
- The selection is drawn as translucent images laid over the page's tiles (approach 2 below). The user first chose
  approach 1 (paint in the page, capture again); once its real cost (about 0.1-0.3 s per repaint) was known, the
  user asked for an image over the original instead, and a spike in their herdr showed it works (Findings).
- The selection's colour follows the theme the way herdr's does: herdr's own selection colour, computed from the
  terminal's background ("Drawing the selection").

## Goals, non-goals, anti-goals

Goals:
- A drag on the page selects the text from where the button went down to the pointer, painted as a selection and
  following the pointer while the button is held.
- A double-click selects the word under the pointer, Korean included.
- Releasing the button after a drag or a double-click copies the selected text as plain text.
- Moving over a link shows where a click would go, on the last row.
- A click on a link still follows it.

Non-goals:
- Scrolling by itself when a drag reaches the top or bottom of the page. (The wheel still scrolls while the button
  is held; the selection's end follows the pointer from the new position on the next move.)
- Triple-click, Shift+click to extend, word-wise dragging after a double-click (herdr has it), selection by keys,
  `Cmd+C`.
- herdr's `ui.copy_on_select = false`: peek copies on release regardless.
- Changing the mouse pointer's shape: herdr does not pass a pane's OSC 22 on to the outer terminal (its source has no
  pointer-shape handling, checked 2026-11-19).
- Rich text or HTML on the clipboard; the Markdown source of the selection.
- Redrawing the hovered link (approach A).
- Text inside images and SVG.

Anti-goals:
- A click on a link that no longer opens it, or a drag that opens a link it started on.
- peek's own copy opening a file through select to open.
- Pointer motion that makes the viewer lag: hover checks or drag updates queuing behind each other.
- Selection images left behind in the terminal after the selection has gone.
- Tiles captured at the wrong scroll position because a pointer script scrolled the page mid-capture.
- A status row that flickers.

## Findings (2026-11-19)

- herdr passes presses, releases, drags and plain motion to a pane program that turned mouse reporting on
  (`src/pane/input.rs`; CHANGELOG #1865: "preserving ... pane application mouse tracking"). When herdr has only a cell
  position it reports the cell's centre in pixels (`cell_center_px`), so a selection moves in steps of a cell, about
  one character.
- peek already asks for motion reports (`?1003h`); `term::mouse` parses them as `MouseKind::Move` with
  `button = b & 3`: 0 while the left button is held, 3 with no button.
- Find marks matches with the CSS Custom Highlight API and captures the tiles again; WebKit paints highlights in its
  snapshots (except in the first ~700 ms after a load, which `run_find` handles by capturing again).
- One tile capture takes 8-36 ms (`evals/results.md`), but a repaint is more than the capture: sending the image
  through herdr and herdr showing it took 91 ms from key to screen for a 12.1 MB image captured on every scroll, and
  318 ms for 21.5 MB (`evals/spike/README.md`). peek's tiles are at most 12 MiB and a full pane shows one or two, so a
  repaint of the screen takes roughly 0.1-0.3 s.
- `Renderer::capture` scrolls the page in one script, then takes the snapshot asynchronously. `click()` also scrolls
  (to the view's top) before its hit test. A pointer script running between a capture's scroll and its snapshot moves
  the page under it.
- `invalidate()` bumps the generation and forgets the tiles, but the images of the old generation stay in the
  terminal until the next `delete_all` (a new page). Find does this per keystroke. The selection does not (approach
  2 never captures the page again), so this stays as it is. While idle, `prefetch` captures about four screens
  around the view.
- herdr shows a pane's Kitty images by sending them on to the outer terminal. Each placement keeps its stacking order
  (`z=`) and each image its RGBA data, alpha included (`src/kitty_graphics.rs`, `encode_display_placement` and
  `upload_control`, in 0.9.0 and on main). herdr places every image in whole cells (`c=`, `r=`), so an image that is
  not a whole number of cells is stretched: images over the page must be whole cell rows, like the tiles.
- Spike (2026-11-19, the user's herdr 0.9.0 under Ghostty, a throwaway script in a tab of its own): an opaque image
  placed as peek places tiles (no `z`), with strips one cell row tall placed over it at `z=1`, each transparent
  except a blue rectangle at about 35 % alpha (non-premultiplied RGBA, `f=32`). The user saw the dark bars of the
  image through the blue, the image unchanged where the strips are transparent, and a strip sent again under the same
  image id about 17 times a second grow smoothly, without flicker.
- The text find collects (`window.peekFind` in `script.rs`) has no break at `<br>` and skips empty text nodes, so an
  empty source line (`<td class="c"></td>`) leaves no line behind. It is not suitable as is for copying.
- The watcher acts on any change of the clipboard; the text only opens a file when it is shown in a pane of the
  current tab, so a path copied from peek that is also on screen in the Claude pane would open.

## Approaches

Hover:
- A. Redraw the link (underline or tint) when the pointer enters or leaves it: a capture each time.
- **B. The destination on the last row**, like a browser's status bubble: text only, the tiles are placed again but
  not captured. Chosen: lighter.

Selection:
- 1. Paint the selection in the page and capture the visible tiles again, the way find does. Least code, but each
  step sends whole tiles again: roughly 0.1-0.3 s per repaint (Findings), so the selection would trail the pointer in
  visible jumps. First chosen, then dropped for 2.
- **2. Ask the page for the selection's rectangles and lay translucent images over the tiles.** Only the strips the
  pointer changed are sent (a cell row is about 0.2 MB at Retina sizes), the page is never captured again, and the
  spike showed herdr and Ghostty blend them. More code than 1: drawing the rectangles into strips, placing them with
  the view, freeing them. Chosen.
- 3. No painting during the drag, copy on release. Lightest, but the user cannot see what is selected.

## Behaviour

### The pointer on the page

The page is rows 2 to the last row, less the last row while a toast or the status row stands. Row 1 and a toast
behave as today.

- **Move, no button:** when the pointer enters another cell, the page is asked for the link under it (the hit test
  clicks use). A link shows the status row; no link takes it away. One question at a time; a newer cell replaces one
  that is waiting.
- **Press (left button):** remembers the cell and the time. A second press in the same cell within the system's
  double-click interval (`NSEvent.doubleClickInterval`) is a double-click: the word at the point is selected and
  painted. Word bounds follow `Intl.Segmenter` (word granularity), so Korean and other scripts select a word, not a
  whole line.
- **Move with the left button held, in another cell than the press:** a drag. The selection runs from the press
  point to the pointer, in document order whichever way the pointer went, and is painted again as the pointer moves.
  A drag that leaves the page rows (onto row 1 or the last row) is taken at the nearest page row.
- **Release after a drag or a double-click:** copies the selected text when it is not empty. The selection stays
  painted.
- **Release after a press without a drag (the same cell):** a click. A painted selection goes away; then a link at
  the point is followed as today. A link is therefore followed on release, not on press.
- **A move without a button while a drag is on** (the release happened outside the pane, or was lost): ends the drag
  as a release would, copying.
- **Esc**, while neither the find row nor the `…` row is open: takes the selection away.
- The selection also goes when another page loads (open, reload, back, a file change, rendered/source). Zoom and a
  pane resize keep it.
- While the button is held, the status row is not shown.

### The status row

- The pane's last row, shared with the toast; the toast wins while it stands.
- It shows what a click would do: a local file as its path, shortened in the middle as the toast does
  (`term::shorten_path`); a web or mail address as written; a link within the page as `#anchor`.
- The same inverted pill as the toast, at the left end of the row, like a browser's status bubble.
- While it stands the page draws one row shorter, as with the toast: the tiles are placed again, not captured.
- No flicker: while the pointer is on the last row, the status row does not change. A click on it follows the link it
  shows (that link may be the one it covers).
- herdr sends no event when the pointer leaves the pane. A status row standing then stays until the pointer moves in
  the pane again, as the row-1 tooltips already do; no timer.

### What is copied

The rendered text as plain text (`NSPasteboardTypeString`), as Safari copies the same selection (WebKit's
`Selection.toString()`), except in the source view (below). Where this table and Safari ever disagree, Safari wins
for the rendered page. Expected results, to be pinned by tests (the rendered-page rows were checked in a hidden
WKWebView during the builder check, 2026-11-19):

| Selection | Copied |
|-----------|--------|
| A heading and two paragraphs | `Heading\n\nFirst para.\n\nSecond para.`: a blank line after the heading and between paragraphs |
| A tight list of three items | three lines, one item each, without bullets or numbers |
| A loose list (`<li><p>`) | the items with a blank line between them |
| A table row | the cells on one line, separated by tabs |
| A line with `<br>` | a line break where the `<br>` is |
| Part of a code block | its text with spaces, line breaks and blank lines as shown |
| Source view, lines 3-6 with line 4 empty | exactly the file's lines 3-6, line 4 empty, no line numbers |
| A word selected by double-click | the word only |

After a copy, a toast like herdr's "copied to clipboard" stands for 2 s (the screenshot toast's pill, shorter
because there is no path to read).

### Clipboard and select to open

peek writes the text together with a second, private pasteboard type that marks it as peek's own copy. The watcher
skips a clipboard change that carries the mark. herdr's next copy clears it, as any write to the pasteboard does.

### Drawing the selection

- The page keeps the selection as a DOM `Range` in peek's own script; nothing in the page is painted for it, and the
  tiles never change because of it.
- After each change of the selection, the page is asked for its rectangles (`Range.getClientRects()`, turned into
  document CSS px with the scroll offset, so asking needs no scroll). peek turns them into device pixels with the
  zoom and the device pixel ratio the tiles use.
- They are drawn into strips: one RGBA image per document cell row the selection touches, as wide as the tiles
  (`cols * cell_w`) and one cell tall (`cell_h`), transparent except inside the rectangles, which get the selection
  colour below (non-premultiplied RGBA, as in the spike).
- The selection colour is herdr's own: herdr colours a selection in a pane with the terminal's background mixed 28 %
  toward white when that background is dark, toward black when it is light (`automatic_selection_bg` in
  `src/ui/panes.rs`, 0.9.0 and main; dark is relative luminance below 0.5). peek draws white or black at 28 % alpha,
  which over the page background (the terminal's background, `PageTheme.base`) gives exactly herdr's colour, so a
  selection in the viewer looks like one in the pane beside it. Dark or light is judged on `PageTheme.base` with
  herdr's rule, so the colour follows the theme and, like the other colours, changes on reload. Unlike herdr, the text
  keeps its own colour (lightened or darkened by the overlay) instead of turning black or white.
- A strip is placed at `z=1` on the screen row where its document row stands, so it scrolls with the page: every draw
  places the strips of the rows on screen, as it places the tiles. Strips exist for rows on screen only; a row that
  scrolls into view gets its strip then, a row that leaves the screen loses it.
- During a drag only the strips whose pixels changed are sent again (under their own image ids, then placed again);
  strips the selection no longer touches are deleted. When the selection goes, all its strips are deleted.
- The strips' image ids never collide with the tiles' (`kitty::image_id`).
- Zoom and a pane resize ask the page for the rectangles again after the re-layout and redraw the strips.
- Find's marks stay painted in the page as today; the selection lies over them.

## Constraints

- Page scripts stay off; the selection, hit test and text are peek's own scripts, in `script.rs`'s shape (a constant
  body called with JSON arguments, never code built from outside text).
- No pointer script runs while a capture is between its scroll and its snapshot. The newest one waits and runs when
  the capture ends.
- While the button is held: one selection update (script, rectangles, strips) at a time, newer positions folded
  into one; no `prefetch`, whose captures would hold pointer scripts back.
- Platform-neutral logic lives outside `src/mac/` and is tested by `cargo test`: telling clicks, drags and
  double-clicks apart from mouse reports and times; drawing rectangles into strips and telling which strips changed;
  the status text; the watcher's check.
- Repository language rules: everything in English (`CLAUDE.md`).

## Tests

- Unit: the gesture logic (press, moves and release with cells and times → click, drag or double-click, including
  the lost release); rectangles into strips (which pixels are coloured, a rectangle across two cell rows, a
  rectangle partly off screen, unchanged strips not sent again); the selection colour (white over a dark
  background, black over a light one, at 28 %, blending to herdr's colour within one step per channel); the status
  text for a file, a web address and an
  anchor; the watcher skipping a change marked as peek's own (`tests/select.rs`'s fake clipboard); the scripts'
  argument shapes (`tests/script.rs`).
- Hand test in a separate herdr session, recorded in `evals/results.md`: a drag and a double-click on a Korean word,
  checked with `pbpaste`; each row of the table above; the selection's colour beside a selection in the next pane,
  in the dark and the light theme; the selection follows the pointer without visible lag; it
  stays on its text when the wheel scrolls, after zoom and after a pane resize, and is gone after Esc and after a
  reload; a click on a link still opens it, a drag that starts on a link does not; hovering a link shows the status
  row; a link on the last row opens through the status row; a copied path that is also on screen in another pane does
  not open; tiles stay correct while the pointer moves during prefetch.

## Left to the builder

- How the scripts find the text position under a point (`document.caretRangeFromPoint` is the obvious one) and how
  they build the copied text so the table above holds (`Selection.toString()` for the rendered page and a walk over
  `td.c` cells for the source view is a reasonable start).
- How strips travel (shared memory as the tiles, or direct), their id scheme, and how changed strips are told apart
  (comparing pixels or the rectangles per row).
- The pasteboard type's name, the pill's spacing.
- Where the gesture and strip logic live (new modules or existing ones).

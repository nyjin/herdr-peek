# peek 0.5.0: integration test (2026-11-15)

The built plugin, linked into a separate herdr 0.9.5 session (`XDG_CONFIG_HOME=/tmp/pkx`, so the herdr in daily use
and its plugin list were untouched), shown in its own Ghostty window on a Retina display (pane cells 17 x 37 px).
Clicks were made through `plugin.action.invoke` with the clicked URL, the path herdr itself takes for a link click;
keys through `herdr pane send-text`; mouse moves, clicks and the wheel as the SGR pixel reports a terminal sends.
Screenshots of each step are in `screens/` (local only).

## What was checked

| Area | Result |
|---|---|
| Click on a markdown path | viewer pane opens to the right without focus; page rendered at native resolution, sharp |
| Markdown | headings, table, relative image from the document's folder, code block |
| Links in the page | local file opens in the viewer, `h` goes back, `#anchor` scrolls (after adding heading ids) |
| Source with `:40` | line 40 marked and scrolled to, highlighting and line numbers |
| Several matches (`README.md`) | numbered list, a digit opens the choice |
| Image, HTML file | image shown; the HTML file's own styles apply and its script does not run |
| Find | 4 matches case-insensitive, 3 with `Aa`; `Enter`/`n` steps; `Esc` clears |
| Zoom | `+` to 120 %, re-laid out; remembered; a herdr action from the Claude pane zooms the viewer |
| Rendered ↔ source | `t` and the bar's Source item |
| Top bar | hovering Reload shows `Reload (r)`; narrow panes show keys only |
| Screenshot | PNG in the session folder, PNG on the clipboard |
| File saved | page redrawn where it was |
| Pane resize | re-laid out, reading position kept |
| Two viewers | two Claude-side panes, two viewers, both drawing |
| Jira / PR without a token | a page with `peek auth …` and how to retry |
| `q` | the viewer pane closes; the next click opens a new one |

Not checked here: open in browser and clicks on web addresses (they open the user's browser), fetching with real
tokens (none registered), PDF (no PDF handling: it shows as text).

## Timings (median)

| | ms |
|---|---|
| key `j` to screen (text baseline in the same window: 38) | 44 |
| key `k` to screen | 52 |
| click to the viewer's first change (includes about 60 ms of the test's own trigger) | 122 (173 before the 30 ms state check) |
| page load in WebKit, small documents | 6 (the design doc: 392) |
| one tile captured (12.5 MB) | 8-36 |

## Found and fixed during the test

- Headings had no ids, so `#anchor` links went nowhere: GitHub-style ids are added.
- The first page was built before herdr reported the pane's pixel size, with a fallback font size: the first open
  waits for it.
- The bar showed the previous document's name over the choice list, and the last part of a link's address for
  fetched pages: it shows "Choose a file" and the fetched page's title.
- The viewer polled the state file every 150 ms: it checks the file's time every 30 ms instead.
- The file watch counted an event from a write made just before the watch started: it compares the file's time and
  size.
- Test detail, not a fix: an action invoked through the API runs for the pane herdr has focused, not the pane named
  in the call's context, so the test focused the clicking pane first.

# peek 0.6.0: select to open, bar and find (2026-11-16 and 17)

Same separate herdr 0.9.5 session as above.

| Area | Result |
|---|---|
| Watcher start | started by a herdr action (`peek action reload`); the action ended `succeeded` at once (stdio on `/dev/null`) |
| One watcher per server | a second one for the same socket exits; the watcher exits two checks after the socket goes, and when the binary is replaced |
| Copy of a path a shell pane shows | opened beside that pane, 55 and 61 ms from the change being seen to the state written |
| Copy of a path no pane shows | nothing opened |
| Selections by hand (the user's) | opened the file beside the Claude Code pane they were made in (`watch.log`); herdr kept focus where it was |
| Korean particle after a path (double-click) | `docs/superpowers/specs/` plus the particle: a folder, nothing opened, as designed |
| Bar | `⟳  Reload   ⌕  Find   </> Source   ↗ Browser   … More`; `v` switches `</> Source` and `◉ View` |
| `… More` | hover tooltip, `+` to 110 %, `Reset` to 100 %, `Esc` back to the bar |
| Find row | typing goes to the field; `Enter`, `Enter`, `Up`, `Shift-Enter` (`CSI 13;2u`) step 2/3, 3/3, 2/3, 1/3; letters such as `j` and `q` go into the query; whole word and regex exclude each other; `Esc` and `✕` close; `↓` hover shows "Next match (Enter)" |
| Screenshot | toast on the last row with the path, the bar left alone; gone after 5 s |

Not checked by a script: the caret's look (the test window sits behind the working one, so captures were stale) and
Shift-Enter from a real keyboard (it depends on herdr passing it on; `↑` does the same).

# Terminal colours and fonts (2026-11-18 and 19)

A separate herdr 0.9.5 session in its own Ghostty 1.3.1 window, as above (`XDG_CONFIG_HOME` pointed at a temporary
folder with `ghostty/config` linked to the user's real one, so Ghostty read the same configuration). Ghostty's theme:
`dark:Catppuccin Mocha,light:Catppuccin Frappe`; font `Jetendard`, then `JetBrains Mono Hangul`. Clicks through
`peek link`, reload and screenshots through `peek action`, from the Claude-side pane.

| Area | Result |
|---|---|
| Colour answers at start (dark) | all 18 (fg, bg, palette 0-15) in 146-259 ms after start; page `--base: #1e1e2e` (Mocha) |
| Source highlighting | keywords `#f5c2e7`, strings `#a6e3a1`, numbers `#f9e2af`, functions `#89b4fa`, comments italic `#6d7187`: readable |
| Ghostty's font | `["Jetendard", "JetBrains Mono Hangul"]` read in 38-97 ms (one first run after a rebuild took over 300 ms: the fonts now still reach the theme when they come late); code shown in Jetendard |
| Plugin pane environment | `TERM_PROGRAM=herdr`, `GHOSTTY_BIN_DIR` set: Ghostty is found by `GHOSTTY_BIN_DIR` |
| System appearance switched to light, viewer open | page unchanged (Mocha); no key or scroll from the answers or the scheme report |
| Reload after the switch | answers again in one query; page `--base: #303446` (Frappe), still dark by the luminance rule |
| Reload | one query and one page build per press; two `peek action reload` at the same moment arrive as one command (the state file) |

Not checked: a terminal that answers fg/bg but no palette (covered by tests only), and Alt+] in the viewer (the
give-up path is covered by `unterminated_osc_gives_way_to_later_input`).

# Text selection, copy and link hover (2026-11-20 and 21)

The user's own herdr 0.9.0 under Ghostty (macOS, Retina). That herdr has no peek plugin installed, so the branch's
release build ran as `peek viewer selection-test.md` in a tab of the session, beside a shell pane showing the same
Markdown. `selection-test.md` is a scratch Markdown file with headings, paragraphs, a tight and a loose list, a table,
a `<br>` line, a code block with an empty line, Korean text, local, web and in-page links, and filler for scrolling; a
five-line `sample.rs` served for the source view.

## Found and fixed during the test

1. Positions landed far from the pointer. A probe pane with `?1000h ?1006h ?1016h` got the four corners of a 194 x 69
   pane as `(1,1)`, `(194,1)`, `(1,69)`, `(194,69)`: herdr 0.9.0-0.9.3 send cell coordinates in SGR form when the
   host gives cells (0.9.4 and later send the cell's centre in pixels). Fixed in 7188dec: peek asks for cell reports
   and takes each cell's centre itself. Link clicks had the same defect before this branch.
2. A source-view drag past the end of a short file copied line numbers (`"1\tfn main"`). Fixed in 2688e3d.
3. No visible way back after following a link (`h` and Backspace existed). `← Back` added to the bar in 955c1b7.

## Rows checked

The user was given a 17-row checklist, in the viewer of the branch's build. The user's reports: after the cell-report
fix, "works well" plus the request for a Back button; after the source-copy and Back fixes, asked to recheck the
`sample.rs` drag past the file's end, Back, and rows 11-17, "works well". Neither report gave detail row by row, so
"reported working" below means those two answers. The clipboard log (the copied text compared with the spec's table)
also confirmed the copied text of rows 1-7 and 9.

| Row | Result |
|---|---|
| 1. Copy from a heading to the second paragraph, with the 2 s copy toast | checked: reported working by the user; clipboard log: blank lines between the heading and the paragraphs |
| 2. Copy the tight list, with the toast | checked: reported working by the user; clipboard log: one item per line |
| 3. Copy the loose list, with the toast | checked: reported working by the user; clipboard log: a blank line between items |
| 4. Copy a table row, with the toast | checked: reported working by the user; clipboard log: tab-separated cells, one row per line |
| 5. Copy the `<br>` lines, with the toast | checked: reported working by the user; clipboard log: two lines |
| 6. Copy part of the code block with its empty line, with the toast | checked: reported working by the user; clipboard log: the indentation and the empty line kept |
| 7. Double-click a Korean word, with the toast | checked: reported working by the user; clipboard log: the word alone (two words) |
| 8. Double-click the right half of a word's last letter, with the toast | checked: reported working by the user; not captured by the clipboard log |
| 9. Drag over source-view lines including an empty one, with the toast | checked: reported working by the user; clipboard log: the Markdown file's lines without numbers, empty lines kept |
| 10. The selection follows the pointer while dragging, and its colour looks like herdr's selection in the next pane (dark theme) | checked: reported working by the user |
| 11. The selection stays on its text when the wheel scrolls; `Esc` and a click take it away | checked: reported working by the user (also in the recheck) |
| 12. Hovering a link shows its destination on the last row (local file path, web address, `#table`) | checked: reported working by the user (also in the recheck) |
| 13. The in-page link scrolls; the local link opens `sample.rs`; back | checked: reported working by the user (also in the recheck) |
| 14. A drag that starts on a link selects and does not open it | checked: reported working by the user (also in the recheck) |
| 15. With the status row standing, wheel to the very end and click the last link: the page does not move on press and the link works | checked: reported working by the user (also in the recheck) |
| 16. Zoom 150 % and 50 %: no seams between tiles while scrolling, and a selection lands on its text; then reset | checked: reported working by the user (also in the recheck) |
| 17. A pane resize keeps the selection on its text | checked: reported working by the user (also in the recheck) |
| Recheck after 2688e3d: the `sample.rs` drag past the file's end | checked: reported working by the user; not captured by the clipboard log |
| Recheck after 955c1b7: Back | checked: reported working by the user |

Not checked:

- the light theme;
- select to open ignoring peek's own copies (no plugin or watcher in that herdr; covered by `tests/select.rs` and by a
  check against the real pasteboard in the plan's Task 5);
- a screenshot at 150 %;
- `+`/`-` keeping the top element in place;
- text crossing tile edges at 110 and 120 %;
- a link on the last row opened through the status row;
- tiles while the pointer moves during prefetch (not tried on purpose);
- the selection going after a reload;
- herdr 0.9.4 or later with cell reports.

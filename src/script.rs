//! Scripts peek runs in the page through `evaluateJavaScript` (page scripts are off). Each evaluates to a JSON
//! string. A script that takes arguments has the shape `(function (a) { ... })(<JSON>)`: the body is a constant
//! and everything that comes from outside (query text, coordinates, marker) is a JSON value, never code.
//!
//! The selection (`selection`) lives in the page as `window.peekSel`, a DOM `Range` and its anchor that only peek's
//! script knows: nothing in the page is painted for it. The script answers with the text's rectangles, which peek
//! draws as overlay strips, and with the selected text, which peek copies.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindQuery {
    pub text: String,
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

impl FindQuery {
    /// Turn option `case`, `word` or `regex` on or off. Whole word and regular expression exclude each other, as in
    /// IntelliJ's and Xcode's find (a pattern sets its own word bounds); match case goes with either.
    pub fn toggle(&mut self, option: &str) {
        match option {
            "case" => self.case = !self.case,
            "word" => {
                self.word = !self.word;
                self.regex &= !self.word;
            }
            "regex" => {
                self.regex = !self.regex;
                self.word &= !self.regex;
            }
            _ => {}
        }
    }
}

/// A word character for "whole word": any letter, combining mark or digit (not only ASCII), or `_`.
const WORD_CHAR: &str = r"[\p{L}\p{M}\p{N}_]";

/// Searches the page's text as one string, so a match may span elements (highlighted source puts every token in
/// its own span); a line break stands between blocks. `window.peekFind` keeps the text (collected once per page:
/// page scripts are off, so the DOM does not change), the last query's matches, and the marked window: at most
/// `LIMIT` matches around the current one are marked, and stepping inside that window only moves the current mark.
const FIND: &str = r#"function (a) {
  const LIMIT = 10000;
  const SKIP = 'script, style, noscript, template, td.n';
  const BLOCK = 'address, article, aside, blockquote, body, dd, details, div, dl, dt, fieldset, figcaption, figure, '
    + 'footer, form, h1, h2, h3, h4, h5, h6, header, hr, li, main, nav, ol, p, pre, section, summary, table, td, '
    + 'th, tr, ul';
  const unmark = () => { CSS.highlights.delete('peek-find'); CSS.highlights.delete('peek-cur'); };
  let st = window.peekFind;
  if (!st) {
    const nodes = [], starts = [], parts = [];
    let len = 0, last = null;
    const walk = document.createTreeWalker(document.body || document.documentElement,
      NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT,
      { acceptNode: (n) => n.nodeType === 1 && n.matches(SKIP) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT });
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      if (n.nodeType !== 3 || !n.data) continue;
      const block = n.parentElement.closest(BLOCK);
      if (block !== last && len) { parts.push('\n'); len += 1; }
      last = block;
      nodes.push(n); starts.push(len); parts.push(n.data); len += n.data.length;
    }
    st = window.peekFind = { nodes, starts, text: parts.join(''), src: null, flags: null, ms: [], all: null, lo: 0 };
  }
  if (st.src !== a.src || st.flags !== a.flags) {
    unmark();
    st.src = null; st.ms = []; st.all = null;
    if (!a.src) return JSON.stringify([0, 0, -1]);
    let re;
    try { re = new RegExp(a.src, a.flags); } catch (e) { return JSON.stringify([-1, 0, -1]); }
    for (const m of st.text.matchAll(re)) {
      if (m[0].length) st.ms.push(m.index, m.index + m[0].length);
    }
    st.src = a.src; st.flags = a.flags;
  }
  const count = st.ms.length / 2;
  if (!count) return JSON.stringify([0, 0, -1]);
  const at = (off, end) => {
    let lo = 0, hi = st.starts.length - 1;
    while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (st.starts[mid] <= off) lo = mid; else hi = mid - 1; }
    const n = st.nodes[lo], o = off - st.starts[lo];
    if (o <= n.data.length) return [n, o];
    return end ? [n, n.data.length] : [st.nodes[lo + 1], 0];
  };
  const range = (k) => {
    const r = new Range(), s = at(st.ms[2 * k], false), e = at(st.ms[2 * k + 1], true);
    r.setStart(s[0], s[1]); r.setEnd(e[0], e[1]);
    return r;
  };
  const i = ((a.index % count) + count) % count;
  if (!st.all || i < st.lo || i >= st.lo + LIMIT) {
    st.lo = Math.max(0, Math.min(i - LIMIT / 2, count - LIMIT));
    st.all = new Highlight();
    for (let k = st.lo; k < Math.min(count, st.lo + LIMIT); k++) st.all.add(range(k));
    CSS.highlights.set('peek-find', st.all);
  }
  const cur = range(i);
  const strong = new Highlight(cur);
  strong.priority = 1;
  CSS.highlights.set('peek-cur', strong);
  return JSON.stringify([count, i, Math.round(cur.getBoundingClientRect().top + window.scrollY)]);
}"#;

/// Removes the marks and forgets the query; the page's collected text stays for the next search.
const FIND_CLEAR: &str = "(() => { CSS.highlights.delete('peek-find'); CSS.highlights.delete('peek-cur'); \
                          const st = window.peekFind; if (st) { st.src = null; st.ms = []; st.all = null; } \
                          return JSON.stringify(null); })()";

const HIT: &str = r#"function (a) {
  // The view may not reach the document point (near the end, or a pane taller than the view): scroll it in first.
  window.scrollTo(0, Math.max(0, a.y - window.innerHeight / 2));
  const e = document.elementFromPoint(a.x, a.y - window.scrollY);
  const link = e && e.closest('a[href]');
  if (!link) return JSON.stringify(null);
  const raw = link.getAttribute('href');
  if (raw.startsWith('#')) return JSON.stringify(raw);
  try { return JSON.stringify(new URL(raw, document.baseURI).href); } catch (err) { return JSON.stringify(raw); }
}"#;

/// The first element under the middle of the viewport, scanning down from the top past margins; the offset is how
/// far the viewport top lies into it, as a fraction of its height, so it survives a re-layout.
const ANCHOR: &str = r#"(() => {
  const all = document.getElementsByTagName('*');
  for (let y = 1; y < window.innerHeight; y += 8) {
    const e = document.elementFromPoint(window.innerWidth / 2, y);
    if (!e || e === document.body || e === document.documentElement) continue;
    const r = e.getBoundingClientRect();
    if (r.height > 0) return JSON.stringify([Array.prototype.indexOf.call(all, e), -r.top / r.height]);
  }
  return JSON.stringify([-1, 0]);
})()"#;

const RESTORE: &str = r#"function (a) {
  const e = document.getElementsByTagName('*')[a[0]];
  if (!e) return JSON.stringify(null);
  const r = e.getBoundingClientRect();
  return JSON.stringify(Math.max(0, Math.round(r.top + window.scrollY + a[1] * r.height)));
}"#;

/// The top hint puts the marked source line a third of the way down the viewport.
const READY: &str = r#"function (a) {
  const d = document.documentElement, hit = document.querySelector('tr.hit');
  const ready = location.search === a.search && document.readyState === 'complete';
  const y = hit ? hit.getBoundingClientRect().top + window.scrollY - window.innerHeight / 3 : 0;
  return JSON.stringify([ready, d ? d.scrollHeight : 0, Math.max(0, Math.round(y))]);
}"#;

/// The selection: `window.peekSel = { anchor, range }`, a point and a `Range` that only this script touches. Points
/// come in document px; the point ops scroll their point into the viewport and take `caretRangeFromPoint` there;
/// when it finds nothing the state stays as it is. Replies: rectangles
/// `[[x, y, w, h], ...]` in document CSS px (`word`, `extend`, `rects`), the text or null (`text`), null otherwise.
const SEL: &str = r#"function (a) {
  const st = window.peekSel || (window.peekSel = { anchor: null, range: null });
  // The document point, scrolled into the viewport first as `hit` does; only the point ops scroll.
  const point = () => {
    window.scrollTo(0, Math.max(0, a.y - window.innerHeight / 2));
    const r = document.caretRangeFromPoint(a.x, a.y - window.scrollY);
    return r ? { node: r.startContainer, offset: r.startOffset } : null;
  };
  // Not painted and not copied: line numbers, and the source view's note row (it is not a file line).
  const SKIP = 'td.n, tr.note';
  const rects = () => {
    const out = [], range = st.range;
    if (!range || range.collapsed) return out;
    const root = range.commonAncestorContainer;
    const nodes = [];
    if (root.nodeType === 3) {
      nodes.push(root);
    } else {
      const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      for (let n = walk.nextNode(); n; n = walk.nextNode()) if (range.intersectsNode(n)) nodes.push(n);
    }
    for (const n of nodes) {
      if (n.parentElement && n.parentElement.closest(SKIP)) continue;
      // the part of this text node inside the range
      const part = new Range();
      part.setStart(n, n === range.startContainer ? range.startOffset : 0);
      part.setEnd(n, n === range.endContainer ? range.endOffset : n.data.length);
      for (const r of part.getClientRects()) {
        if (r.width > 0 && r.height > 0) out.push([r.left + window.scrollX, r.top + window.scrollY, r.width, r.height]);
      }
    }
    return out;
  };
  // The source view: the code cells of the rows the range touches, each clipped to the range, one line each.
  const sourceText = (table) => {
    const range = st.range;
    const rows = Array.prototype.slice.call(table.rows);
    const rowOf = (node, fallback) => {
      const el = node.nodeType === 1 ? node : node.parentElement;
      const tr = el && el.closest('tr');
      return tr && rows.indexOf(tr) >= 0 ? rows.indexOf(tr) : fallback;
    };
    const lines = [];
    for (let i = rowOf(range.startContainer, 0); i <= rowOf(range.endContainer, rows.length - 1); i++) {
      const cell = rows[i].classList.contains('note') ? null : rows[i].querySelector('td.c');
      if (!cell) continue;
      // the range ends before this cell, or starts after it
      if (range.comparePoint(cell, 0) > 0 || range.comparePoint(cell, cell.childNodes.length) < 0) continue;
      const part = new Range();
      part.selectNodeContents(cell);
      if (range.compareBoundaryPoints(Range.START_TO_START, part) > 0) part.setStart(range.startContainer, range.startOffset);
      if (range.compareBoundaryPoints(Range.END_TO_END, part) < 0) part.setEnd(range.endContainer, range.endOffset);
      lines.push(part.toString());
    }
    return lines.join('\n');
  };
  switch (a.op) {
    case 'press': {
      const p = point();
      if (p) st.anchor = p;
      return JSON.stringify(null);
    }
    case 'extend': {
      const p = point();
      if (p) {
        if (!st.anchor) st.anchor = p;
        // a collapsed range at the anchor tells which of the two points comes first
        const probe = new Range();
        probe.setStart(st.anchor.node, st.anchor.offset);
        probe.collapse(true);
        const first = probe.comparePoint(p.node, p.offset) >= 0 ? [st.anchor, p] : [p, st.anchor];
        const range = new Range();
        range.setStart(first[0].node, first[0].offset);
        range.setEnd(first[1].node, first[1].offset);
        st.range = range;
      }
      return JSON.stringify(rects());
    }
    case 'word': {
      const p = point();
      if (p && p.node.nodeType === 3 && p.node.data) {
        // The word segment holding the offset; at the node's end, the last one. At a boundary the offset also lies
        // just after the previous segment (a click on a word's right half reports it): a word beats a space or
        // punctuation there.
        let hit = null, prev = null;
        for (const seg of new Intl.Segmenter(undefined, { granularity: 'word' }).segment(p.node.data)) {
          if (seg.index + seg.segment.length > p.offset) { hit = seg; break; }
          prev = seg;
        }
        if (!hit) hit = prev;
        else if (!hit.isWordLike && prev && prev.isWordLike && prev.index + prev.segment.length === p.offset) hit = prev;
        if (hit) {
          const range = new Range();
          range.setStart(p.node, hit.index);
          range.setEnd(p.node, hit.index + hit.segment.length);
          st.range = range;
          st.anchor = { node: p.node, offset: hit.index };
        }
      }
      return JSON.stringify(rects());
    }
    case 'rects':
      return JSON.stringify(rects());
    case 'text': {
      const range = st.range;
      if (!range) return JSON.stringify(null);
      const el = range.commonAncestorContainer.nodeType === 1 ? range.commonAncestorContainer : range.commonAncestorContainer.parentElement;
      const table = el && el.closest('table.source');
      if (table) return JSON.stringify(sourceText(table));
      // Safari's own rendering of the text: select the range, read it, and empty the selection again
      const sel = window.getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
      const text = sel.toString();
      sel.removeAllRanges();
      return JSON.stringify(text);
    }
    default:
      st.anchor = null;
      st.range = null;
      return JSON.stringify(null);
  }
}"#;

const IMAGES_PENDING: &str =
    "(() => JSON.stringify(Array.prototype.filter.call(document.images, (i) => !i.complete).length))()";

fn call(body: &str, args: &serde_json::Value) -> String {
    format!("({body})({args})")
}

/// The regular expression source for a query: the text itself when `regex` is set, else the text with its regex
/// syntax escaped; wrapped in Unicode-aware word boundaries when `word` is set. Empty text stays empty.
fn source(q: &FindQuery) -> String {
    if q.text.is_empty() {
        return String::new();
    }
    let mut src = String::new();
    if q.regex {
        src.push_str(&q.text);
    } else {
        for c in q.text.chars() {
            if r"\^$.|?*+()[]{}".contains(c) {
                src.push('\\');
            }
            src.push(c);
        }
    }
    if q.word {
        src = format!("(?<!{WORD_CHAR})(?:{src})(?!{WORD_CHAR})");
    }
    src
}

/// Mark every match (CSS Custom Highlight API) and make match `index` current: `[count, index, y]`; no match
/// `[0, 0, -1]`; an invalid regular expression `[-1, 0, -1]`. `index` may be negative or past the end; the reply
/// carries it normalised. `y` is the current match's top in document CSS px.
pub fn find(q: &FindQuery, index: i64) -> String {
    let flags = if q.case { "gu" } else { "gui" };
    call(FIND, &serde_json::json!({ "src": source(q), "flags": flags, "index": index }))
}

pub fn find_clear() -> String {
    FIND_CLEAR.to_string()
}

/// The `href` of the link under the document point (CSS px from the document's top left), or `null`. The script
/// scrolls the point into the viewport first. A link within the page (`#...`) comes back as written; any other is
/// resolved to an absolute URL.
pub fn hit(x_css: f64, y_doc: f64) -> String {
    call(HIT, &serde_json::json!({ "x": x_css, "y": y_doc }))
}

/// `[element index, offset]` of the element at the top of the viewport, to keep the reading position: the index
/// among `document.getElementsByTagName('*')`, the offset a fraction of its height. `[-1, 0]` if none.
pub fn anchor() -> String {
    ANCHOR.to_string()
}

/// The scroll y that puts the element from `anchor()` back at the top, or `null` when it is gone. Anything that is
/// not an `anchor()` reply gives `null`.
pub fn restore(anchor_json: &str) -> String {
    match serde_json::from_str::<(i64, f64)>(anchor_json) {
        Ok((index, offset)) if index >= 0 && offset.is_finite() => call(RESTORE, &serde_json::json!([index, offset])),
        _ => "JSON.stringify(null)".to_string(),
    }
}

/// `[ready, scrollHeight, topHint]`: ready when the page URL carries `?peek=<marker>` and `readyState` is complete.
pub fn ready(marker: u64) -> String {
    call(READY, &serde_json::json!({ "search": format!("?peek={marker}") }))
}

/// Number of images not finished loading.
pub fn images_pending() -> String {
    IMAGES_PENDING.to_string()
}

/// What the selection script does; points are document CSS px (from the document's top left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SelOp {
    /// The button went down: the selection starts at the point. Reply `null`.
    Press { x: f64, y: f64 },
    /// Select the word at the point. Reply: the selection's rectangles.
    Word { x: f64, y: f64 },
    /// Select from where the press was to the point. Reply: the selection's rectangles.
    Extend { x: f64, y: f64 },
    /// The selection's rectangles again (after a re-layout). `[]` without a selection.
    Rects,
    /// The selected text, or `null` without a selection: rendered text as Safari copies it, in the source view the
    /// file's lines without line numbers.
    Text,
    /// Forget the selection. Reply `null`.
    Clear,
}

/// The selection script doing `op`. The point ops (`Press`, `Word`, `Extend`) scroll their point into the viewport
/// first; the others do not scroll (rectangles come back in document CSS px either way). A coordinate that is not
/// finite is sent as 0.
pub fn selection(op: SelOp) -> String {
    let n = |v: f64| if v.is_finite() { v } else { 0.0 };
    let args = match op {
        SelOp::Press { x, y } => serde_json::json!({ "op": "press", "x": n(x), "y": n(y) }),
        SelOp::Word { x, y } => serde_json::json!({ "op": "word", "x": n(x), "y": n(y) }),
        SelOp::Extend { x, y } => serde_json::json!({ "op": "extend", "x": n(x), "y": n(y) }),
        SelOp::Rects => serde_json::json!({ "op": "rects" }),
        SelOp::Text => serde_json::json!({ "op": "text" }),
        SelOp::Clear => serde_json::json!({ "op": "clear" }),
    };
    call(SEL, &args)
}

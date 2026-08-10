// The browser half of the editor.
//
// Rust owns the document; the browser owns the caret and the text inside whichever
// block is focused. This file is the seam between those two, and it exists because
// three things have no Rust-side equivalent inside a webview: reading and restoring
// the caret, applying a mark to a selection, and intercepting a paste before the
// browser inserts markup we cannot model.
//
// Everything here is delegated from `document` rather than bound per block, so one
// script serves every editing surface in the app — the notes pane, the report pane
// and each description field in the template builder — with no per-block setup and
// nothing to tear down when a block is removed.
//
// Offsets crossing into Rust are **code point** counts, not UTF-16 code units, since
// that is what `str::chars()` counts on the other side. `[...s].length` rather than
// `s.length` throughout: they differ the moment anyone pastes an emoji, and the
// mismatch would silently corrupt every edit after it in the block.

// A second eval (a hot reload, a remount) must not leave the previous listeners
// attached: they would keep sending on a dead channel and every keystroke would be
// handled twice.
if (window.__reportEditorCleanup) {
  window.__reportEditorCleanup();
}

const MARK_KEYS = { b: "bold", i: "italic", e: "code" };

function blockOf(node) {
  while (node && node !== document) {
    if (node.nodeType === 1 && node.dataset && node.dataset.blockId) return node;
    node = node.parentNode;
  }
  return null;
}

function elementFor(id) {
  return document.querySelector(`[data-block-id="${id}"]`);
}

/** Code points in `el` before the position (node, offset). */
function offsetOf(el, node, offset) {
  const range = document.createRange();
  range.selectNodeContents(el);
  try {
    range.setEnd(node, offset);
  } catch (_) {
    return 0;
  }
  return [...range.toString()].length;
}

function textLength(el) {
  return [...el.textContent].length;
}

/** The current selection within `el`, or null if it is elsewhere. */
function selectionIn(el) {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0);
  if (!el.contains(range.startContainer) || !el.contains(range.endContainer)) return null;
  return {
    start: offsetOf(el, range.startContainer, range.startOffset),
    end: offsetOf(el, range.endContainer, range.endOffset),
  };
}

function caretIn(el) {
  const where = selectionIn(el);
  return where ? where.end : 0;
}

/** Resolve a code point offset to a (textNode, utf16Offset) pair. */
function locate(el, offset) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let remaining = offset;
  let node = walker.nextNode();
  let last = null;
  while (node) {
    const chars = [...node.data];
    if (remaining <= chars.length) {
      return { node, offset: chars.slice(0, remaining).join("").length };
    }
    remaining -= chars.length;
    last = node;
    node = walker.nextNode();
  }
  // Past the end, or an empty block with no text node at all.
  if (last) return { node: last, offset: last.data.length };
  return null;
}

function setSelection(el, start, end) {
  const from = locate(el, start);
  const to = locate(el, end === undefined ? start : end);
  const range = document.createRange();
  if (from && to) {
    range.setStart(from.node, from.offset);
    range.setEnd(to.node, to.offset);
  } else {
    // An empty block has no text node to place the caret in.
    range.selectNodeContents(el);
    range.collapse(true);
  }
  const sel = window.getSelection();
  sel.removeAllRanges();
  sel.addRange(range);
}

// --- Rust → browser -------------------------------------------------------
//
// Exposed on `window` rather than driven over the event channel, so Rust can issue a
// command with a one-shot eval instead of the editor having to hold a live duplex
// channel open for its whole lifetime.

window.__reportEditor = {
  focus(id, offset) {
    const el = elementFor(id);
    if (!el) return;
    el.focus({ preventScroll: false });
    setSelection(el, offset);
  },
  select(id, start, end) {
    const el = elementFor(id);
    if (!el) return;
    el.focus({ preventScroll: true });
    setSelection(el, start, end);
  },
  // Force a focused block's content to match Rust again, after an edit Rust made
  // itself (a toolbar mark, say). While a block is focused Rust deliberately stops
  // rewriting its HTML — see the focus guard in `editable.rs` — so this is the only
  // way in.
  sync(id, html, start, end) {
    const el = elementFor(id);
    if (!el) return;
    el.innerHTML = html;
    if (document.activeElement === el) setSelection(el, start, end);
  },
};

// --- browser → Rust -------------------------------------------------------

function onInput(event) {
  const el = blockOf(event.target);
  if (!el) return;
  dioxus.send({
    kind: "input",
    id: el.dataset.blockId,
    html: el.innerHTML,
    caret: caretIn(el),
  });
}

function onKeyDown(event) {
  const el = blockOf(event.target);
  if (!el) return;

  const mod = event.metaKey || event.ctrlKey;
  if (mod && !event.altKey) {
    const mark = MARK_KEYS[event.key.toLowerCase()];
    if (mark) {
      // The browser's own bold/italic would insert markup of its choosing; Rust
      // applies the mark to the model instead and syncs the result back.
      event.preventDefault();
      const where = selectionIn(el) || { start: 0, end: 0 };
      dioxus.send({ kind: "mark", id: el.dataset.blockId, mark, ...where });
      return;
    }
  }

  const structural =
    event.key === "Enter" ||
    event.key === "Tab" ||
    // Backspace is only ours at the very start of a block, where it means "merge
    // into the previous one". Everywhere else the browser's own handling is better
    // than anything we would reimplement — it knows about grapheme clusters,
    // input methods and the undo stack.
    (event.key === "Backspace" && isCollapsedAtStart(el));

  if (!structural) return;
  // A soft break (Shift+Enter) is left to the browser: it stays inside one block.
  if (event.key === "Enter" && event.shiftKey) return;

  event.preventDefault();
  dioxus.send({
    kind: "key",
    id: el.dataset.blockId,
    key: event.key,
    shift: event.shiftKey,
    caret: caretIn(el),
    length: textLength(el),
    html: el.innerHTML,
  });
}

function isCollapsedAtStart(el) {
  const sel = window.getSelection();
  if (!sel || !sel.isCollapsed) return false;
  return caretIn(el) === 0;
}

function onFocusIn(event) {
  const el = blockOf(event.target);
  if (!el) return;
  dioxus.send({ kind: "focus", id: el.dataset.blockId });
}

function onFocusOut(event) {
  const el = blockOf(event.target);
  if (!el) return;
  // The final HTML rides along: an edit made and then dismissed with a click
  // elsewhere would otherwise be lost between the last `input` and the blur.
  dioxus.send({ kind: "blur", id: el.dataset.blockId, html: el.innerHTML });
}

function onSelectionChange() {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return;
  const el = blockOf(sel.anchorNode);
  if (!el) return;
  const where = selectionIn(el);
  if (!where) return;
  dioxus.send({ kind: "selection", id: el.dataset.blockId, ...where });
}

function onPaste(event) {
  const el = blockOf(event.target);
  if (!el) return;
  // Pasted HTML is arbitrary — Word styling, whole tables, scripts. Taking the
  // plain text and letting Rust re-apply structure is the only version of this that
  // cannot inject markup the model has no way to represent.
  event.preventDefault();
  const text = (event.clipboardData || window.clipboardData).getData("text/plain");
  if (!text) return;
  // `insertText` rather than a manual range edit, because it participates in the
  // browser's native undo stack.
  document.execCommand("insertText", false, text.replace(/\r?\n/g, " "));
}

document.addEventListener("input", onInput);
document.addEventListener("keydown", onKeyDown, true);
document.addEventListener("focusin", onFocusIn);
document.addEventListener("focusout", onFocusOut);
document.addEventListener("selectionchange", onSelectionChange);
document.addEventListener("paste", onPaste, true);

window.__reportEditorCleanup = () => {
  document.removeEventListener("input", onInput);
  document.removeEventListener("keydown", onKeyDown, true);
  document.removeEventListener("focusin", onFocusIn);
  document.removeEventListener("focusout", onFocusOut);
  document.removeEventListener("selectionchange", onSelectionChange);
  document.removeEventListener("paste", onPaste, true);
};

// Keep the eval alive. Dropping out of scope here would tear down the channel that
// every listener above sends on.
await new Promise(() => {});

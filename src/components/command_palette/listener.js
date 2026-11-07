// The app's global keyboard shortcuts (command_palette/shortcuts.rs). The
// bindings arrive as __TABLE__: { combo: [action, whileTyping, arrows,
// repeats] }. The listener builds each key's combo the way shortcuts::combo
// does, decides here (synchronously) whether the key is ours so it can stop
// the browser's default, and posts [combo, typing] to Rust, which resolves
// it against the same registry and runs the action.
//
// Bubble phase on document: component handlers (Now Playing's Escape and A,
// Cover Flow's arrows, the row gestures' keyboard drag) run first, and any
// key they handled arrives with defaultPrevented. While the palette is open
// (window.__rsPaletteOpen) its input owns the keyboard.
// The script never resolves, so its channel stays open for dioxus.send.
if (window.__rsShortcuts) document.removeEventListener("keydown", window.__rsShortcuts);
{
  const table = __TABLE__;
  const apple = /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent || "");
  const FIELDS = "input, textarea, select, [contenteditable]:not([contenteditable='false'])";
  // Multi-line editors keep every key, ⌘ combos included.
  const EDITORS = "textarea, [contenteditable]:not([contenteditable='false'])";
  // Widgets that move with plain arrows keep them.
  const ARROW_WIDGETS =
    "[role=slider], [role=tablist], [role=tab], [role=radiogroup], [role=listbox], [role=menu], [role=menubar], [role=grid], [role=tree], [role=toolbar], [aria-roledescription=carousel], [data-arrow-keys]";
  // Menus and lists take Space and Enter for their own items.
  const OWN_SPACE = "[role=menu], [role=listbox], [role=combobox]";

  const handler = (event) => {
    if (!event || event.defaultPrevented || event.isComposing || window.__rsPaletteOpen) return;
    // The other platform's modifier (Ctrl on a Mac) is never ours.
    if (apple ? event.ctrlKey : event.metaKey) return;
    const primary = apple ? event.metaKey : event.ctrlKey;
    let key = (event.key || "").toLowerCase();
    if (key === " " || key === "spacebar") key = "space";
    const single = [...key].length === 1;
    const shift = event.shiftKey && !(single && !/^\p{L}$/u.test(key));
    const combo = (primary ? "mod+" : "") + (event.altKey ? "alt+" : "") + (shift ? "shift+" : "") + key;
    const hit = table[combo];
    if (!hit) return;
    const [, whileTyping, arrows, repeats] = hit;
    const target = event.target && event.target.closest ? event.target : null;
    const typing = !!(target && target.closest(FIELDS));
    if (typing && (!whileTyping || target.closest(EDITORS))) return;
    if (arrows && target && target.closest(ARROW_WIDGETS)) return;
    if (key === "space" && target && target.closest(OWN_SPACE)) return;
    event.preventDefault();
    if (event.repeat && !repeats) return;
    dioxus.send([combo, typing]);
  };
  window.__rsShortcuts = handler;
  document.addEventListener("keydown", handler);
}
await new Promise(() => {});

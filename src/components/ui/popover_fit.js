// Keeps anchored popovers (.rs-menu--popover) on screen: writes each one's
// real size to --pop-w / --pop-h, which `popover_style` (menu.rs) clamps
// against, so a menu opened near the bottom or right edge moves inside the
// window instead of overflowing it. Runs once for the whole app.
(() => {
  if (window.__rsPopoverFit) return;
  window.__rsPopoverFit = true;
  const fit = (el) => {
    el.style.setProperty("--pop-w", `${el.offsetWidth}px`);
    el.style.setProperty("--pop-h", `${el.offsetHeight}px`);
  };
  const resize = new ResizeObserver((entries) => entries.forEach((e) => fit(e.target)));
  const watch = (root) => {
    const found = root.matches && root.matches(".rs-menu--popover") ? [root] : [];
    if (root.querySelectorAll) found.push(...root.querySelectorAll(".rs-menu--popover"));
    for (const el of found) {
      if (el.__rsPopoverFit) continue;
      el.__rsPopoverFit = true;
      fit(el);
      resize.observe(el);
    }
  };
  new MutationObserver((records) => {
    for (const record of records) for (const node of record.addedNodes) if (node.nodeType === 1) watch(node);
  }).observe(document.body, { childList: true, subtree: true });
  watch(document);
})();

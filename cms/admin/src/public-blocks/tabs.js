import { cl, decodeHash, direct, focusNoScroll, ga, ra, sa } from "./dom.js";

const ROOT_ATTR = "data-od-tabs-root";
const TAB_SEL = '[role="tab"]';
const PANEL_SEL = '[role="tabpanel"][data-od-panel-index]';
const D_ROOT = `[${ROOT_ATTR}]`;
const A_TAB_IDX = "data-od-tab-index";
const A_PANEL_IDX = "data-od-panel-index";

function activateTab(root, idx, moveFocus) {
  let active = null;
  direct(root, TAB_SEL, ROOT_ATTR).forEach((btn) => {
    const match = String(ga(btn, A_TAB_IDX)) === String(idx);
    sa(btn, "aria-selected", match ? "true" : "false");
    sa(btn, "tabindex", match ? "0" : "-1");
    if (match) {
      sa(btn, "focusgroupstart", "");
      active = btn;
    } else {
      ra(btn, "focusgroupstart");
    }
  });
  direct(root, '[role="tabpanel"]', ROOT_ATTR).forEach((panel) => {
    if (String(ga(panel, A_PANEL_IDX)) === String(idx)) {
      ra(panel, "hidden");
      sa(panel, "tabindex", "0");
    } else {
      sa(panel, "hidden", "until-found");
      ra(panel, "tabindex");
    }
  });
  if (moveFocus && active) {
    focusNoScroll(active);
    try {
      active.scrollIntoView?.({ block: "nearest", inline: "nearest", container: "nearest" });
    } catch {
      try {
        active.scrollIntoView?.({ block: "nearest", inline: "nearest" });
      } catch {}
    }
  }
}

function revealAncestors(target) {
  let cur = cl(target, PANEL_SEL);
  while (cur) {
    const root = cl(cur, D_ROOT);
    const idx = ga(cur, A_PANEL_IDX);
    if (root && idx !== null) activateTab(root, idx, false);
    cur = cl(root?.parentElement, PANEL_SEL);
  }
}

export function initTabs() {
  document.addEventListener("click", (e) => {
    const btn = cl(e.target, TAB_SEL);
    const root = btn && cl(btn, D_ROOT);
    const idx = root && ga(btn, A_TAB_IDX);
    if (idx !== null && idx !== undefined) activateTab(root, idx, true);
  });

  document.addEventListener("keydown", (e) => {
    const btn = cl(e.target, TAB_SEL);
    const root = btn && cl(btn, D_ROOT);
    if (!root) return;
    const tabs = direct(root, TAB_SEL, ROOT_ATTR);
    const cur = tabs.indexOf(btn);
    if (cur < 0 || !tabs.length) return;
    let rtl = false;
    try {
      rtl = window.getComputedStyle(root).direction === "rtl";
    } catch {}
    const k = e.key;
    let next = -1;
    if (k === "ArrowRight" || k === "ArrowDown") {
      next = k === "ArrowRight" && rtl
        ? (cur - 1 + tabs.length) % tabs.length
        : (cur + 1) % tabs.length;
    } else if (k === "ArrowLeft" || k === "ArrowUp") {
      next = k === "ArrowLeft" && rtl
        ? (cur + 1) % tabs.length
        : (cur - 1 + tabs.length) % tabs.length;
    } else if (k === "Home") next = 0;
    else if (k === "End") next = tabs.length - 1;
    if (next >= 0) {
      e.preventDefault();
      const idx = ga(tabs[next], A_TAB_IDX);
      if (idx !== null) activateTab(root, idx, true);
    }
  });

  document.addEventListener("beforematch", (e) => e.target && revealAncestors(e.target), true);

  const onHash = () => {
    const h = window.location.hash;
    if (h && h.length > 1) {
      const el = document.getElementById(decodeHash(h));
      if (el) revealAncestors(el);
    }
  };
  onHash();
  window.addEventListener("hashchange", onHash);
}

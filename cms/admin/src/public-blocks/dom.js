export const MEDIA_RE =
  /^\/media\/[0-9a-f]{64}\/(file|thumbnail|variant\/[a-z0-9_.-]+)(\?v=[1-9]\d*)?$/;

export const qsa = (root, sel) =>
  root ? Array.from(root.querySelectorAll(sel)) : [];

export const qs = (root, sel) => (root ? root.querySelector(sel) : null);

export const cl = (el, sel) => (el && el.closest ? el.closest(sel) : null);

export const ga = (el, k) => el.getAttribute(k);

export const sa = (el, k, v) => el.setAttribute(k, v);

export const ra = (el, k) => el.removeAttribute(k);

export const ha = (el, k) => el.hasAttribute(k);

export const direct = (root, sel, attr) =>
  qsa(root, sel).filter((el) => cl(el, `[${attr}]`) === root);

export const focusNoScroll = (el) => {
  if (!el || el.isConnected === false) return;
  try {
    el.focus({ preventScroll: true });
  } catch {}
};

export const decodeHash = (raw) => {
  const s = raw.startsWith("#") ? raw.slice(1) : raw;
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
};

export const sameDocHash = (link) => {
  if (!link || !link.getAttribute) return null;
  if (link.target === "_blank" || ha(link, "download")) return null;
  try {
    const cur = window.location;
    const u = new URL(ga(link, "href") || "", cur.href);
    if (
      u.origin !== cur.origin ||
      u.pathname !== cur.pathname ||
      u.search !== cur.search ||
      u.hash.length <= 1
    ) {
      return null;
    }
    return decodeHash(u.hash);
  } catch {
    return null;
  }
};

export const outsideRect = (r, x, y) =>
  x < r.left || x > r.right || y < r.top || y > r.bottom;

export const showModalDlg = (dlg) => {
  if (typeof dlg.showModal === "function") {
    if (!dlg.open) dlg.showModal();
  } else {
    sa(dlg, "open", "");
  }
};

export const closeDlg = (dlg) => {
  if (typeof dlg.close === "function" && dlg.open) dlg.close();
  else ra(dlg, "open");
};

export const supportsCss = (prop, val) => {
  try {
    return typeof CSS !== "undefined" && !!CSS.supports?.(prop, val);
  } catch {
    return false;
  }
};

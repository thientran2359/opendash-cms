import { cl, focusNoScroll, ga, qsa, ra, sa } from "./dom.js";

function legacyCopy(text, restoreEl) {
  const ta = document.createElement("textarea");
  ta.value = text;
  sa(ta, "readonly", "");
  ta.style.position = "fixed";
  ta.style.left = "-9999px";
  document.body.appendChild(ta);
  ta.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch {
    ok = false;
  }
  document.body.removeChild(ta);
  focusNoScroll(restoreEl);
  return ok;
}

export function initShare() {
  qsa(document, "[data-od-share-href][data-od-share-path]").forEach((a) => {
    const tpl = ga(a, "data-od-share-href") || "";
    let p = ga(a, "data-od-share-path") || "/";
    if (p.startsWith("//") || !p.startsWith("/")) p = "/";
    sa(a, "href", tpl.replace("__OD_SHARE_URL__", encodeURIComponent(window.location.origin + p)));
  });

  let flip = false;
  document.addEventListener("click", (e) => {
    const btn = cl(e.target, "[data-od-copy-url]");
    if (!btn) return;
    const raw = ga(btn, "data-od-copy-url") || "";
    const url = raw.startsWith("/") && !raw.startsWith("//")
      ? window.location.origin + raw
      : raw || window.location.href;
    const okText = ga(btn, "data-od-copied-label") || "Copied";
    const failText = ga(btn, "data-od-copy-failed-label") || "Copy failed";
    const status = btn.nextElementSibling && ga(btn.nextElementSibling, "role") === "status"
      ? btn.nextElementSibling
      : null;

    const show = (ok) => {
      ra(btn, ok ? "data-failed" : "data-copied");
      sa(btn, ok ? "data-copied" : "data-failed", "1");
      flip = !flip;
      if (status) status.textContent = (ok ? okText : failText) + (flip ? "\u00A0" : "");
      setTimeout(() => {
        ra(btn, "data-copied");
        ra(btn, "data-failed");
      }, 2000);
    };

    const cp = navigator.clipboard;
    if (cp && typeof cp.writeText === "function") {
      cp.writeText(url).then(() => show(true), () => show(legacyCopy(url, btn)));
    } else {
      show(legacyCopy(url, btn));
    }
  });
}

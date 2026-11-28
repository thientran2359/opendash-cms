import {
  cl, closeDlg, decodeHash, focusNoScroll, ga, ha, outsideRect, qs, qsa, sa, sameDocHash,
  showModalDlg, supportsCss,
} from "./dom.js";

const P_SEL = "dialog[data-od-popup-dialog]";
const A_ID = "data-od-popup-dialog";
const A_TRIG = "data-od-popup-trigger";
const A_BACK = "data-od-popup-backdrop-close";
const A_EXP = "aria-expanded";

export function initPopups() {
  const popups = qsa(document, P_SEL);
  if (!popups.length) return;

  const hasDiscrete =
    supportsCss("transition-behavior", "allow-discrete") && supportsCss("overlay", "auto");
  const byId = (id) => popups.find((d) => ga(d, A_ID) === id) || null;
  const setExp = (t, v) => t?.setAttribute && sa(t, A_EXP, String(v));
  const isOpen = (d) => !!(d && (d.open || ha(d, "open")));

  function closePopup(dlg) {
    if (!isOpen(dlg)) return;
    if (!hasDiscrete) {
      if (dlg.dataset.closing === "1") return;
      const seq = (dlg.__odSeq = (dlg.__odSeq || 0) + 1);
      dlg.dataset.closing = "1";
      const anims = typeof dlg.getAnimations === "function" ? dlg.getAnimations({ subtree: true }) : [];
      if (anims.length) {
        Promise.allSettled(anims.map((a) => a.finished)).then(() => {
          if (dlg.__odSeq !== seq) return;
          delete dlg.dataset.closing;
          closeDlg(dlg);
        });
        return;
      }
      delete dlg.dataset.closing;
    }
    closeDlg(dlg);
  }

  function openPopup(dlg, trigger) {
    popups.forEach((other) => {
      if (other !== dlg && isOpen(other)) {
        other.__odSeq = (other.__odSeq || 0) + 1;
        delete other.dataset.closing;
        setExp(other.__odTrigger, false);
        other.__odTrigger = null;
        closeDlg(other);
      }
    });
    dlg.__odSeq = (dlg.__odSeq || 0) + 1;
    delete dlg.dataset.closing;
    dlg.__odTrigger = trigger || null;
    if (trigger?.setAttribute && dlg.id) {
      sa(trigger, "aria-controls", dlg.id);
      setExp(trigger, true);
    }
    showModalDlg(dlg);
  }

  popups.forEach((dlg) => {
    let downOut = false;
    const surfRect = () => (qs(dlg, ".od-popup-surface") || dlg).getBoundingClientRect();
    dlg.addEventListener("cancel", (e) => {
      if (!hasDiscrete) {
        e.preventDefault();
        closePopup(dlg);
      }
    });
    dlg.addEventListener("close", () => {
      delete dlg.dataset.closing;
      qsa(dlg, "video, audio").forEach((m) => m.pause?.());
      const trig = dlg.__odTrigger;
      dlg.__odTrigger = null;
      setExp(trig, false);
      focusNoScroll(trig);
    });
    dlg.addEventListener("pointerdown", (e) => {
      downOut = ga(dlg, A_BACK) === "true" && outsideRect(surfRect(), e.clientX, e.clientY);
    });
    dlg.addEventListener("pointerup", (e) => {
      if (!downOut) return;
      downOut = false;
      if (ga(dlg, A_BACK) === "true" && outsideRect(surfRect(), e.clientX, e.clientY)) {
        closePopup(dlg);
      }
    });
  });

  qsa(document, `[${A_TRIG}], a[href]`).forEach((el) => {
    const id = ha(el, A_TRIG) ? (ga(el, A_TRIG) || "").trim().replace(/^#/, "") : sameDocHash(el);
    const dlg = id && byId(id);
    if (dlg?.id) {
      sa(el, "aria-haspopup", "dialog");
      sa(el, "aria-controls", dlg.id);
      sa(el, A_EXP, "false");
    }
  });

  document.addEventListener("click", (e) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const closeEl = cl(e.target, "[data-od-popup-close]");
    const parentDlg = closeEl && cl(closeEl, P_SEL);
    if (parentDlg) {
      e.preventDefault();
      return closePopup(parentDlg);
    }
    const trig = cl(e.target, `[${A_TRIG}]`);
    if (trig) {
      const targetDlg = byId((ga(trig, A_TRIG) || "").trim().replace(/^#/, ""));
      if (targetDlg) {
        e.preventDefault();
        return openPopup(targetDlg, trig);
      }
    }
    const link = cl(e.target, "a[href]");
    const hashId = link && sameDocHash(link);
    if (!hashId) return;
    const matched = byId(hashId);
    if (matched) {
      e.preventDefault();
      return openPopup(matched, link);
    }
    const host = cl(link, P_SEL);
    if (isOpen(host)) closePopup(host);
  });

  const onHash = () => {
    const h = window.location.hash;
    if (!h || h.length <= 1) return;
    const dlg = byId(decodeHash(h));
    if (dlg && ga(dlg, "data-od-popup-hash") === "true") openPopup(dlg, null);
  };
  onHash();
  window.addEventListener("hashchange", onHash);
}

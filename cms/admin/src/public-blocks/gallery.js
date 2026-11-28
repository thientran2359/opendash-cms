import { closeDlg, focusNoScroll, ga, MEDIA_RE, outsideRect, qs, qsa, showModalDlg } from "./dom.js";

export function initGalleries() {
  qsa(document, ".od-gallery").forEach((gallery) => {
    const dlg = qs(gallery, "dialog.od-lightbox-dialog");
    const trigs = qsa(gallery, "[data-od-lb-trigger]");
    if (!dlg || !trigs.length) return;

    const img = qs(dlg, "[data-od-lb-img]");
    const capEl = qs(dlg, "[data-od-lb-caption]");
    const cntEl = qs(dlg, "[data-od-lb-counter]");
    let idx = 0;
    let lastTrig = null;
    let downOut = false;

    const showAt = (n) => {
      idx = (n + trigs.length) % trigs.length;
      const t = trigs[idx];
      const src = ga(t, "data-od-lb-src") || "";
      if (!MEDIA_RE.test(src)) return;
      if (img) {
        img.src = src;
        img.alt = ga(t, "data-od-lb-alt") || "";
      }
      if (capEl) capEl.textContent = ga(t, "data-od-lb-caption") || "";
      if (cntEl) cntEl.textContent = `${idx + 1} / ${trigs.length}`;
    };

    trigs.forEach((t, i) =>
      t.addEventListener("click", () => {
        if (!MEDIA_RE.test(ga(t, "data-od-lb-src") || "")) return;
        lastTrig = t;
        showAt(i);
        showModalDlg(dlg);
      })
    );

    qs(dlg, "[data-od-lb-prev]")?.addEventListener("click", () => showAt(idx - 1));
    qs(dlg, "[data-od-lb-next]")?.addEventListener("click", () => showAt(idx + 1));
    qs(dlg, "[data-od-lb-close]")?.addEventListener("click", () => closeDlg(dlg));

    dlg.addEventListener("keydown", (e) => {
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.preventDefault();
        showAt(idx + (e.key === "ArrowRight" ? 1 : -1));
      }
    });

    const boxRect = () => (qs(dlg, ".od-lightbox-content") || img || dlg).getBoundingClientRect();
    dlg.addEventListener("pointerdown", (e) => {
      downOut = e.target === dlg && outsideRect(boxRect(), e.clientX, e.clientY);
    });
    dlg.addEventListener("pointerup", (e) => {
      if (!downOut) return;
      downOut = false;
      if (e.target === dlg && outsideRect(boxRect(), e.clientX, e.clientY)) closeDlg(dlg);
    });
    dlg.addEventListener("close", () => {
      focusNoScroll(lastTrig);
      lastTrig = null;
    });
  });
}

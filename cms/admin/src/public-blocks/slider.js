import { direct, focusNoScroll, ga, qs, qsa, ra, sa, supportsCss } from "./dom.js";

const R_ATTR = "data-od-slider-root";

function setupSlider(root, reduced) {
  const track = direct(root, ".od-slider-track", R_ATTR)[0];
  if (!track) return;
  const slides = qsa(track, "[data-od-slide-index]").filter((s) => s.parentElement === track);
  if (slides.length < 2) return;

  const isLoop =
    (ga(root, "data-od-slider-spv") || "1") === "1" &&
    ga(root, "data-od-slider-loop") === "true" &&
    slides.length >= 3;
  const autoSec = parseInt(ga(root, "data-od-slider-autoplay") || "0", 10) || 0;
  const statusEl = direct(root, "[data-od-slider-status]", R_ATTR)[0];
  const statusTpl = ga(root, "data-od-label-slide") || "Slide {n} of {total}";
  const pauseLbl = ga(root, "data-od-label-pause") || "Pause autoplay";
  const playLbl = ga(root, "data-od-label-play") || "Play autoplay";
  const prevBtn = direct(root, '[data-od-slide-dir="prev"]', R_ATTR)[0];
  const nextBtn = direct(root, '[data-od-slide-dir="next"]', R_ATTR)[0];
  const dots = direct(root, "[data-od-slide-to]", R_ATTR);
  const toggleBtn = direct(root, "[data-od-slider-toggle]", R_ATTR)[0];

  let cur = 0;
  let scrollT = null;
  let autoT = null;
  let paused = reduced;

  const hasCssOrder = supportsCss("order", "mod(calc(sibling-index() - 1), 3)");
  const applyOrder = (idx) => {
    if (!hasCssOrder) {
      slides.forEach((sl, i) => {
        sl.style.order = String(((i - idx) % slides.length + slides.length) % slides.length);
      });
    }
  };
  if (isLoop) {
    if (!supportsCss("order", "sibling-count()")) {
      root.style.setProperty("--children-count", String(slides.length));
    }
    root.style.setProperty("--current", "0");
    applyOrder(0);
  }

  const syncUi = (idx) => {
    cur = idx;
    dots.forEach((d, i) => (i === idx ? sa(d, "aria-current", "true") : ra(d, "aria-current")));
    if (!isLoop && prevBtn && nextBtn) {
      const pDis = idx <= 0;
      const nDis = idx >= slides.length - 1;
      const act = document.activeElement;
      const pFocus = act === prevBtn;
      const nFocus = act === nextBtn;
      prevBtn.disabled = pDis;
      nextBtn.disabled = nDis;
      if (pDis && pFocus && !nDis) focusNoScroll(nextBtn);
      else if (nDis && nFocus && !pDis) focusNoScroll(prevBtn);
    }
    if (statusEl) {
      statusEl.textContent = statusTpl
        .replace("{n}", String(idx + 1))
        .replace("{total}", String(slides.length));
    }
  };

  const park = (idx) => {
    root.classList.add("od-slider-parking");
    root.style.setProperty("--current", String(idx));
    applyOrder(idx);
    track.scrollLeft = 0;
    const clear = () => root.classList.remove("od-slider-parking");
    const raf = window.requestAnimationFrame;
    if (typeof raf === "function") raf(() => raf(clear));
    else clear();
  };

  const goTo = (target, directDot) => {
    const next = isLoop
      ? (target + slides.length) % slides.length
      : Math.max(0, Math.min(slides.length - 1, target));
    const s = slides[next];
    if (isLoop) {
      if (directDot) root.style.removeProperty("--current");
      if (s && track.scrollTo) {
        track.scrollTo({ left: s.offsetLeft || 0, behavior: reduced ? "auto" : "smooth" });
      }
      park(next);
    } else if (s) {
      const left = s.offsetLeft;
      if (track.scrollTo) {
        track.scrollTo({ left, behavior: reduced ? "auto" : "smooth" });
      } else {
        track.scrollLeft = left;
      }
    }
    syncUi(next);
  };

  const inferNearest = () => {
    let best = 0;
    let dist = Infinity;
    const left = track.scrollLeft || 0;
    slides.forEach((s, i) => {
      const d = Math.abs((s.offsetLeft || 0) - left);
      if (d < dist) {
        dist = d;
        best = i;
      }
    });
    syncUi(best);
  };

  if ("onscrollend" in track) track.addEventListener("scrollend", inferNearest);
  else {
    track.addEventListener("scroll", () => {
      clearTimeout(scrollT);
      scrollT = setTimeout(inferNearest, 120);
    });
  }

  prevBtn?.addEventListener("click", () => goTo(cur - 1, false));
  nextBtn?.addEventListener("click", () => goTo(cur + 1, false));
  dots.forEach((dot) =>
    dot.addEventListener("click", () => goTo(parseInt(ga(dot, "data-od-slide-to") || "0", 10) || 0, true))
  );

  const stopAuto = () => {
    if (autoT) {
      clearInterval(autoT);
      autoT = null;
    }
  };
  const syncToggle = () => {
    if (!toggleBtn) return;
    sa(toggleBtn, "aria-pressed", String(paused));
    sa(toggleBtn, "aria-label", paused ? playLbl : pauseLbl);
    qs(toggleBtn, ".od-pause-glyph")?.classList.toggle("hidden", paused);
    qs(toggleBtn, ".od-play-glyph")?.classList.toggle("hidden", !paused);
  };
  const startAuto = () => {
    stopAuto();
    if (autoSec <= 0 || paused || document.hidden) return;
    autoT = setInterval(() => goTo(!isLoop && cur >= slides.length - 1 ? 0 : cur + 1, false), autoSec * 1000);
  };

  if (toggleBtn && autoSec > 0) {
    syncToggle();
    toggleBtn.addEventListener("click", () => {
      paused = !paused;
      syncToggle();
      if (paused) stopAuto();
      else startAuto();
    });
  }
  if (autoSec > 0) {
    root.addEventListener("mouseenter", stopAuto);
    root.addEventListener("mouseleave", startAuto);
    root.addEventListener("focusin", stopAuto);
    root.addEventListener("focusout", () =>
      setTimeout(() => !root.contains(document.activeElement) && startAuto(), 0)
    );
    document.addEventListener("visibilitychange", () => (document.hidden ? stopAuto() : startAuto()));
    startAuto();
  }
  syncUi(0);
}

export function initSliders() {
  let reduced = false;
  try {
    reduced = !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  } catch {}
  qsa(document, `[${R_ATTR}]`).forEach((r) => setupSlider(r, reduced));
}

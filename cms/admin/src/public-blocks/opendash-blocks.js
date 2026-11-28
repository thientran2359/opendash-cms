import { initTabs } from "./tabs.js";
import { initSliders } from "./slider.js";
import { initGalleries } from "./gallery.js";
import { initShare } from "./share.js";
import { initPopups } from "./popup.js";

function boot() {
  if (typeof window === "undefined" || window.__odBlocksInit) return;
  window.__odBlocksInit = true;
  initTabs();
  initSliders();
  initGalleries();
  initShare();
  initPopups();
}

if (typeof document !== "undefined") {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot, { once: true });
  } else {
    boot();
  }
}

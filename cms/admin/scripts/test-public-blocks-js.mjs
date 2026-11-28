/**
 * Unit tests for public micro-JS pack opendash-blocks.js (Tabs, Slider, Lightbox, Share, Popup).
 * Zero external runtime dependencies; mocks strict DOM primitives.
 */
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const adminDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = resolve(adminDir, '../..');
const sourcePath = join(repoRoot, 'cms/crates/opendash-server/src/builtin_themes/opendash-blocks.js');

class MockEl {
  constructor(tag, attrs = {}, parent = null) {
    this.tagName = tag.toUpperCase();
    this.attributes = new Map(Object.entries(attrs).map(([k, v]) => [k, String(v)]));
    this.children = [];
    this.parentElement = parent;
    this.listeners = new Map();
    this.dataset = {};
    this.style = {
      props: new Map(),
      setProperty: (k, v) => this.style.props.set(k, String(v)),
      removeProperty: (k) => this.style.props.delete(k),
    };
    this.classList = {
      set: new Set((attrs.class || '').split(/\s+/).filter(Boolean)),
      add: (c) => this.classList.set.add(c),
      remove: (c) => this.classList.set.delete(c),
      toggle: (c, f) => (f === undefined ? (this.classList.set.has(c) ? this.classList.set.delete(c) : this.classList.set.add(c)) : f ? this.classList.set.add(c) : this.classList.set.delete(c)),
      contains: (c) => this.classList.set.has(c),
    };
    this.textContent = '';
    this.src = attrs.src || '';
    this.alt = attrs.alt || '';
    this.id = attrs.id || '';
    this.open = this.attributes.has('open');
    this.disabled = false;
    this.scrollLeft = 0;
    this.offsetLeft = 0;
    this.isConnected = true;
    this.paused = false;
    this._rect = { left: 100, top: 100, right: 400, bottom: 400 };
    if (parent) parent.children.push(this);
  }
  get nextElementSibling() {
    if (!this.parentElement) return null;
    const i = this.parentElement.children.indexOf(this);
    return i >= 0 && i + 1 < this.parentElement.children.length ? this.parentElement.children[i + 1] : null;
  }
  get target() { return this.getAttribute('target') || ''; }
  appendChild(c) { c.parentElement = this; this.children.push(c); return c; }
  removeChild(c) { this.children = this.children.filter((x) => x !== c); c.parentElement = null; return c; }
  contains(t) {
    let cur = t;
    while (cur) { if (cur === this) return true; cur = cur.parentElement; }
    return false;
  }
  getAttribute(k) { return this.attributes.has(k) ? this.attributes.get(k) : null; }
  setAttribute(k, v) { this.attributes.set(k, String(v)); if (k === 'id') this.id = String(v); if (k === 'open') this.open = true; if (k === 'class') this.classList.set = new Set(String(v).split(/\s+/).filter(Boolean)); }
  removeAttribute(k) { this.attributes.delete(k); if (k === 'open') this.open = false; }
  hasAttribute(k) { return this.attributes.has(k); }
  addEventListener(type, fn) { if (!this.listeners.has(type)) this.listeners.set(type, []); this.listeners.get(type).push(fn); }
  dispatchEvent(ev) {
    ev.target = ev.target || this;
    if (ev.cancelable === undefined) ev.cancelable = true;
    for (const fn of this.listeners.get(ev.type) || []) fn(ev);
    if (ev.bubbles && this.parentElement) this.parentElement.dispatchEvent(ev);
    return !ev.defaultPrevented;
  }
  matches(sel) {
    return sel.split(',').map((s) => s.trim()).some((part) => {
      if (part === '[data-od-tabs-root]') return this.hasAttribute('data-od-tabs-root');
      if (part === '[data-od-slider-root]') return this.hasAttribute('data-od-slider-root');
      if (part === '.od-gallery') return this.classList.contains('od-gallery');
      if (part === '.od-slider-track') return this.classList.contains('od-slider-track');
      if (part === '.od-popup-surface') return this.classList.contains('od-popup-surface');
      if (part === '[role="tab"]') return this.getAttribute('role') === 'tab';
      if (part === '[role="tabpanel"][data-od-panel-index]') return this.getAttribute('role') === 'tabpanel' && this.hasAttribute('data-od-panel-index');
      if (part === '[role="tabpanel"]') return this.getAttribute('role') === 'tabpanel';
      if (part === '[data-od-slide-index]') return this.hasAttribute('data-od-slide-index');
      if (part === '[data-od-lb-trigger]') return this.hasAttribute('data-od-lb-trigger');
      if (part === '[data-od-lb-img]') return this.hasAttribute('data-od-lb-img');
      if (part === '[data-od-lb-caption]') return this.hasAttribute('data-od-lb-caption');
      if (part === '[data-od-lb-counter]') return this.hasAttribute('data-od-lb-counter');
      if (part === '[data-od-lb-prev]') return this.hasAttribute('data-od-lb-prev');
      if (part === '[data-od-lb-next]') return this.hasAttribute('data-od-lb-next');
      if (part === '[data-od-lb-close]') return this.hasAttribute('data-od-lb-close');
      if (part === '[data-od-copy-url]') return this.hasAttribute('data-od-copy-url');
      if (part === '[data-od-share-href][data-od-share-path]') return this.hasAttribute('data-od-share-href') && this.hasAttribute('data-od-share-path');
      if (part === '[data-od-slider-status]') return this.hasAttribute('data-od-slider-status');
      if (part === '[data-od-slide-dir="prev"]') return this.getAttribute('data-od-slide-dir') === 'prev';
      if (part === '[data-od-slide-dir="next"]') return this.getAttribute('data-od-slide-dir') === 'next';
      if (part === '[data-od-slide-to]') return this.hasAttribute('data-od-slide-to');
      if (part === '[data-od-slider-toggle]') return this.hasAttribute('data-od-slider-toggle');
      if (part === 'dialog.od-lightbox-dialog') return this.tagName === 'DIALOG' && this.classList.contains('od-lightbox-dialog');
      if (part === 'dialog[data-od-popup-dialog]') return this.tagName === 'DIALOG' && this.hasAttribute('data-od-popup-dialog');
      if (part === '[data-od-popup-close]') return this.hasAttribute('data-od-popup-close');
      if (part === '[data-od-popup-trigger]') return this.hasAttribute('data-od-popup-trigger');
      if (part === 'a[href]') return this.tagName === 'A' && this.hasAttribute('href');
      if (part === 'video') return this.tagName === 'VIDEO';
      if (part === 'audio') return this.tagName === 'AUDIO';
      return false;
    });
  }
  closest(sel) {
    let cur = this;
    while (cur) { if (cur.matches && cur.matches(sel)) return cur; cur = cur.parentElement; }
    return null;
  }
  querySelectorAll(sel) {
    const out = [];
    const walk = (n) => {
      for (const c of n.children) {
        if (c.matches && c.matches(sel)) out.push(c);
        walk(c);
      }
    };
    walk(this);
    return out;
  }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
  focus() { this.ownerDoc.activeElement = this; }
  select() {}
  getBoundingClientRect() { return this._rect; }
  scrollIntoView() {}
  scrollTo({ left }) { this.scrollLeft = left; }
  pause() { this.paused = true; }
}

function createEnv({ allowDiscrete = true, overlay = true, reduced = false, hash = '' } = {}) {
  const root = new MockEl('html');
  const body = new MockEl('body', {}, root);
  const docListeners = new Map();
  const winListeners = new Map();
  let clipboardText = null;
  let execCopied = false;

  const doc = {
    readyState: 'complete',
    body,
    activeElement: body,
    hidden: false,
    execCommand: (cmd) => { if (cmd === 'copy') { execCopied = true; return true; } return false; },
    createElement: (tag) => { const el = new MockEl(tag); el.ownerDoc = doc; return el; },
    getElementById: (id) => {
      const walk = (n) => {
        for (const c of n.children) {
          if (c.id === id) return c;
          const hit = walk(c);
          if (hit) return hit;
        }
        return null;
      };
      return walk(root);
    },
    querySelectorAll: (sel) => root.querySelectorAll(sel),
    querySelector: (sel) => root.querySelector(sel),
    addEventListener: (t, fn) => { if (!docListeners.has(t)) docListeners.set(t, []); docListeners.get(t).push(fn); },
    dispatchEvent: (ev) => {
      for (const fn of docListeners.get(ev.type) || []) fn(ev);
    },
  };
  body.ownerDoc = doc;
  root.ownerDoc = doc;

  const el = (tag, attrs, parent) => {
    const node = new MockEl(tag, attrs, parent);
    node.ownerDoc = doc;
    return node;
  };

  const win = {
    location: new URL('https://example.test/page' + hash),
    navigator: {
      clipboard: {
        writeText: async (text) => { clipboardText = text; },
      },
    },
    CSS: {
      supports: (p, v) => {
        if (p === 'order' && v === 'sibling-count()') return false;
        if (p === 'transition-behavior' && v === 'allow-discrete') return allowDiscrete;
        if (p === 'overlay' && v === 'auto') return overlay;
        return false;
      },
    },
    matchMedia: () => ({ matches: reduced }),
    getComputedStyle: () => ({ direction: 'ltr' }),
    addEventListener: (t, fn) => { if (!winListeners.has(t)) winListeners.set(t, []); winListeners.get(t).push(fn); },
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    URL,
    Promise,
  };
  win.document = doc;
  win.window = win;

  function makeEvent(type, target, extra = {}) {
    return {
      type,
      target,
      defaultPrevented: false,
      button: 0,
      metaKey: false,
      ctrlKey: false,
      shiftKey: false,
      altKey: false,
      bubbles: true,
      preventDefault() { this.defaultPrevented = true; },
      ...extra,
    };
  }

  return {
    root,
    body,
    doc,
    win,
    el,
    makeEvent,
    getClipboard: () => clipboardText,
    didExecCopy: () => execCopied,
    emitDocClick: (target, extra) => {
      const ev = makeEvent('click', target, extra);
      doc.dispatchEvent(ev);
      return ev;
    },
    emitWinEvent: (type) => {
      for (const fn of winListeners.get(type) || []) fn({ type });
    },
  };
}

async function runTests() {
  const code = await readFile(sourcePath, 'utf8');

  // Test 1: Nested Tabs + RTL + beforematch + hash reveal
  {
    const env = createEnv({ hash: '#deep-anchor' });
    const outer = env.el('div', { 'data-od-tabs-root': '' }, env.body);
    const t0 = env.el('button', { role: 'tab', 'data-od-tab-index': '0', 'aria-selected': 'true', tabindex: '0' }, outer);
    const t1 = env.el('button', { role: 'tab', 'data-od-tab-index': '1', 'aria-selected': 'false', tabindex: '-1' }, outer);
    const p0 = env.el('div', { role: 'tabpanel', 'data-od-panel-index': '0', tabindex: '0' }, outer);
    const p1 = env.el('div', { role: 'tabpanel', 'data-od-panel-index': '1', hidden: 'until-found' }, outer);
    const inner = env.el('div', { 'data-od-tabs-root': '' }, p1);
    const it0 = env.el('button', { role: 'tab', 'data-od-tab-index': '0', 'aria-selected': 'true', tabindex: '0' }, inner);
    const it1 = env.el('button', { role: 'tab', 'data-od-tab-index': '1', 'aria-selected': 'false', tabindex: '-1' }, inner);
    const ip0 = env.el('div', { role: 'tabpanel', 'data-od-panel-index': '0', tabindex: '0' }, inner);
    const ip1 = env.el('div', { role: 'tabpanel', 'data-od-panel-index': '1', hidden: 'until-found' }, inner);
    env.el('span', { id: 'deep-anchor' }, ip1);

    vm.runInNewContext(code, { ...env.win, window: env.win, document: env.doc, navigator: env.win.navigator, CSS: env.win.CSS, URL });

    // Initial #deep-anchor reveals both outer p1 and inner ip1
    assert.equal(p1.hasAttribute('hidden'), false);
    assert.equal(ip1.hasAttribute('hidden'), false);
    assert.equal(p0.getAttribute('hidden'), 'until-found');
    assert.equal(t1.getAttribute('aria-selected'), 'true');
    assert.equal(it1.getAttribute('aria-selected'), 'true');

    // Keyboard navigation on outer tabs preserves inner tab state independently
    const keyEv = env.makeEvent('keydown', t1, { key: 'Home' });
    env.doc.dispatchEvent(keyEv);
    assert.equal(p0.hasAttribute('hidden'), false);
    assert.equal(t0.getAttribute('aria-selected'), 'true');
    assert.equal(p1.getAttribute('hidden'), 'until-found');
    assert.equal(it1.getAttribute('aria-selected'), 'true');
    assert.equal(ip1.hasAttribute('hidden'), false);

    // beforematch on hidden nested anchor re-opens both without error
    env.doc.dispatchEvent(env.makeEvent('beforematch', ip1));
    assert.equal(p1.hasAttribute('hidden'), false);
    assert.equal(ip1.hasAttribute('hidden'), false);
  }

  // Test 2: Slider finite focus redirect & order-loop CSS vars
  {
    const env = createEnv();
    const slider = env.el('section', {
      'data-od-slider-root': '',
      'data-od-slider-spv': '1',
      'data-od-slider-loop': 'true',
      'data-od-label-slide': 'Slide {n}/{total}',
    }, env.body);
    const prev = env.el('button', { 'data-od-slide-dir': 'prev' }, slider);
    const next = env.el('button', { 'data-od-slide-dir': 'next' }, slider);
    const track = env.el('div', { class: 'od-slider-track' }, slider);
    const s0 = env.el('div', { 'data-od-slide-index': '0' }, track); s0.offsetLeft = 0;
    const s1 = env.el('div', { 'data-od-slide-index': '1' }, track); s1.offsetLeft = 400;
    const s2 = env.el('div', { 'data-od-slide-index': '2' }, track); s2.offsetLeft = 800;
    const status = env.el('p', { 'data-od-slider-status': '' }, slider);

    // Finite multi-column slider for focus redirect check
    const finite = env.el('section', {
      'data-od-slider-root': '',
      'data-od-slider-spv': 'multi',
      'data-od-slider-loop': 'false',
    }, env.body);
    const fPrev = env.el('button', { 'data-od-slide-dir': 'prev' }, finite);
    const fNext = env.el('button', { 'data-od-slide-dir': 'next' }, finite);
    const fTrack = env.el('div', { class: 'od-slider-track' }, finite);
    env.el('div', { 'data-od-slide-index': '0' }, fTrack);
    const fs1 = env.el('div', { 'data-od-slide-index': '1' }, fTrack); fs1.offsetLeft = 300;

    vm.runInNewContext(code, { ...env.win, window: env.win, document: env.doc, navigator: env.win.navigator, CSS: env.win.CSS, URL });
    assert.equal(slider.style.props.get('--children-count'), '3');
    next.dispatchEvent(env.makeEvent('click', next));
    assert.equal(slider.style.props.get('--current'), '1');
    assert.equal(status.textContent, 'Slide 2/3');
    prev.dispatchEvent(env.makeEvent('click', prev));
    assert.equal(slider.style.props.get('--current'), '0');

    // Finite boundary disables fNext on last slide and redirects focus to fPrev
    fNext.focus();
    fNext.dispatchEvent(env.makeEvent('click', fNext));
    assert.equal(fNext.disabled, true);
    assert.equal(env.doc.activeElement, fPrev);
  }

  // Test 3: Gallery Lightbox regex + Share relative URL + Copy fallback
  {
    const env = createEnv();
    const validSrc = '/media/' + 'a'.repeat(64) + '/file?v=2';
    const gallery = env.el('div', { class: 'od-gallery' }, env.body);
    const trigValid = env.el('button', { 'data-od-lb-trigger': '', 'data-od-lb-src': validSrc, 'data-od-lb-alt': 'Hero' }, gallery);
    const trigBad = env.el('button', { 'data-od-lb-trigger': '', 'data-od-lb-src': '/api/admin/media/abc' }, gallery);
    const lbDlg = env.el('dialog', { class: 'od-lightbox-dialog' }, gallery);
    lbDlg.showModal = function () { this.open = true; this.setAttribute('open', ''); };
    lbDlg.close = function () { this.open = false; this.removeAttribute('open'); this.dispatchEvent({ type: 'close' }); };
    const lbImg = env.el('img', { 'data-od-lb-img': '' }, lbDlg);

    const shareLink = env.el('a', {
      href: '#share',
      'data-od-share-href': 'https://example.invalid/?u=__OD_SHARE_URL__',
      'data-od-share-path': '/bai-viet/demo',
    }, env.body);
    const copyWrap = env.el('div', {}, env.body);
    const copyBtn = env.el('button', {
      'data-od-copy-url': '/bai-viet/demo',
      'data-od-copied-label': 'Đã chép',
    }, copyWrap);
    const copyStatus = env.el('span', { role: 'status' }, copyWrap);

    vm.runInNewContext(code, { ...env.win, window: env.win, document: env.doc, navigator: env.win.navigator, CSS: env.win.CSS, URL });

    // Rejects invalid /api/admin/media src
    trigBad.dispatchEvent(env.makeEvent('click', trigBad));
    assert.equal(lbDlg.open, false);

    // Accepts valid /media/<64hex>/file?v=2
    trigValid.dispatchEvent(env.makeEvent('click', trigValid));
    assert.equal(lbDlg.open, true);
    assert.equal(lbImg.src, validSrc);

    // Relative share link resolved with zero http:// constant in JS
    assert.equal(shareLink.getAttribute('href'), 'https://example.invalid/?u=' + encodeURIComponent('https://example.test/bai-viet/demo'));

    // Copy Link updates sibling role="status" and does not disable button
    env.emitDocClick(copyBtn);
    await new Promise((r) => setTimeout(r, 10));
    assert.equal(env.getClipboard(), 'https://example.test/bai-viet/demo');
    assert.equal(copyBtn.disabled, false);
    assert.equal(copyBtn.getAttribute('data-copied'), '1');
    assert.equal(copyStatus.textContent.trim(), 'Đã chép');
  }

  // Test 4: Popup — #id trigger, same-doc guard, Firefox/Safari [data-closing] + closedby fallback, video pause, focus guard
  {
    const env = createEnv({ allowDiscrete: true, overlay: false, hash: '#promo' });
    const trigOther = env.el('a', { href: '/other-page#promo' }, env.body);
    const trigBlank = env.el('a', { href: '#promo', target: '_blank' }, env.body);
    const trigGood = env.el('a', { href: '#drawer-nav' }, env.body);

    const promoDlg = env.el('dialog', {
      id: 'od-popup-dlg-s1',
      'data-od-popup-dialog': 'promo',
      'data-od-popup-hash': 'true',
      'data-od-popup-backdrop-close': 'false',
    }, env.body);
    const navDlg = env.el('dialog', {
      id: 'od-popup-dlg-s2',
      'data-od-popup-dialog': 'drawer-nav',
      'data-od-popup-hash': 'false',
      'data-od-popup-backdrop-close': 'true',
    }, env.body);
    for (const d of [promoDlg, navDlg]) {
      let animResolve;
      d.showModal = function () { this.open = true; this.setAttribute('open', ''); };
      d.close = function () { this.open = false; this.removeAttribute('open'); this.dispatchEvent({ type: 'close' }); };
      d.getAnimations = () => [{ finished: new Promise((res) => { animResolve = res; d.__finishAnim = animResolve; }) }];
    }
    const navSurface = env.el('div', { class: 'od-popup-surface' }, navDlg);
    const navVid = env.el('video', {}, navSurface);
    const sectionLink = env.el('a', { href: '#features' }, navSurface);

    vm.runInNewContext(code, { ...env.win, window: env.win, document: env.doc, navigator: env.win.navigator, CSS: env.win.CSS, URL });

    // Open on hash when trigger === null: open without crashing on close
    assert.equal(promoDlg.open, true);

    // Other-page or target="_blank" links must NOT hijack navigation
    const otherEv = env.emitDocClick(trigOther);
    assert.equal(otherEv.defaultPrevented, false);
    const blankEv = env.emitDocClick(trigBlank);
    assert.equal(blankEv.defaultPrevented, false);

    // Clicking #drawer-nav closes promoDlg, opens navDlg, sets aria-expanded="true"
    trigGood.focus();
    const openEv = env.emitDocClick(trigGood);
    assert.equal(openEv.defaultPrevented, true);
    assert.equal(promoDlg.open, false);
    assert.equal(navDlg.open, true);
    assert.equal(trigGood.getAttribute('aria-expanded'), 'true');

    // Safari backdrop fallback when backdrop-close=true triggers [data-closing] (since overlay=false)
    navDlg.dispatchEvent(env.makeEvent('pointerdown', navDlg, { clientX: 10, clientY: 10 }));
    navDlg.dispatchEvent(env.makeEvent('pointerup', navDlg, { clientX: 10, clientY: 10 }));
    assert.equal(navDlg.dataset.closing, '1');
    assert.equal(navDlg.open, true);
    navDlg.__finishAnim();
    await new Promise((r) => setTimeout(r, 10));
    assert.equal(navDlg.open, false);
    assert.equal(navVid.paused, true);
    assert.equal(trigGood.getAttribute('aria-expanded'), 'false');
    assert.equal(env.doc.activeElement, trigGood);

    // Re-open drawer, click internal #features link -> initiates drawer close cleanly without defaultPrevented
    env.emitDocClick(trigGood);
    assert.equal(navDlg.open, true);
    const secEv = env.emitDocClick(sectionLink);
    assert.equal(secEv.defaultPrevented, false);
    assert.equal(navDlg.dataset.closing, '1');
    navDlg.__finishAnim();
    await new Promise((r) => setTimeout(r, 10));
    assert.equal(navDlg.open, false);
  }

  process.stdout.write('PASS test-public-blocks-js: 4/4 suites passed (Tabs, Slider, Lightbox+Share, Popup)\n');
}

await runTests();

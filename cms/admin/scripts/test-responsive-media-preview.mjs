/** Node-only responsive media preview & API URL helper regressions. */
import assert from 'node:assert/strict';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const { outputFiles: [{ text: previewCode }] } = await build({
  entryPoints: [fileURLToPath(new URL('../src/builder/previewMediaRewrite.ts', import.meta.url))],
  bundle: true,
  write: false,
  platform: 'node',
  format: 'esm',
  target: 'es2022',
});
const preview = await import(`data:text/javascript;base64,${Buffer.from(previewCode).toString('base64')}`);

const { outputFiles: [{ text: mediaApiCode }] } = await build({
  entryPoints: [fileURLToPath(new URL('../src/api/media.ts', import.meta.url))],
  bundle: true,
  write: false,
  platform: 'node',
  format: 'esm',
  target: 'es2022',
});
const mediaApi = await import(`data:text/javascript;base64,${Buffer.from(mediaApiCode).toString('base64')}`);

class MockEl {
  constructor(tagName, attrs = {}, parent = null) {
    this.tagName = tagName.toUpperCase();
    this.attrs = new Map(Object.entries(attrs));
    this.parentElement = parent;
    this.parentNode = parent;
    this.removed = false;
  }
  getAttribute(name) {
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }
  setAttribute(name, value) {
    this.attrs.set(name, String(value));
  }
  removeAttribute(name) {
    this.attrs.delete(name);
  }
  remove() {
    this.removed = true;
  }
}

class MockDoc {
  constructor(elements) {
    this.elements = elements;
  }
  querySelectorAll(selector) {
    return this.elements.filter(el => {
      if (el.removed) return false;
      const match = selector.match(/^([a-z]+)\[([a-z-]+)\]$/);
      if (!match) return false;
      const [, tag, attr] = match;
      return el.tagName === tag.toUpperCase() && el.attrs.has(attr);
    });
  }
}

const sampleId = 'a'.repeat(64);

test('PRIVATE_MEDIA_PATH and CSS_MEDIA_URL accept versioned variants and reject invalid paths', () => {
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/file`), true);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/file?v=2`), true);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/thumbnail?v=3`), true);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/variant/medium.webp?v=2`), true);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/variant/large.jpg`), true);

  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/file?v=0`), false);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/file?foo=bar`), false);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test(`/api/admin/media/${sampleId}/variant/medium.gif?v=1`), false);
  assert.equal(preview.PRIVATE_MEDIA_PATH.test('/api/admin/media/../secret/file'), false);

  const cssText = [
    `.a{background-image:url("/api/admin/media/${sampleId}/variant/large.webp?v=2")}`,
    `.b{background-image:url('/media/${sampleId}/file?v=4')}`,
    `.c{background-image:url(/media/${sampleId}/variant/bad.gif?v=1)}`,
  ].join('\n');
  preview.CSS_MEDIA_URL.lastIndex = 0;
  const matched = Array.from(cssText.matchAll(preview.CSS_MEDIA_URL), m => m[2]);
  assert.deepEqual(matched, [
    `/api/admin/media/${sampleId}/variant/large.webp?v=2`,
    `/media/${sampleId}/file?v=4`,
  ]);
});

test('rewritePreviewSrcset fails closed on private or mixed srcsets and rewrites public srcsets', () => {
  assert.deepEqual(
    preview.rewritePreviewSrcset(
      `/api/admin/media/${sampleId}/variant/medium.webp?v=2 768w, /api/admin/media/${sampleId}/file?v=2 1200w`,
      'https://cms.example',
    ),
    { kind: 'private' },
  );
  assert.deepEqual(
    preview.rewritePreviewSrcset(
      `/media/${sampleId}/variant/medium.webp?v=2 768w, /api/admin/media/${sampleId}/file?v=2 1200w`,
      'https://cms.example',
    ),
    { kind: 'private' },
  );
  assert.deepEqual(
    preview.rewritePreviewSrcset(
      `/media/${sampleId}/variant/medium.webp?v=2 768w, /media/${sampleId}/file?v=2 1200w`,
      'https://cms.example',
    ),
    {
      kind: 'rewritten',
      value: `https://cms.example/media/${sampleId}/variant/medium.webp?v=2 768w, https://cms.example/media/${sampleId}/file?v=2 1200w`,
    },
  );
});

test('applyPreviewMediaRewrites handles private/public picture srcset, video poster, and CSS urls', () => {
  const privId = 'b'.repeat(64);
  const pubId = 'c'.repeat(64);
  const privPic = new MockEl('picture');
  const privSource = new MockEl(
    'source',
    {
      type: 'image/webp',
      srcset: `/api/admin/media/${privId}/variant/medium.webp?v=3 768w, /api/admin/media/${privId}/file?v=3 1400w`,
    },
    privPic,
  );
  const privImg = new MockEl(
    'img',
    {
      src: `/api/admin/media/${privId}/file?v=3`,
      srcset: `/api/admin/media/${privId}/variant/medium.jpg?v=3 768w, /api/admin/media/${privId}/file?v=3 1400w`,
    },
    privPic,
  );

  const pubPic = new MockEl('picture');
  const pubSource = new MockEl(
    'source',
    {
      type: 'image/webp',
      srcset: `/media/${pubId}/variant/medium.webp?v=5 768w, /media/${pubId}/file?v=5 1600w`,
    },
    pubPic,
  );
  const pubImg = new MockEl(
    'img',
    {
      src: `/media/${pubId}/file?v=5`,
      srcset: `/media/${pubId}/variant/medium.jpg?v=5 768w, /media/${pubId}/file?v=5 1600w`,
    },
    pubPic,
  );

  const videoEl = new MockEl('video', {
    src: `/media/${pubId}/file?v=2`,
    poster: `/api/admin/media/${privId}/thumbnail?v=3`,
  });

  const scopedStyles = [
    {
      textContent:
        `.hero{background-image:url("/api/admin/media/${privId}/file?v=3")}`
        + `.card{background-image:url("/media/${pubId}/variant/large.webp?v=5")}`,
    },
  ];
  const doc = new MockDoc([privSource, privImg, pubSource, pubImg, videoEl]);

  const collected = preview.collectPrivateMediaPaths(doc, scopedStyles);
  assert.deepEqual(collected, [
    `/api/admin/media/${privId}/file?v=3`,
    `/api/admin/media/${privId}/thumbnail?v=3`,
  ]);

  const replacements = new Map([
    [`/api/admin/media/${privId}/file?v=3`, 'data:image/png;base64,AAA'],
    [`/api/admin/media/${privId}/thumbnail?v=3`, 'data:image/png;base64,BBB'],
  ]);
  preview.applyPreviewMediaRewrites(doc, scopedStyles, replacements, 'https://site.local');

  assert.equal(privSource.removed, true);
  assert.equal(privImg.getAttribute('src'), 'data:image/png;base64,AAA');
  assert.equal(privImg.getAttribute('srcset'), null);

  assert.equal(pubSource.removed, false);
  assert.equal(
    pubSource.getAttribute('srcset'),
    `https://site.local/media/${pubId}/variant/medium.webp?v=5 768w, https://site.local/media/${pubId}/file?v=5 1600w`,
  );
  assert.equal(pubImg.getAttribute('src'), `https://site.local/media/${pubId}/file?v=5`);
  assert.equal(
    pubImg.getAttribute('srcset'),
    `https://site.local/media/${pubId}/variant/medium.jpg?v=5 768w, https://site.local/media/${pubId}/file?v=5 1600w`,
  );
  assert.equal(videoEl.getAttribute('src'), `https://site.local/media/${pubId}/file?v=2`);
  assert.equal(videoEl.getAttribute('poster'), 'data:image/png;base64,BBB');
  assert.equal(
    scopedStyles[0].textContent,
    `.hero{background-image:url("data:image/png;base64,AAA")}`
      + `.card{background-image:url("https://site.local/media/${pubId}/variant/large.webp?v=5")}`,
  );
});

test('mediaThumbnailUrl, mediaOriginalUrl, and formatMediaIfMatch preserve version and avoid double-quoting', () => {
  assert.equal(mediaApi.mediaThumbnailUrl('abc'), '/api/admin/media/abc/thumbnail');
  assert.equal(mediaApi.mediaThumbnailUrl('abc', 4), '/api/admin/media/abc/thumbnail?v=4');
  assert.equal(mediaApi.mediaThumbnailUrl('abc', 0), '/api/admin/media/abc/thumbnail');
  assert.equal(mediaApi.mediaOriginalUrl('abc'), '/api/admin/media/abc/file');
  assert.equal(mediaApi.mediaOriginalUrl('abc', 7), '/api/admin/media/abc/file?v=7');

  assert.equal(mediaApi.formatMediaIfMatch(2, 5, '"gv2-v5"'), '"gv2-v5"');
  assert.equal(mediaApi.formatMediaIfMatch(2, 5, ' "gv2-v5" '), '"gv2-v5"');
  assert.equal(mediaApi.formatMediaIfMatch(2, 5, undefined), '"gv2-v5"');
  assert.equal(mediaApi.formatMediaIfMatch(2, 5, 'gv2-v5'), '"gv2-v5"');
});

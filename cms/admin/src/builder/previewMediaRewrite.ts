export const PRIVATE_MEDIA_PATH =
  /^\/api\/admin\/media\/([a-zA-Z0-9_-]{1,128})\/(file|thumbnail|variant\/(?:thumbnail|medium|large|full)\.(?:jpg|png|webp))(?:\?v=[1-9][0-9]*)?$/;

export const CSS_MEDIA_URL =
  /url\(\s*(["']?)((?:\/api\/admin\/media|\/media)\/[a-zA-Z0-9_-]{1,128}\/(?:file|thumbnail|variant\/(?:thumbnail|medium|large|full)\.(?:jpg|png|webp))(?:\?v=[1-9][0-9]*)?)\1\s*\)/g;

export interface PreviewStyleElementLike {
  textContent: string | null;
}

export interface PreviewElementLike {
  tagName?: string;
  parentElement?: PreviewElementLike | null;
  parentNode?: PreviewElementLike | null;
  getAttribute(name: string): string | null;
  setAttribute(name: string, value: string): void;
  removeAttribute(name: string): void;
  remove(): void;
}

export interface PreviewDocumentLike {
  querySelectorAll(selector: string): Iterable<PreviewElementLike>;
}

export function rewriteRootRelativeUrl(raw: string | null, origin: string): string | null {
  if (!raw || !origin) return null;
  if (raw.startsWith('/theme/') || raw.startsWith('/media/') || raw === '/assets/opendash-tailwind.css') {
    return `${origin}${raw}`;
  }
  return null;
}

export type SrcsetRewriteResult =
  | { kind: 'private' }
  | { kind: 'rewritten'; value: string }
  | { kind: 'unchanged' };

export function rewritePreviewSrcset(rawSrcset: string, resolvedOrigin: string): SrcsetRewriteResult {
  const candidates = rawSrcset
    .split(',')
    .map(part => part.trim())
    .filter(Boolean);
  if (candidates.length === 0) {
    return { kind: 'unchanged' };
  }

  let changed = false;
  const nextCandidates: string[] = [];
  for (const candidate of candidates) {
    const tokens = candidate.split(/\s+/);
    const url = tokens[0] ?? '';
    if (url.startsWith('/api/admin/media/') || PRIVATE_MEDIA_PATH.test(url)) {
      return { kind: 'private' };
    }
    const rewritten = rewriteRootRelativeUrl(url, resolvedOrigin);
    if (rewritten) {
      changed = true;
      nextCandidates.push([rewritten, ...tokens.slice(1)].join(' '));
    } else {
      nextCandidates.push(candidate);
    }
  }

  return changed ? { kind: 'rewritten', value: nextCandidates.join(', ') } : { kind: 'unchanged' };
}

export function collectPrivateMediaPaths(
  parsedDoc: PreviewDocumentLike,
  scopedStyles: ReadonlyArray<PreviewStyleElementLike>,
): string[] {
  const candidates: string[] = [];
  for (const el of parsedDoc.querySelectorAll('img[src]')) {
    const val = el.getAttribute('src') ?? '';
    if (PRIVATE_MEDIA_PATH.test(val)) candidates.push(val);
  }
  for (const el of parsedDoc.querySelectorAll('video[poster]')) {
    const val = el.getAttribute('poster') ?? '';
    if (PRIVATE_MEDIA_PATH.test(val)) candidates.push(val);
  }
  for (const style of scopedStyles) {
    CSS_MEDIA_URL.lastIndex = 0;
    for (const match of (style.textContent ?? '').matchAll(CSS_MEDIA_URL)) {
      const path = match[2] ?? '';
      if (PRIVATE_MEDIA_PATH.test(path)) candidates.push(path);
    }
  }
  return [...new Set(candidates)];
}

export function applyPreviewMediaRewrites(
  parsedDoc: PreviewDocumentLike,
  scopedStyles: ReadonlyArray<PreviewStyleElementLike>,
  replacements: ReadonlyMap<string, string>,
  resolvedOrigin: string,
): void {
  const inlinedImages = new Set<PreviewElementLike>();
  const inlinedPictures = new Set<PreviewElementLike>();

  for (const el of parsedDoc.querySelectorAll('img[src]')) {
    const replacement = replacements.get(el.getAttribute('src') ?? '');
    if (replacement) {
      el.setAttribute('src', replacement);
      inlinedImages.add(el);
      const parent = el.parentElement ?? el.parentNode;
      if (parent && parent.tagName?.toUpperCase() === 'PICTURE') {
        inlinedPictures.add(parent);
      }
    }
  }

  for (const el of parsedDoc.querySelectorAll('video[poster]')) {
    const replacement = replacements.get(el.getAttribute('poster') ?? '');
    if (replacement) {
      el.setAttribute('poster', replacement);
    }
  }

  for (const el of parsedDoc.querySelectorAll('img[srcset]')) {
    const res = rewritePreviewSrcset(el.getAttribute('srcset') ?? '', resolvedOrigin);
    if (inlinedImages.has(el) || res.kind === 'private') {
      el.removeAttribute('srcset');
    } else if (res.kind === 'rewritten') {
      el.setAttribute('srcset', res.value);
    }
  }

  for (const el of Array.from(parsedDoc.querySelectorAll('source[srcset]'))) {
    const parent = el.parentElement ?? el.parentNode;
    const res = rewritePreviewSrcset(el.getAttribute('srcset') ?? '', resolvedOrigin);
    if ((parent && inlinedPictures.has(parent)) || res.kind === 'private') {
      el.remove();
    } else if (res.kind === 'rewritten') {
      el.setAttribute('srcset', res.value);
    }
  }

  for (const style of scopedStyles) {
    CSS_MEDIA_URL.lastIndex = 0;
    style.textContent = (style.textContent ?? '').replace(CSS_MEDIA_URL, (original, _quote: string, path: string) => {
      if (PRIVATE_MEDIA_PATH.test(path)) {
        const replacement = replacements.get(path);
        return replacement ? `url("${replacement}")` : original;
      }
      const rewritten = rewriteRootRelativeUrl(path, resolvedOrigin);
      return rewritten ? `url("${rewritten}")` : original;
    });
  }

  if (resolvedOrigin) {
    const urlAttributes: Array<[string, string]> = [
      ['link[href]', 'href'],
      ['img[src]', 'src'],
      ['video[src]', 'src'],
      ['video[poster]', 'poster'],
      ['source[src]', 'src'],
    ];
    for (const [selector, attr] of urlAttributes) {
      for (const el of parsedDoc.querySelectorAll(selector)) {
        const rewritten = rewriteRootRelativeUrl(el.getAttribute(attr), resolvedOrigin);
        if (rewritten) el.setAttribute(attr, rewritten);
      }
    }
  }
}

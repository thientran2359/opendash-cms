import type { ParsedRoute } from './router.ts';

export type StudioPanelId =
  | 'root'
  | 'identity'
  | 'options'
  | 'theme-options'
  | 'menus'
  | 'reading'
  | 'templates';

export type StudioOptionsScope =
  | { kind: 'index' }
  | { kind: 'global'; section: string }
  | { kind: 'theme'; section?: string }
  | { kind: 'custom'; pageKey: string; groupId?: string };

const PAGE_KEY_RE = /^[a-z][a-z0-9_]{0,63}$/;
const TOKEN_RE = /^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/;

export function safeDecodeSegment(raw: string | undefined): string | null {
  if (!raw) return null;
  try {
    const decoded = decodeURIComponent(raw);
    if (!decoded || /[\u0000-\u001f\u007f]/.test(decoded)) return null;
    return decoded;
  } catch {
    return null;
  }
}

function invalidStudio(hash: string): ParsedRoute {
  return {
    view: 'studio',
    sub: 'invalid',
    studioPanel: 'root',
    rawHash: hash,
  };
}

export function parseStudioRoute(segs: string[], params: URLSearchParams, hash: string): ParsedRoute {
  // Mode 2: #/studio/entry/:typeKey/:id
  if (segs[1] === 'entry') {
    const rawTypeKey = safeDecodeSegment(segs[2]) ?? undefined;
    const entryId = segs[3] && /^[1-9]\d*$/.test(segs[3]) && Number.isSafeInteger(Number(segs[3]))
      ? Number(segs[3])
      : undefined;
    const valid =
      segs.length === 4 &&
      Boolean(rawTypeKey) &&
      rawTypeKey !== 'section' &&
      PAGE_KEY_RE.test(rawTypeKey ?? '') &&
      entryId !== undefined;
    return {
      view: 'studio-entry',
      sub: valid ? 'entry' : 'invalid',
      typeKey: rawTypeKey,
      entryId,
      rawHash: hash,
    };
  }

  // Root or legacy query-based Studio links (?panel=...&template=...)
  if (segs.length === 1) {
    const rawPanel = params.get('panel');
    const rawTemplate = params.get('template');
    if (rawPanel || rawTemplate) {
      if (rawTemplate && (!rawPanel || rawPanel === 'templates')) {
        if (!TOKEN_RE.test(rawTemplate)) return invalidStudio(hash);
        return {
          view: 'studio',
          sub: 'templates',
          studioPanel: 'templates',
          studioTemplateKey: rawTemplate,
          rawHash: hash,
          redirectHash: `#/studio/templates/${encodeURIComponent(rawTemplate)}`,
        };
      }
      if (rawPanel === 'root') {
        return { view: 'studio', sub: 'root', studioPanel: 'root', rawHash: hash, redirectHash: '#/studio' };
      }
      if (rawPanel === 'identity') {
        return { view: 'studio', sub: 'identity', studioPanel: 'identity', rawHash: hash, redirectHash: '#/studio/identity' };
      }
      if (rawPanel === 'theme-options' || rawPanel === 'options') {
        return {
          view: 'studio',
          sub: 'options',
          studioPanel: 'options',
          studioOptionsRoute: { kind: 'index' },
          rawHash: hash,
          redirectHash: '#/studio/options',
        };
      }
      if (rawPanel === 'menus') {
        return { view: 'studio', sub: 'menus', studioPanel: 'menus', rawHash: hash, redirectHash: '#/studio/menus' };
      }
      if (rawPanel === 'reading') {
        return { view: 'studio', sub: 'reading', studioPanel: 'reading', rawHash: hash, redirectHash: '#/studio/reading' };
      }
      if (rawPanel === 'templates') {
        return { view: 'studio', sub: 'templates', studioPanel: 'templates', rawHash: hash, redirectHash: '#/studio/templates' };
      }
      return invalidStudio(hash);
    }
    return { view: 'studio', sub: 'root', studioPanel: 'root', rawHash: '#/studio' };
  }

  const section = segs[1];
  if (section === 'identity' && segs.length === 2) {
    return { view: 'studio', sub: 'identity', studioPanel: 'identity', rawHash: '#/studio/identity' };
  }
  if (section === 'menus' && segs.length === 2) {
    return { view: 'studio', sub: 'menus', studioPanel: 'menus', rawHash: '#/studio/menus' };
  }
  if (section === 'reading' && segs.length === 2) {
    return { view: 'studio', sub: 'reading', studioPanel: 'reading', rawHash: '#/studio/reading' };
  }
  if (section === 'templates') {
    if (segs.length === 2) {
      return { view: 'studio', sub: 'templates', studioPanel: 'templates', rawHash: '#/studio/templates' };
    }
    if (segs.length === 3) {
      const targetKey = safeDecodeSegment(segs[2]);
      if (!targetKey || !TOKEN_RE.test(targetKey)) return invalidStudio(hash);
      return {
        view: 'studio',
        sub: 'templates',
        studioPanel: 'templates',
        studioTemplateKey: targetKey,
        rawHash: `#/studio/templates/${encodeURIComponent(targetKey)}`,
      };
    }
    return invalidStudio(hash);
  }
  if (section === 'options') {
    if (segs.length === 2) {
      return {
        view: 'studio',
        sub: 'options',
        studioPanel: 'options',
        studioOptionsRoute: { kind: 'index' },
        rawHash: '#/studio/options',
      };
    }
    const scopeKind = segs[2];
    if (scopeKind === 'global' && segs.length === 4) {
      const key = safeDecodeSegment(segs[3]);
      if (!key || !TOKEN_RE.test(key)) return invalidStudio(hash);
      return {
        view: 'studio',
        sub: 'options',
        studioPanel: 'options',
        studioOptionsRoute: { kind: 'global', section: key },
        rawHash: `#/studio/options/global/${encodeURIComponent(key)}`,
      };
    }
    if (scopeKind === 'theme' && (segs.length === 3 || segs.length === 4)) {
      const key = segs.length === 4 ? safeDecodeSegment(segs[3]) : undefined;
      if (segs.length === 4 && (!key || !TOKEN_RE.test(key))) return invalidStudio(hash);
      return {
        view: 'studio',
        sub: 'options',
        studioPanel: 'options',
        studioOptionsRoute: key ? { kind: 'theme', section: key } : { kind: 'theme' },
        rawHash: key ? `#/studio/options/theme/${encodeURIComponent(key)}` : '#/studio/options/theme',
      };
    }
    if (scopeKind === 'custom' && (segs.length === 4 || segs.length === 5)) {
      const pageKey = safeDecodeSegment(segs[3]);
      const groupId = segs.length === 5 ? safeDecodeSegment(segs[4]) : undefined;
      if (!pageKey || !PAGE_KEY_RE.test(pageKey)) return invalidStudio(hash);
      if (segs.length === 5 && (!groupId || !TOKEN_RE.test(groupId))) return invalidStudio(hash);
      return {
        view: 'studio',
        sub: 'options',
        studioPanel: 'options',
        studioOptionsRoute: groupId
          ? { kind: 'custom', pageKey, groupId }
          : { kind: 'custom', pageKey },
        rawHash: groupId
          ? `#/studio/options/custom/${encodeURIComponent(pageKey)}/${encodeURIComponent(groupId)}`
          : `#/studio/options/custom/${encodeURIComponent(pageKey)}`,
      };
    }
    return invalidStudio(hash);
  }

  return invalidStudio(hash);
}

export function parseLegacyBuilderRoute(
  params: URLSearchParams,
  queryPart: string | undefined,
  hash: string,
): ParsedRoute {
  const entryParam = params.get('entry');
  const entryId = entryParam && /^[1-9]\d*$/.test(entryParam) && Number.isSafeInteger(Number(entryParam))
    ? Number(entryParam) : undefined;
  const templateRole = params.get('template') ?? undefined;
  const typeKey = params.get('type') ?? undefined;
  const validType = Boolean(typeKey && PAGE_KEY_RE.test(typeKey));

  if (templateRole === 'entry' && typeKey === 'pattern' && entryId !== undefined) {
    return {
      view: 'studio-entry', sub: 'entry', typeKey: 'pattern', entryId,
      rawHash: hash, redirectHash: `#/studio/entry/pattern/${entryId}`,
    };
  }
  if (templateRole === 'page' && typeKey === 'page' && entryId !== undefined) {
    return {
      view: 'studio-entry', sub: 'entry', typeKey: 'page', entryId,
      rawHash: hash, redirectHash: `#/studio/entry/page/${entryId}`,
    };
  }
  if (templateRole === 'entry' && validType && typeKey !== 'section' && entryId !== undefined) {
    return {
      view: 'studio-entry', sub: 'entry', typeKey, entryId,
      rawHash: hash, redirectHash: `#/studio/entry/${encodeURIComponent(typeKey!)}/${entryId}`,
    };
  }
  if (!entryParam && !templateRole && !typeKey && !queryPart) {
    return { view: 'studio', sub: 'root', studioPanel: 'root', rawHash: hash, redirectHash: '#/studio' };
  }
  if (templateRole === 'single' && validType && entryId !== undefined) {
    const studioTemplateKey = `single:type:${typeKey}:id:${entryId}`;
    return {
      view: 'studio', sub: 'templates', studioPanel: 'templates', studioTemplateKey,
      rawHash: hash, redirectHash: `#/studio/templates/${encodeURIComponent(studioTemplateKey)}`,
    };
  }
  if (!entryParam && (!typeKey || validType) && (
    templateRole === 'header' || templateRole === 'footer'
    || templateRole === 'taxonomy_archive' || templateRole === 'archive' || templateRole === 'single'
  )) {
    const mappedKey =
      templateRole === 'header'
        ? 'part:header'
        : templateRole === 'footer'
          ? 'part:footer'
          : templateRole === 'taxonomy_archive'
            ? (typeKey ? `taxonomy:key:${typeKey}` : 'taxonomy')
            : typeKey
              ? `${templateRole}:type:${typeKey}`
              : templateRole;
    return {
      view: 'studio', sub: 'templates', studioPanel: 'templates', studioTemplateKey: mappedKey,
      rawHash: hash, redirectHash: `#/studio/templates/${encodeURIComponent(mappedKey)}`,
    };
  }
  return {
    view: 'studio', sub: 'invalid', studioPanel: 'root',
    templateRole, typeKey, entryId, rawHash: hash,
  };
}

import type { Role } from '../types.ts';

export type AdminView =
  | 'dashboard'
  | 'posts'
  | 'pages'
  | 'sections'
  | 'cpt'
  | 'media'
  | 'taxonomy'
  | 'themes'
  | 'builder'
  | 'plugins'
  | 'users'
  | 'profile'
  | 'tools'
  | 'settings'
  | 'content';

export interface ParsedRoute {
  view: AdminView;
  sub: string;
  typeKey?: string;
  entryId?: number;
  taxonomyKind?: string;
  taxonomyMode?: 'definitions' | 'list' | 'new' | 'edit' | 'invalid';
  templateRole?: string;
  rawHash: string;
  isLegacyContentRedirect?: boolean;
}

export function parseAdminRoute(hash: string, pathname: string = '/admin/'): ParsedRoute {
  if (!hash || hash === '#' || hash === '#/') {
    if (pathname.replace(/\/$/, '') === '/builder') {
      return { view: 'builder', sub: 'builder', rawHash: '#/builder' };
    }
    return { view: 'dashboard', sub: 'dashboard', rawHash: '#/dashboard' };
  }

  const cleaned = hash.replace(/^#\/?/, '');
  const [pathPart, queryPart] = cleaned.split('?');
  // Preserve table filters in deep links (including direct reload and
  // edit→Back). The router still parses only path segments for identity.
  const tableSuffix = queryPart ? `?${queryPart}` : '';
  const segs = pathPart.split('/').filter(Boolean);
  const params = new URLSearchParams(queryPart ?? '');
  const first = segs[0] ?? 'dashboard';

  if (first === 'content') {
    return {
      view: 'settings',
      sub: 'content-types',
      rawHash: '#/content',
      isLegacyContentRedirect: true,
    };
  }
  if (first === 'dashboard') {
    return { view: 'dashboard', sub: 'dashboard', rawHash: '#/dashboard' };
  }
  if (first === 'posts') {
    if (segs[1] === 'categories' || segs[1] === 'tags') {
      const sub = segs[1];
      const taxonomyKind = sub === 'categories' ? 'category' : 'tag';
      if (segs[2] === 'new' && segs.length === 3) {
        return { view: 'taxonomy', sub, taxonomyKind, taxonomyMode: 'new', rawHash: `#/posts/${sub}/new` };
      }
      if (segs.length === 4 && segs[3] === 'edit' && /^[1-9]\d*$/.test(segs[2]) && Number.isSafeInteger(Number(segs[2]))) {
        return { view: 'taxonomy', sub, taxonomyKind, taxonomyMode: 'edit', entryId: Number(segs[2]), rawHash: `#/posts/${sub}/${segs[2]}/edit` };
      }
      if (segs.length === 2) return { view: 'taxonomy', sub, taxonomyKind, taxonomyMode: 'list', rawHash: `#/posts/${sub}` };
      return { view: 'taxonomy', sub, taxonomyKind, taxonomyMode: 'edit', entryId: 0, rawHash: hash };
    }
    if (segs[1] === 'new') return { view: 'posts', sub: 'new', rawHash: '#/posts/new' };
    if (segs[1] && segs[2] === 'edit' && /^\d+$/.test(segs[1])) {
      return { view: 'posts', sub: 'edit', entryId: Number(segs[1]), rawHash: `#/posts/${segs[1]}/edit${tableSuffix}` };
    }
    return { view: 'posts', sub: 'list', rawHash: `#/posts${tableSuffix}` };
  }
  if (first === 'pages') {
    if (segs[1] === 'new') return { view: 'pages', sub: 'new', rawHash: '#/pages/new' };
    if (segs[1] && segs[2] === 'edit' && /^\d+$/.test(segs[1])) {
      return { view: 'pages', sub: 'edit', entryId: Number(segs[1]), rawHash: `#/pages/${segs[1]}/edit${tableSuffix}` };
    }
    return { view: 'pages', sub: 'list', rawHash: `#/pages${tableSuffix}` };
  }
  if (first === 'cpt' && segs[1]) {
    const typeKey = decodeURIComponent(segs[1]);
    if (segs[2] === 'new') {
      return { view: 'cpt', sub: 'new', typeKey, rawHash: `#/cpt/${typeKey}/new` };
    }
    if (segs[2] && segs[3] === 'edit' && /^\d+$/.test(segs[2])) {
      return {
        view: 'cpt',
        sub: 'edit',
        typeKey,
        entryId: Number(segs[2]),
        rawHash: `#/cpt/${typeKey}/${segs[2]}/edit${tableSuffix}`,
      };
    }
    return { view: 'cpt', sub: 'list', typeKey, rawHash: `#/cpt/${typeKey}${tableSuffix}` };
  }
  if (first === 'sections') {
    if (segs[1] === 'new' && segs.length === 2) return { view: 'sections', sub: 'new', rawHash: '#/sections/new' };
    if (segs.length === 3 && segs[2] === 'edit' && /^[1-9]\d*$/.test(segs[1]) && Number.isSafeInteger(Number(segs[1]))) {
      return { view: 'sections', sub: 'edit', entryId: Number(segs[1]), rawHash: `#/sections/${segs[1]}/edit` };
    }
    return { view: 'sections', sub: segs.length === 1 ? 'list' : 'invalid', rawHash: hash };
  }
  if (first === 'media') {
    return { view: 'media', sub: segs[1] === 'upload' ? 'upload' : 'library', rawHash: `#/media${segs[1] === 'upload' ? '/upload' : ''}` };
  }
  if (first === 'taxonomy') {
    if (segs.length === 2 && segs[1] === 'definitions') {
      return { view: 'taxonomy', sub: 'definitions', taxonomyMode: 'definitions', rawHash: '#/taxonomy/definitions' };
    }
    // The old /taxonomy/categories|tags aliases continue to work.
    if (segs.length === 2 && (segs[1] === 'categories' || segs[1] === 'tags')) {
      const sub = segs[1];
      return { view: 'taxonomy', sub, taxonomyKind: sub === 'tags' ? 'tag' : 'category', taxonomyMode: 'list', rawHash: hash };
    }
    const key = segs[1];
    const validKey = Boolean(key && /^[a-z][a-z0-9_]{0,63}$/.test(key) && key !== 'section');
    if (validKey && segs[2] === 'terms') {
      const root = `#/taxonomy/${key}/terms`;
      if (segs.length === 3) return { view: 'taxonomy', sub: 'terms', taxonomyKind: key, taxonomyMode: 'list', rawHash: root };
      if (segs.length === 4 && segs[3] === 'new') {
        return { view: 'taxonomy', sub: 'terms', taxonomyKind: key, taxonomyMode: 'new', rawHash: `${root}/new` };
      }
      if (segs.length === 5 && segs[4] === 'edit' && /^[1-9]\d*$/.test(segs[3]) && Number.isSafeInteger(Number(segs[3]))) {
        return { view: 'taxonomy', sub: 'terms', taxonomyKind: key, taxonomyMode: 'edit', entryId: Number(segs[3]), rawHash: `${root}/${segs[3]}/edit` };
      }
    }
    return { view: 'taxonomy', sub: 'invalid', taxonomyMode: 'invalid', rawHash: hash };
  }
  if (first === 'appearance' || first === 'themes') {
    const sub = first === 'themes' ? 'themes' : (segs[1] ?? 'themes');
    if (sub === 'content-types') {
      const typeKey = segs[2] ? decodeURIComponent(segs[2]) : undefined;
      return {
        view: 'settings', sub: 'content-types', typeKey,
        rawHash: typeKey ? `#/appearance/content-types/${typeKey}` : '#/appearance/content-types',
      };
    }
    if (sub === 'library') return { view: 'themes', sub: 'library', rawHash: '#/appearance/library' };
    if (sub === 'options') return { view: 'themes', sub: 'options', rawHash: '#/appearance/options' };
    if (sub === 'menus') return { view: 'themes', sub: 'menus', rawHash: '#/appearance/menus' };
    return { view: 'themes', sub: 'themes', rawHash: '#/appearance/themes' };
  }
  if (first === 'builder') {
    const entryParam = params.get('entry');
    const entryId = entryParam && /^[1-9]\d*$/.test(entryParam) && Number.isSafeInteger(Number(entryParam))
      ? Number(entryParam) : undefined;
    const templateRole = params.get('template') ?? undefined;
    const typeKey = params.get('type') ?? undefined;
    const scopedPage = templateRole === 'page' && typeKey === 'page' && entryId !== undefined;
    const scopedSection = templateRole === 'section' && typeKey === 'section' && entryId !== undefined;
    const noScope = !entryParam && !templateRole && !typeKey && !queryPart;
    const validGlobal = !entryParam && templateRole !== 'page' &&
      (templateRole === 'homepage' || templateRole === 'header' || templateRole === 'footer'
        || templateRole === 'taxonomy_archive' || templateRole === 'archive'
        || templateRole === 'single') && (!typeKey || /^[a-z][a-z0-9_]{0,63}$/.test(typeKey));
    const validAdminScopedSingle = templateRole === 'single' && !!typeKey && entryId !== undefined;
    return {
      view: 'builder',
      sub: scopedPage || scopedSection || noScope || validGlobal || validAdminScopedSingle ? 'builder' : 'invalid',
      templateRole,
      typeKey,
      entryId,
      rawHash: hash,
    };
  }
  if (first === 'contact') {
    // Preserve legacy contact deep links while routing to the same guarded
    // feature screen as #/settings/contact.
    return { view: 'settings', sub: 'contact', rawHash: `#/contact/${segs[1] ?? 'submissions'}` };
  }
  if (first === 'plugins') {
    return { view: 'plugins', sub: segs[1] === 'install' ? 'install' : 'list', rawHash: `#/plugins${segs[1] === 'install' ? '/install' : ''}` };
  }
  if (first === 'users') {
    if (segs[1] === 'profile') return { view: 'profile', sub: 'profile', rawHash: '#/users/profile' };
    return { view: 'users', sub: segs[1] === 'new' ? 'new' : 'list', rawHash: `#/users${segs[1] === 'new' ? '/new' : ''}` };
  }
  if (first === 'tools') {
    const sub = segs[1] === 'restore' ? 'backup' : (segs[1] ?? 'backup');
    return { view: 'tools', sub, rawHash: `#/tools/${segs[1] ?? 'backup'}` };
  }
  if (first === 'settings') {
    const sub = segs[1] ?? 'general';
    const typeKey = sub === 'content-types' && segs[2] ? decodeURIComponent(segs[2]) : undefined;
    return {
      view: 'settings',
      sub,
      typeKey,
      rawHash: typeKey ? `#/settings/content-types/${typeKey}` : `#/settings/${sub}`,
    };
  }

  return { view: 'dashboard', sub: 'dashboard', rawHash: '#/dashboard' };
}

export function isRouteAllowed(route: ParsedRoute, role: Role): boolean {
  if (route.view === 'sections' && route.sub === 'invalid') return false;
  if (route.view === 'taxonomy' && route.taxonomyMode === 'invalid') return false;
  if (route.view === 'builder' && route.sub !== 'builder') return false;
  if (role === 'admin') return true;
  // Editors may update term extension values on an existing term. Creating
  // terms and editing definition/eligibility schemas remain Admin-only.
  if (route.view === 'taxonomy' && !['list', 'edit'].includes(route.taxonomyMode ?? '')) return false;
  if (route.view === 'themes' || route.view === 'plugins' || route.view === 'users' || route.view === 'tools' || route.view === 'settings') {
    return false;
  }
  if (route.view === 'builder') {
    return ((route.templateRole === 'page' && route.typeKey === 'page')
      || (route.templateRole === 'section' && route.typeKey === 'section'))
      && Number.isSafeInteger(route.entryId) && (route.entryId ?? 0) > 0;
  }
  return true;
}

export function parseAdminView(hash: string, pathname: string = '/admin/'): AdminView {
  return parseAdminRoute(hash, pathname).view;
}

export function isViewAllowed(view: AdminView, role: Role): boolean {
  if (role === 'admin') return true;
  return !['themes', 'plugins', 'users', 'tools', 'settings'].includes(view);
}

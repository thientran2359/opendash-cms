import React from 'react';
import { useI18n } from '../i18n/index.tsx';
import type { OwnedContentType } from '../features/content/types.ts';
import {
  isRouteAllowed,
  isViewAllowed,
  parseAdminRoute,
  parseAdminView,
  type AdminView,
  type ParsedRoute,
} from './router.ts';

export {
  isRouteAllowed,
  isViewAllowed,
  parseAdminRoute,
  parseAdminView,
  type AdminView,
  type ParsedRoute,
};

export interface SubMenuItem {
  label: string;
  hash: string;
  adminOnly?: boolean;
  disabled?: boolean;
}

export type NavigationGroup = 'content' | 'appearance' | 'system';

export interface ParentMenuItem {
  id: string;
  label: string;
  icon: string;
  hash: string;
  group?: NavigationGroup;
  adminOnly?: boolean;
  disabled?: boolean;
  children?: SubMenuItem[];
}

export function buildMenuTree(
  t: ReturnType<typeof useI18n>['t'],
  cptTypes: OwnedContentType[],
  isAdmin: boolean,
  cptTaxonomies: Record<string, Array<{ key: string; label: string }>> = {},
): ParentMenuItem[] {
  const customTypes = cptTypes.filter(
    ct => ct.descriptor.key !== 'post' && ct.descriptor.key !== 'page' && ct.descriptor.key !== 'section' && ct.descriptor.key !== 'pattern',
  );
  return [
    { id: 'dashboard', label: t('shell.dashboard'), icon: 'dashboard', hash: '#/dashboard' },
    {
      id: 'posts',
      label: t('shell.posts'),
      icon: 'posts',
      hash: '#/posts',
      group: 'content',
      children: [
        { label: t('shell.all_posts'), hash: '#/posts' },
        { label: t('shell.new_post'), hash: '#/posts/new' },
        { label: t('shell.categories'), hash: '#/posts/categories' },
        { label: t('shell.tags'), hash: '#/posts/tags' },
      ],
    },
    {
      id: 'pages',
      label: t('shell.pages'),
      icon: 'pages',
      hash: '#/pages',
      group: 'content',
      children: [
        { label: t('shell.all_pages'), hash: '#/pages' },
        { label: t('shell.new_page'), hash: '#/pages/new' },
      ],
    },
    ...customTypes.map(ct => ({
      id: `cpt:${ct.descriptor.key}`,
      label: ct.descriptor.label,
      icon: 'content',
      hash: `#/cpt/${ct.descriptor.key}`,
      group: 'content' as const,
      children: [
        { label: t('shell.all_custom', { label: ct.descriptor.label }), hash: `#/cpt/${ct.descriptor.key}` },
        { label: t('shell.new'), hash: `#/cpt/${ct.descriptor.key}/new` },
        ...(cptTaxonomies[ct.descriptor.key] ?? []).map(tax => ({
          label: tax.label,
          hash: `#/cpt/${encodeURIComponent(ct.descriptor.key)}/taxonomy/${encodeURIComponent(tax.key)}`,
        })),
      ],
    })),
    {
      id: 'media',
      label: t('shell.media'),
      icon: 'media',
      hash: '#/media',
      group: 'content',
    },
    {
      id: 'contact',
      label: t('shell.contact'),
      icon: 'mail',
      hash: '#/settings/contact',
      group: 'content',
      adminOnly: true,
    },
    {
      id: 'studio',
      label: t('shell.studio'),
      icon: 'builder',
      hash: '#/studio',
      group: 'appearance',
      adminOnly: true,
    },
    {
      id: 'themes',
      label: t('shell.themes'),
      icon: 'themes',
      hash: '#/appearance/themes',
      group: 'appearance',
      adminOnly: true,
      children: [
        { label: t('shell.installed_themes'), hash: '#/appearance/themes' },
        { label: t('shell.install_theme'), hash: '#/appearance/library' },
      ],
    },
    {
      id: 'patterns',
      label: t('shell.patterns'),
      icon: 'content',
      hash: '#/patterns',
      group: 'appearance',
      adminOnly: true,
      children: [
        { label: t('shell.all_patterns'), hash: '#/patterns' },
        { label: t('shell.new_pattern'), hash: '#/patterns/new' },
      ],
    },
    {
      id: 'schema',
      label: t('shell.schema'),
      icon: 'schema',
      hash: '#/appearance/content-types',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.content_types'), hash: '#/appearance/content-types' },
        { label: t('shell.taxonomy_definitions'), hash: '#/taxonomy/definitions' },
      ],
    },
    {
      id: 'plugins',
      label: t('shell.plugins'),
      icon: 'plugins',
      hash: '#/plugins',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.installed_plugins'), hash: '#/plugins' },
        { label: t('shell.install_plugin'), hash: '#/plugins/install' },
      ],
    },
    {
      id: 'users',
      label: t('shell.users'),
      icon: 'users',
      hash: isAdmin ? '#/users' : '#/users/profile',
      group: 'system',
      children: [
        { label: t('shell.all_users'), hash: '#/users', adminOnly: true },
        { label: t('shell.new_user'), hash: '#/users/new', adminOnly: true },
        { label: t('shell.profile'), hash: '#/users/profile' },
      ],
    },
    {
      id: 'tools',
      label: t('shell.tools'),
      icon: 'tools',
      hash: '#/tools/backup',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.backup'), hash: '#/tools/backup' },
        { label: t('shell.media_reconcile'), hash: '#/tools/media-reconcile' },
        { label: t('shell.health'), hash: '#/tools/health' },
      ],
    },
    {
      id: 'settings',
      label: t('shell.settings'),
      icon: 'settings',
      hash: '#/settings/general',
      group: 'system',
      adminOnly: true,
      children: [
        { label: t('shell.general'), hash: '#/settings/general' },
        { label: t('shell.multilingual'), hash: '#/settings/multilingual', adminOnly: true },
        { label: t('shell.reading'), hash: '#/settings/reading' },
        { label: t('shell.permalinks'), hash: '#/settings/permalinks' },
        { label: t('shell.seo'), hash: '#/settings/seo' },
        { label: t('shell.smtp'), hash: '#/settings/smtp' },
      ],
    },
  ];
}

export function NavIcon({ name }: { name: string }) {
  const paths: Record<string, React.ReactNode> = {
    dashboard: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>,
    posts: <><path d="M8 3h9l4 4v14H8z" /><path d="M17 3v5h4M11 12h7M11 16h7M3 7v14" /></>,
    pages: <><path d="M5 3h10l4 4v14H5zM15 3v5h4M9 12h6M9 16h6" /></>,
    content: <><path d="M4 5h16v14H4zM8 9h3v3H8zM14 9h3M14 13h3M8 16h9" /></>,
    media: <><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="8" cy="9" r="1.5" /><path d="m4 17 5-5 4 3 3-4 5 6" /></>,
    mail: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m3 7 9 6 9-6" /></>,
    builder: <><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16M9 11h12" /></>,
    themes: <><path d="M12 3 3 8l9 5 9-5-9-5ZM3 12l9 5 9-5M3 16l9 5 9-5" /></>,
    palette: <><circle cx="12" cy="12" r="9" /><circle cx="9" cy="10" r="1" /><circle cx="14" cy="9" r="1" /><circle cx="16" cy="13" r="1" /><circle cx="10" cy="15" r="1" /></>,
    schema: <><ellipse cx="12" cy="5" rx="8" ry="3" /><path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6" /></>,
    plugins: <><path d="M9 3h6v5h5v5h-5v8H9v-8H4V8h5z" /></>,
    users: <><circle cx="9" cy="8" r="3" /><path d="M3 21v-2a6 6 0 0 1 12 0v2M17 5a3 3 0 0 1 0 6M18 15a5 5 0 0 1 3 5v1" /></>,
    tools: <><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" /></>,
    settings: <><circle cx="12" cy="12" r="3" /><path d="m10 2-1 3-3 1-3-1-2 4 3 2v2l-3 2 2 4 3-1 3 1 1 3h4l1-3 3-1 3 1 2-4-3-2v-2l3-2-2-4-3 1-3-1-1-3z" /></>,
  };
  return <svg aria-hidden="true" width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" strokeLinecap="round">{paths[name] ?? paths.content}</svg>;
}


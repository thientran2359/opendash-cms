/** Node-only unit regressions for Admin shell routing, RBAC guards, and sidebar IA. */
import assert from 'node:assert/strict';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const reactStubPlugin = {
  name: 'react-stub',
  setup(pluginBuild) {
    pluginBuild.onResolve({ filter: /^react(\/jsx-runtime)?$/ }, args => ({
      path: args.path,
      namespace: 'react-stub',
    }));
    pluginBuild.onLoad({ filter: /.*/, namespace: 'react-stub' }, () => ({
      contents: `
        const noop = () => {};
        export const Fragment = Symbol('Fragment');
        export function createContext(v) { return { Provider: noop, _defaultValue: v }; }
        export function useCallback(fn) { return fn; }
        export function useContext(ctx) { return ctx?._defaultValue; }
        export function useEffect() {}
        export function useRef(v) { return { current: v }; }
        export function useState(v) { return [typeof v === 'function' ? v() : v, noop]; }
        export function jsx() { return null; }
        export function jsxs() { return null; }
        export default { createContext, useCallback, useContext, useEffect, useRef, useState, Fragment };
      `,
      loader: 'js',
    }));
  },
};

const [{ outputFiles: [routerOut] }, { outputFiles: [menuOut] }] = await Promise.all([
  build({
    entryPoints: [fileURLToPath(new URL('../src/shell/router.ts', import.meta.url))],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'esm',
    target: 'es2022',
  }),
  build({
    entryPoints: [fileURLToPath(new URL('../src/shell/AdminShellMenu.tsx', import.meta.url))],
    bundle: true,
    write: false,
    platform: 'node',
    format: 'esm',
    target: 'es2022',
    plugins: [reactStubPlugin],
  }),
]);

const {
  parseAdminRoute,
  parseAdminView,
  isRouteAllowed,
  isViewAllowed,
} = await import(`data:text/javascript;base64,${Buffer.from(routerOut.text).toString('base64')}`);

const { buildMenuTree } = await import(
  `data:text/javascript;base64,${Buffer.from(menuOut.text).toString('base64')}`
);

test('contact routes normalize canonical #/contact and keep legacy aliases', () => {
  assert.deepEqual(parseAdminRoute('#/contact'), {
    view: 'contact',
    sub: 'forms',
    rawHash: '#/contact',
  });
  assert.deepEqual(parseAdminRoute('#/contact/inbox'), {
    view: 'contact',
    sub: 'inbox',
    rawHash: '#/contact/inbox',
  });
  assert.deepEqual(parseAdminRoute('#/contact/submissions'), {
    view: 'contact',
    sub: 'inbox',
    rawHash: '#/contact/submissions',
  });
  assert.deepEqual(parseAdminRoute('#/contact/unknown'), {
    view: 'contact',
    sub: 'forms',
    rawHash: '#/contact',
  });
  assert.deepEqual(parseAdminRoute('#/settings/contact'), {
    view: 'contact',
    sub: 'forms',
    rawHash: '#/settings/contact',
    redirectHash: '#/contact',
  });
  assert.deepEqual(parseAdminRoute('#/settings/contact/submissions'), {
    view: 'contact',
    sub: 'inbox',
    rawHash: '#/settings/contact/submissions',
    redirectHash: '#/contact/inbox',
  });
});

test('themes install route separates from Template Library (#/appearance/library)', () => {
  assert.deepEqual(parseAdminRoute('#/appearance/themes'), {
    view: 'themes',
    sub: 'themes',
    rawHash: '#/appearance/themes',
  });
  assert.deepEqual(parseAdminRoute('#/appearance/themes/install'), {
    view: 'themes',
    sub: 'install',
    rawHash: '#/appearance/themes/install',
  });
  assert.deepEqual(parseAdminRoute('#/themes/install'), {
    view: 'themes',
    sub: 'install',
    rawHash: '#/themes/install',
    redirectHash: '#/appearance/themes/install',
  });
  assert.deepEqual(parseAdminRoute('#/appearance/library'), {
    view: 'themes',
    sub: 'library',
    rawHash: '#/appearance/library',
  });
  assert.deepEqual(parseAdminRoute('#/appearance/menus'), {
    view: 'studio',
    sub: 'menus',
    studioPanel: 'menus',
    rawHash: '#/appearance/menus',
    redirectHash: '#/studio/menus',
  });
  assert.deepEqual(parseAdminRoute('#/appearance/options'), {
    view: 'studio',
    sub: 'options',
    studioPanel: 'options',
    studioOptionsRoute: { kind: 'index' },
    rawHash: '#/appearance/options',
    redirectHash: '#/studio/options',
  });
});

test('Studio path sub-routes and legacy single-pass redirects parse cleanly', () => {
  assert.deepEqual(parseAdminRoute('#/studio/identity'), {
    view: 'studio',
    sub: 'identity',
    studioPanel: 'identity',
    rawHash: '#/studio/identity',
  });
  assert.deepEqual(parseAdminRoute('#/studio/options'), {
    view: 'studio',
    sub: 'options',
    studioPanel: 'options',
    studioOptionsRoute: { kind: 'index' },
    rawHash: '#/studio/options',
  });
  assert.deepEqual(parseAdminRoute('#/studio/options/global/acc_social_links'), {
    view: 'studio',
    sub: 'options',
    studioPanel: 'options',
    studioOptionsRoute: { kind: 'global', section: 'acc_social_links' },
    rawHash: '#/studio/options/global/acc_social_links',
  });
  assert.deepEqual(parseAdminRoute('#/studio/options/theme'), {
    view: 'studio',
    sub: 'options',
    studioPanel: 'options',
    studioOptionsRoute: { kind: 'theme' },
    rawHash: '#/studio/options/theme',
  });
  assert.deepEqual(parseAdminRoute('#/studio/options/custom/optheme_options'), {
    view: 'studio',
    sub: 'options',
    studioPanel: 'options',
    studioOptionsRoute: { kind: 'custom', pageKey: 'optheme_options' },
    rawHash: '#/studio/options/custom/optheme_options',
  });
  assert.deepEqual(parseAdminRoute('#/studio/options/custom/optheme_options/site%3Abrand_grp'), {
    view: 'studio',
    sub: 'options',
    studioPanel: 'options',
    studioOptionsRoute: { kind: 'custom', pageKey: 'optheme_options', groupId: 'site:brand_grp' },
    rawHash: '#/studio/options/custom/optheme_options/site%3Abrand_grp',
  });
  assert.deepEqual(parseAdminRoute('#/studio/templates/part%3Aheader'), {
    view: 'studio',
    sub: 'templates',
    studioPanel: 'templates',
    studioTemplateKey: 'part:header',
    rawHash: '#/studio/templates/part%3Aheader',
  });
  // Legacy query & builder single-pass redirects
  assert.equal(parseAdminRoute('#/studio?panel=theme-options').redirectHash, '#/studio/options');
  assert.equal(parseAdminRoute('#/studio?panel=menus').redirectHash, '#/studio/menus');
  assert.equal(parseAdminRoute('#/studio?panel=templates&template=part:header').redirectHash, '#/studio/templates/part%3Aheader');
  assert.equal(parseAdminRoute('#/builder?template=header').redirectHash, '#/studio/templates/part%3Aheader');
  // Malformed URI encoding never throws and marks sub as invalid
  assert.equal(parseAdminRoute('#/studio/options/custom/%E0%A4%A').sub, 'invalid');
});

test('content-structure canonical routes and legacy aliases parse cleanly', () => {
  assert.deepEqual(parseAdminRoute('#/content-structure/content-types'), {
    view: 'content-structure',
    sub: 'content-types',
    typeKey: undefined,
    rawHash: '#/content-structure/content-types',
  });
  assert.deepEqual(parseAdminRoute('#/content-structure/content-types/project'), {
    view: 'content-structure',
    sub: 'content-types',
    typeKey: 'project',
    rawHash: '#/content-structure/content-types/project',
  });
  assert.deepEqual(parseAdminRoute('#/content-structure/taxonomies'), {
    view: 'content-structure',
    sub: 'taxonomies',
    rawHash: '#/content-structure/taxonomies',
  });
  assert.deepEqual(parseAdminRoute('#/content-structure/field-groups'), {
    view: 'content-structure',
    sub: 'field-groups',
    typeKey: undefined,
    rawHash: '#/content-structure/field-groups',
  });
  assert.deepEqual(parseAdminRoute('#/content-structure/options-pages'), {
    view: 'content-structure',
    sub: 'options-pages',
    typeKey: undefined,
    rawHash: '#/content-structure/options-pages',
  });
  assert.deepEqual(parseAdminRoute('#/options/brand_config'), {
    view: 'options-page',
    sub: 'view',
    typeKey: 'brand_config',
    rawHash: '#/options/brand_config',
  });
  assert.deepEqual(parseAdminRoute('#/content-structure'), {
    view: 'content-structure',
    sub: 'content-types',
    typeKey: undefined,
    rawHash: '#/content-structure/content-types',
  });
  // Legacy deep links continue parsing without forced URL rewriting.
  assert.equal(parseAdminRoute('#/appearance/content-types/page').rawHash, '#/appearance/content-types/page');
  assert.equal(parseAdminRoute('#/settings/content-types/legacy_cpt').rawHash, '#/settings/content-types/legacy_cpt');
  assert.equal(parseAdminRoute('#/taxonomy/definitions').rawHash, '#/taxonomy/definitions');
  assert.equal(parseAdminRoute('#/content').isLegacyContentRedirect, true);
});

test('RBAC blocks editor from contact, themes install, library, content-structure, users, tools, settings, and studio', () => {
  const adminRoutes = [
    '#/contact',
    '#/contact/inbox',
    '#/settings/contact',
    '#/appearance/themes',
    '#/appearance/themes/install',
    '#/appearance/library',
    '#/studio',
    '#/studio/options',
    '#/studio/options/custom/optheme_options',
    '#/studio/menus',
    '#/studio?panel=menus',
    '#/patterns',
    '#/content-structure/content-types',
    '#/content-structure/taxonomies',
    '#/content-structure/field-groups',
    '#/content-structure/options-pages',
    '#/users',
    '#/tools/redirects',
    '#/settings/general',
  ];
  for (const hash of adminRoutes) {
    const parsed = parseAdminRoute(hash);
    assert.equal(isRouteAllowed(parsed, 'admin'), true, `admin must access ${hash}`);
    assert.equal(isRouteAllowed(parsed, 'editor'), false, `editor must be blocked from ${hash}`);
  }

  const editorAllowed = ['#/dashboard', '#/posts', '#/posts/new', '#/pages', '#/media', '#/users/profile'];
  for (const hash of editorAllowed) {
    const parsed = parseAdminRoute(hash);
    assert.equal(isRouteAllowed(parsed, 'editor'), true, `editor must access ${hash}`);
  }
  assert.equal(isViewAllowed('contact', 'editor'), false);
  assert.equal(isViewAllowed('contact', 'admin'), true);
  assert.equal(parseAdminView('#/contact'), 'contact');
});

test('buildMenuTree matches reorganized IA order, icons, groups, and clean Editor visibility', () => {
  const sampleCpts = [
    { descriptor: { key: 'post', label: 'Posts' } },
    { descriptor: { key: 'page', label: 'Pages' } },
    { descriptor: { key: 'pattern', label: 'Patterns' } },
    { descriptor: { key: 'project', label: 'Projects' } },
  ];
  const sampleTaxonomies = {
    project: [
      { key: 'project_category', label: 'Project Categories' },
      { key: 'tag', label: 'Tags' },
    ],
  };

  const sampleOptionsPages = [
    {
      page_key: 'optheme_options',
      menu_label: 'OPTheme options',
      parent_menu: 'appearance',
      menu_position: 10,
      min_edit_role: 'admin',
      provider_status: 'active',
      declaration_state: 'declared',
    },
    {
      page_key: 'editor_banner',
      menu_label: 'Editor Banner',
      parent_menu: 'appearance',
      menu_position: 20,
      min_edit_role: 'editor',
      provider_status: 'active',
      declaration_state: 'declared',
    },
    {
      page_key: 'inactive_theme_page',
      menu_label: 'Inactive Page',
      parent_menu: 'appearance',
      menu_position: 30,
      min_edit_role: 'admin',
      provider_status: 'provider_inactive',
      declaration_state: 'declared',
    },
  ];

  const fullTree = buildMenuTree(sampleCpts, true, sampleTaxonomies, sampleOptionsPages);
  assert.deepEqual(
    fullTree.map(item => item.id),
    [
      'dashboard',
      'posts',
      'pages',
      'cpt:project',
      'media',
      'contact',
      'themes',
      'studio',
      'patterns',
      'schema',
      'plugins',
      'users',
      'tools',
      'settings',
    ],
  );

  // Verify Appearance ordering, Customize Studio submenu & active-only appearance options pages
  const appearance = fullTree.filter(item => item.group === 'appearance');
  assert.deepEqual(appearance.map(item => item.id), ['themes', 'studio', 'patterns']);
  assert.deepEqual(appearance[0].children, [
    { label: 'Installed themes', hash: '#/appearance/themes' },
    { label: 'Install theme', hash: '#/appearance/themes/install' },
  ]);
  assert.equal(appearance[1].label, 'Customize Studio');
  assert.deepEqual(appearance[1].children, [
    { label: 'Site identity', hash: '#/studio/identity' },
    { label: 'Options', hash: '#/studio/options' },
    { label: 'Menus', hash: '#/studio/menus' },
    { label: 'OPTheme options', hash: '#/studio/options/custom/optheme_options' },
    { label: 'Editor Banner', hash: '#/studio/options/custom/editor_banner' },
    { label: 'Templates', hash: '#/studio/templates' },
  ]);
  assert.equal(appearance[2].icon, 'patterns');
  assert.deepEqual(appearance[2].children, [
    { label: 'All patterns', hash: '#/patterns' },
    { label: 'Add pattern', hash: '#/patterns/new' },
    { label: 'Template library', hash: '#/appearance/library' },
  ]);

  // Verify Contact, Content structure, Users, Tools, Settings
  const contact = fullTree.find(item => item.id === 'contact');
  assert.equal(contact.hash, '#/contact');
  assert.equal(contact.adminOnly, true);

  const schema = fullTree.find(item => item.id === 'schema');
  assert.deepEqual(schema.children, [
    { label: 'Content types', hash: '#/content-structure/content-types' },
    { label: 'Taxonomies', hash: '#/content-structure/taxonomies' },
    { label: 'Field groups', hash: '#/content-structure/field-groups' },
    { label: 'Options pages', hash: '#/content-structure/options-pages' },
  ]);

  const users = fullTree.find(item => item.id === 'users');
  assert.equal(users.adminOnly, true);
  assert.deepEqual(
    users.children.map(c => c.hash),
    ['#/users', '#/users/new'],
  );

  const tools = fullTree.find(item => item.id === 'tools');
  assert.deepEqual(
    tools.children.map(c => c.hash),
    ['#/tools/redirects', '#/tools/backup', '#/tools/media-reconcile', '#/tools/search', '#/tools/health'],
  );

  const settings = fullTree.find(item => item.id === 'settings');
  assert.deepEqual(
    settings.children.map(c => [c.label, c.hash]),
    [
      ['General', '#/settings/general'],
      ['Homepage & posts', '#/settings/reading'],
      ['Permalinks', '#/settings/permalinks'],
      ['Homepage SEO', '#/settings/seo'],
      ['Multilingual', '#/settings/multilingual'],
      ['Email notifications (SMTP)', '#/settings/smtp'],
    ],
  );

  // Verify Editor sees ONLY ungrouped Dashboard + Content (excluding Contact); zero Appearance/System items;
  // appearance options page granted min_edit_role='editor' falls back cleanly into Content -> #/options/:key.
  const editorTree = buildMenuTree(sampleCpts, false, sampleTaxonomies, sampleOptionsPages);
  const editorVisible = editorTree.filter(item => !item.adminOnly);
  assert.deepEqual(
    editorVisible.map(item => item.id),
    ['dashboard', 'posts', 'pages', 'cpt:project', 'options:editor_banner', 'media'],
  );
  const editorBanner = editorVisible.find(item => item.id === 'options:editor_banner');
  assert.equal(editorBanner.hash, '#/options/editor_banner');
  assert.equal(editorBanner.group, 'content');
  assert.equal(editorVisible.some(item => item.group === 'system' || item.group === 'appearance'), false);
});

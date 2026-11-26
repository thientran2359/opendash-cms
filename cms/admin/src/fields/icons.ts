export interface CoreIconDef {
  readonly name: string;
  readonly paths: string;
}

/** Curated 48-icon static registry synced 1:1 with `opendash-core/src/icons.rs::CORE_ICONS`. */
export const CORE_ICONS: readonly CoreIconDef[] = [
  { name: 'arrow-left', paths: '<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>' },
  { name: 'arrow-right', paths: '<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>' },
  { name: 'arrow-up', paths: '<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>' },
  { name: 'arrow-down', paths: '<path d="M12 5v14"/><path d="m19 12-7 7-7-7"/>' },
  { name: 'arrow-up-right', paths: '<path d="M7 17 17 7"/><path d="M7 7h10v10"/>' },
  { name: 'chevron-left', paths: '<path d="m15 18-6-6 6-6"/>' },
  { name: 'chevron-right', paths: '<path d="m9 18 6-6-6-6"/>' },
  { name: 'chevron-up', paths: '<path d="m18 15-6-6-6 6"/>' },
  { name: 'chevron-down', paths: '<path d="m6 9 6 6 6-6"/>' },
  { name: 'external-link', paths: '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M21 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h6"/>' },
  { name: 'menu', paths: '<path d="M4 6h16M4 12h16M4 18h16"/>' },
  { name: 'x', paths: '<path d="M18 6 6 18M6 6l12 12"/>' },
  { name: 'home', paths: '<path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/><path d="M9 21v-8h6v8"/>' },
  { name: 'calendar', paths: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M16 3v4M8 3v4M3 10h18"/>' },
  { name: 'clock', paths: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>' },
  { name: 'user', paths: '<circle cx="12" cy="8" r="4"/><path d="M4 21v-2a8 8 0 0 1 16 0v2"/>' },
  { name: 'users', paths: '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75"/>' },
  { name: 'tag', paths: '<path d="m20 13-7 7a2 2 0 0 1-3 0l-7-7V3h10l7 7a2 2 0 0 1 0 3Z"/><circle cx="8" cy="8" r="1"/>' },
  { name: 'folder', paths: '<path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v10H3z"/>' },
  { name: 'file-text', paths: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6M8 13h8M8 17h8"/>' },
  { name: 'book-open', paths: '<path d="M12 7a8 8 0 0 0-9-2v14a8 8 0 0 1 9 2 8 8 0 0 1 9-2V5a8 8 0 0 0-9 2zM12 7v14"/>' },
  { name: 'bookmark', paths: '<path d="M5 3h14v18l-7-5-7 5z"/>' },
  { name: 'eye', paths: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>' },
  { name: 'message-circle', paths: '<path d="M21 11.5a8.5 8.5 0 0 1-12 7.8L3 21l1.7-6A8.5 8.5 0 1 1 21 11.5Z"/>' },
  { name: 'share-2', paths: '<circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><path d="m8.7 10.7 6.6-4.4m-6.6 7 6.6 4.4"/>' },
  { name: 'link', paths: '<path d="M10 13a5 5 0 0 0 8 1l3-3a5 5 0 0 0-7-7l-2 2"/><path d="M14 11a5 5 0 0 0-8-1l-3 3a5 5 0 0 0 7 7l2-2"/>' },
  { name: 'search', paths: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>' },
  { name: 'check', paths: '<path d="m4 12 5 5L20 6"/>' },
  { name: 'check-circle', paths: '<circle cx="12" cy="12" r="10"/><path d="m7 12 3 3 7-7"/>' },
  { name: 'plus', paths: '<path d="M12 5v14M5 12h14"/>' },
  { name: 'minus', paths: '<path d="M5 12h14"/>' },
  { name: 'info', paths: '<circle cx="12" cy="12" r="10"/><path d="M12 11v6M12 7h.01"/>' },
  { name: 'alert-triangle', paths: '<path d="m10.3 3.8-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.7-3.2l-8-14a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 17h.01"/>' },
  { name: 'alert-circle', paths: '<circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/>' },
  { name: 'help-circle', paths: '<circle cx="12" cy="12" r="10"/><path d="M9 9a3 3 0 1 1 5 2c-1 1-2 1-2 3M12 17h.01"/>' },
  { name: 'sparkles', paths: '<path d="m12 3 2 6 6 2-6 2-2 6-2-6-6-2 6-2zM20 18l1 2-1 2-1-2z"/>' },
  { name: 'star', paths: '<path d="m12 2 3 7 7 .7-5.5 4.8 1.7 7.5L12 18l-6.2 4 1.7-7.5L2 9.7 9 9z"/>' },
  { name: 'heart', paths: '<path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8L12 21l8.8-8.6a5.5 5.5 0 0 0 0-7.8Z"/>' },
  { name: 'shield', paths: '<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"/>' },
  { name: 'lock', paths: '<rect x="4" y="11" width="16" height="11" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>' },
  { name: 'settings', paths: '<circle cx="12" cy="12" r="3"/><path d="M10 2h4l1 3 3 1 3-1 2 4-2 2v3l2 2-2 4-3-1-3 1-1 2h-4l-1-2-3-1-3 1-2-4 2-2v-3L1 9l2-4 3 1 3-1z"/>' },
  { name: 'mail', paths: '<rect x="2" y="4" width="20" height="16" rx="2"/><path d="m2 6 10 7 10-7"/>' },
  { name: 'phone', paths: '<path d="M22 17v3a2 2 0 0 1-2 2C10 22 2 14 2 4a2 2 0 0 1 2-2h3l3 5-2 3a16 16 0 0 0 6 6l3-2z"/>' },
  { name: 'map-pin', paths: '<path d="M20 10c0 6-8 12-8 12S4 16 4 10a8 8 0 1 1 16 0Z"/><circle cx="12" cy="10" r="3"/>' },
  { name: 'globe', paths: '<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"/>' },
  { name: 'sun', paths: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>' },
  { name: 'moon', paths: '<path d="M20.5 14A9 9 0 0 1 10 3.5 9 9 0 1 0 20.5 14Z"/>' },
  { name: 'download', paths: '<path d="M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4"/>' },
] as const;

export const CORE_ICON_MAP: ReadonlyMap<string, string> = new Map(
  CORE_ICONS.map(item => [item.name, item.paths]),
);

export const CORE_ICON_NAMES: readonly string[] = CORE_ICONS.map(item => item.name);

export function lookupCoreIconPaths(name: string): string | null {
  return CORE_ICON_MAP.get(name) ?? null;
}

import { __, _x } from '../../i18n/index.tsx';
import { formatDateTime } from '../../i18n/format.ts';
import type { RevisionSource } from '../../api/revisions.ts';

export type AutosaveStatus = 'idle' | 'saving' | 'saved' | 'error';

export interface AutosaveState {
  status: AutosaveStatus;
  savedAt: string | null;
  isStaleBase: boolean;
  errorMessage: string | null;
}

export type RevisionModalInitialCompare = {
  from: string;
  to: string;
} | null;

export function formatRevisionSource(source: RevisionSource): string {
  switch (source) {
    case 'create':
      return _x("Created", "revision.source");
    case 'save':
      return _x("Saved", "revision.source");
    case 'builder_save':
      return _x("Builder save", "revision.source");
    case 'builder_reset':
      return _x("Builder reset", "revision.source");
    case 'quick_edit':
      return _x("Quick edit", "revision.source");
    case 'seo_save':
      return _x("SEO save", "revision.source");
    case 'pattern_save':
      return _x("Pattern save", "revision.source");
    case 'plugin_save':
      return _x("Plugin save", "revision.source");
    case 'restore':
      return _x("Restored", "revision.source");
    case 'import':
      return _x("Imported", "revision.source");
    case 'baseline':
      return _x("Baseline", "revision.source");
  }
}

export function parseRevisionUtcTimestamp(raw?: string | null): Date | null {
  if (!raw) return null;
  const iso = raw.includes('T') ? raw : `${raw.replace(' ', 'T')}Z`;
  const parsed = new Date(iso);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

export function formatRevisionTimestamp(raw?: string | null): string {
  const parsed = parseRevisionUtcTimestamp(raw);
  if (!parsed) return raw || __("Unknown time");
  return formatDateTime(parsed, {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

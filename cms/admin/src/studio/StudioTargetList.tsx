import React from 'react';
import type { StudioTargetGroup, StudioTargetItem } from '../api/studio.ts';
import { AdminBadge } from '../components/AdminPrimitives.tsx';
import { __ } from '../i18n/index.tsx';

export const STUDIO_TARGET_GROUP_ORDER: StudioTargetGroup[] = [
  'parts',
  'core',
  'content_types',
  'taxonomies',
  'page_layouts',
  'custom_templates',
  'overrides',
];

export function studioTargetSourceBadge(
  item: StudioTargetItem,
): { label: string; tone: 'success' | 'neutral' | 'warning' } {
  if (item.effective_source.kind === 'db_override') {
    return { label: __("Page builder override"), tone: 'success' };
  }
  if (item.effective_source.kind === 'active_theme') {
    return { label: __("C-SEC theme"), tone: 'neutral' };
  }
  return { label: __("Core default"), tone: 'warning' };
}

interface StudioTargetListProps {
  targets: StudioTargetItem[];
  onSelectTarget: (targetKey: string | null) => void;
}

export const StudioTargetList: React.FC<StudioTargetListProps> = ({
  targets,
  onSelectTarget,
}) => {
  const groupLabels: Record<StudioTargetGroup, string> = {
    parts: __("Shared components (Parts)"),
    core: __("Core templates"),
    content_types: __("Content types (CPT)"),
    taxonomies: __("Taxonomies"),
    page_layouts: __("Page layouts"),
    custom_templates: __("Custom detail templates"),
    overrides: __("Other database overrides"),
  };

  return (
    <div className="flex h-full flex-col overflow-y-auto" data-testid="studio-panel-templates">
      <div className="space-y-4 p-3">
        {targets.length === 0 ? (
          <p className="px-1 text-xs text-zinc-500">{__("Loading templates…")}</p>
        ) : (
          STUDIO_TARGET_GROUP_ORDER.map(group => {
            const groupItems = targets.filter(item => item.group === group);
            if (groupItems.length === 0) return null;
            return (
              <div key={group} className="space-y-1">
                <p className="px-2 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">
                  {groupLabels[group]}
                </p>
                <ul className="m-0 list-none space-y-1 p-0">
                  {groupItems.map(item => {
                    const badge = studioTargetSourceBadge(item);
                    return (
                      <li key={item.canonical_target_key}>
                        <button
                          type="button"
                          data-testid={`studio-target-${item.canonical_target_key}`}
                          onClick={() => onSelectTarget(item.canonical_target_key)}
                          className="flex w-full items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-white px-2.5 py-2 text-left transition-colors hover:border-zinc-300 hover:bg-zinc-50"
                        >
                          <span className="min-w-0">
                            <span className="block truncate text-xs font-semibold text-zinc-900">{item.label}</span>
                            <code className="mt-0.5 block truncate font-mono text-[10px] text-zinc-500">
                              {item.canonical_target_key}
                            </code>
                          </span>
                          <AdminBadge tone={badge.tone}>{badge.label}</AdminBadge>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
};

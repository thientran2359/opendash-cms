import React, { useEffect, useRef, useState } from 'react';
import {
  getEditorLayout,
  getEffectiveEditorConfig,
  putEditorLayout,
} from '../../api/content.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';
import { __, useI18n } from '../../i18n/index.tsx';
import type {
  EditorLayoutConfigV1,
  EditorLayoutSnapshot,
  EffectiveEditorConfigProjection,
  LayoutFieldSubgroup,
  PanelPlacementConfig,
} from './types.ts';

interface EditorLayoutConfiguratorProps {
  typeKey: string;
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onVersionChange?: (version: number | null) => void;
  onDirtyChange?: (dirty: boolean) => void;
}

type RegionKey = 'main_top' | 'main_primary' | 'main_bottom' | 'sidebar';

const REGION_KEYS: readonly RegionKey[] = ['main_top', 'main_primary', 'main_bottom', 'sidebar'];

const REGION_LABELS: Record<RegionKey, string> = {
  get main_top() { return __("Top region (main_top)"); },
  get main_primary() { return __("Main region (main_primary)"); },
  get main_bottom() { return __("Bottom region (main_bottom)"); },
  get sidebar() { return __("Sidebar (sidebar)"); },
};

function renumberRegion(list: PanelPlacementConfig[]): PanelPlacementConfig[] {
  return list.map((p, idx) => ({ ...p, order: (idx + 1) * 10 }));
}

function serializeLayout(layout: EditorLayoutConfigV1 | null): string {
  return layout ? JSON.stringify(layout.regions) : '';
}

export const EditorLayoutConfigurator: React.FC<EditorLayoutConfiguratorProps> = ({
  typeKey,
  csrfToken,
  onApiError,
  onVersionChange,
  onDirtyChange,
}) => {
  useI18n();
  const [snapshot, setSnapshot] = useState<EditorLayoutSnapshot | null>(null);
  const [effectiveCfg, setEffectiveCfg] = useState<EffectiveEditorConfigProjection | null>(null);
  const [layout, setLayout] = useState<EditorLayoutConfigV1 | null>(null);
  const [expandedSubgroupsPanel, setExpandedSubgroupsPanel] = useState<string | null>(null);
  const [newSubgroupTitle, setNewSubgroupTitle] = useState('');
  const [dragItem, setDragItem] = useState<{ panelId: string; from: RegionKey } | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [liveStatus, setLiveStatus] = useState('');
  const onVersionChangeRef = useRef(onVersionChange);
  onVersionChangeRef.current = onVersionChange;

  const dirty = Boolean(
    snapshot && layout && serializeLayout(snapshot.layout) !== serializeLayout(layout),
  );

  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);

  const load = async () => {
    try {
      const [data, eff] = await Promise.all([
        getEditorLayout(typeKey),
        getEffectiveEditorConfig(typeKey).catch(() => null),
      ]);
      setSnapshot(data);
      setLayout(structuredClone(data.layout));
      setEffectiveCfg(eff);
    } catch (err) {
      onApiError(err, __("Could not load editor layout configuration."));
    }
  };

  useEffect(() => {
    setMessage(null);
    setExpandedSubgroupsPanel(null);
    void load();
  }, [typeKey]);

  useEffect(() => {
    onVersionChangeRef.current?.(snapshot?.version ?? null);
  }, [snapshot?.version]);

  if (!snapshot || !layout) {
    return <p className="text-sm text-zinc-600">{__("Loading editor layout configuration...")}</p>;
  }

  const movePanelToRegion = (panelId: string, from: RegionKey, to: RegionKey, targetIndex?: number) => {
    if (!snapshot) return;
    const def = snapshot.panels.find(p => p.panel_id === panelId);
    if (def && !def.allowed_regions.includes(to)) return;

    setLayout(prev => {
      if (!prev) return prev;
      const item = prev.regions[from].find(p => p.panel_id === panelId);
      if (!item) return prev;
      if (from === to) {
        const list = prev.regions[from].filter(p => p.panel_id !== panelId);
        const insertAt = targetIndex !== undefined ? Math.max(0, Math.min(targetIndex, list.length)) : list.length;
        list.splice(insertAt, 0, item);
        return { ...prev, regions: { ...prev.regions, [from]: renumberRegion(list) } };
      }
      const nextFrom = renumberRegion(prev.regions[from].filter(p => p.panel_id !== panelId));
      const destList = [...prev.regions[to]];
      const insertAt = targetIndex !== undefined ? Math.max(0, Math.min(targetIndex, destList.length)) : destList.length;
      destList.splice(insertAt, 0, item);
      return {
        ...prev,
        regions: {
          ...prev.regions,
          [from]: nextFrom,
          [to]: renumberRegion(destList),
        },
      };
    });
    setLiveStatus(__("Moved panel {{value1}} to {{value2}}.", { value1: def?.title ?? panelId, value2: to }));
  };

  const stepPanel = (panelId: string, region: RegionKey, delta: -1 | 1) => {
    const idx = layout.regions[region].findIndex(p => p.panel_id === panelId);
    if (idx < 0) return;
    const nextIdx = idx + delta;
    if (nextIdx < 0 || nextIdx >= layout.regions[region].length) return;
    movePanelToRegion(panelId, region, region, nextIdx);
  };

  const updatePlacement = (
    panelId: string,
    region: RegionKey,
    patch: Partial<PanelPlacementConfig>,
  ) => {
    setLayout(prev => {
      if (!prev) return prev;
      return {
        ...prev,
        regions: {
          ...prev.regions,
          [region]: prev.regions[region].map(p =>
            p.panel_id === panelId ? { ...p, ...patch } : p,
          ),
        },
      };
    });
  };

  const availableFieldKeysForPanel = (panelId: string): string[] => {
    if (!effectiveCfg) return [];
    const allProjs = [
      ...effectiveCfg.regions.main_top,
      ...effectiveCfg.regions.main_primary,
      ...effectiveCfg.regions.main_bottom,
      ...effectiveCfg.regions.sidebar,
    ];
    return allProjs.find(p => p.panel.panel_id === panelId)?.field_keys ?? [];
  };

  const addSubgroup = (panelId: string, region: RegionKey) => {
    const title = newSubgroupTitle.trim();
    if (!title) return;
    const pl = layout.regions[region].find(p => p.panel_id === panelId);
    if (!pl) return;
    const subgroup: LayoutFieldSubgroup = {
      subgroup_id: `sg_${Date.now().toString(36)}`,
      title,
      field_keys: [],
    };
    updatePlacement(panelId, region, { subgroups: [...pl.subgroups, subgroup] });
    setNewSubgroupTitle('');
    setLiveStatus(__("Added subgroup {{value1}}.", { value1: title }));
  };

  const toggleSubgroupField = (
    panelId: string,
    region: RegionKey,
    subgroupId: string,
    fieldKey: string,
    checked: boolean,
  ) => {
    const pl = layout.regions[region].find(p => p.panel_id === panelId);
    if (!pl) return;
    const nextSubs = pl.subgroups.map(sg => {
      if (sg.subgroup_id === subgroupId) {
        const nextKeys = checked
          ? Array.from(new Set([...sg.field_keys, fieldKey]))
          : sg.field_keys.filter(k => k !== fieldKey);
        return { ...sg, field_keys: nextKeys };
      }
      // Keep each field assigned to at most one subgroup inside the same panel.
      return checked
        ? { ...sg, field_keys: sg.field_keys.filter(k => k !== fieldKey) }
        : sg;
    });
    updatePlacement(panelId, region, { subgroups: nextSubs });
  };

  const removeSubgroup = (panelId: string, region: RegionKey, subgroupId: string) => {
    const pl = layout.regions[region].find(p => p.panel_id === panelId);
    if (!pl) return;
    updatePlacement(panelId, region, {
      subgroups: pl.subgroups.filter(sg => sg.subgroup_id !== subgroupId),
    });
  };

  const handleSave = async () => {
    if (!csrfToken || !layout) return;
    setSaving(true);
    setMessage(null);
    try {
      const updated = await putEditorLayout(typeKey, snapshot.version, layout, csrfToken);
      setSnapshot(updated);
      setLayout(structuredClone(updated.layout));
      setMessage(__("Editor layout saved (v{{value1}}).", { value1: updated.version }));
    } catch (err) {
      onApiError(err, __("Could not save the editor layout."));
    } finally {
      setSaving(false);
    }
  };

  const handleReset = () => {
    if (!snapshot) return;
    setLayout(structuredClone(snapshot.layout));
    setMessage(null);
  };

  const renderRegionCard = (region: RegionKey) => (
    <div
      key={region}
      className="rounded-lg border border-zinc-200 bg-zinc-50/60 p-3.5"
      onDragOver={e => {
        if (!dragItem) return;
        const def = snapshot.panels.find(p => p.panel_id === dragItem.panelId);
        if (!def || def.allowed_regions.includes(region)) {
          e.preventDefault();
        }
      }}
      onDrop={e => {
        e.preventDefault();
        if (dragItem) {
          movePanelToRegion(dragItem.panelId, dragItem.from, region);
          setDragItem(null);
        }
      }}
    >
      <div className="mb-2.5 flex items-center justify-between gap-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-zinc-600">
          {REGION_LABELS[region]}
        </span>
        <span className="text-xs text-zinc-400">{layout.regions[region].length}</span>
      </div>
      <ul className="list-none space-y-2 pl-0">
        {layout.regions[region].map((pl, idx) => {
          const def = snapshot.panels.find(p => p.panel_id === pl.panel_id);
          const allowedRegions = def?.allowed_regions?.length ? def.allowed_regions : [...REGION_KEYS];
          const isContentFieldsPanel = def?.kind.kind === 'content_fields';
          const panelFieldKeys = isContentFieldsPanel ? availableFieldKeysForPanel(pl.panel_id) : [];
          const subgroupsOpen = expandedSubgroupsPanel === pl.panel_id;
          return (
            <li
              key={pl.panel_id}
              draggable={Boolean(csrfToken && !saving)}
              onDragStart={() => setDragItem({ panelId: pl.panel_id, from: region })}
              onDragOver={e => {
                if (dragItem && allowedRegions.includes(region)) e.preventDefault();
              }}
              onDrop={e => {
                e.preventDefault();
                e.stopPropagation();
                if (dragItem && dragItem.panelId !== pl.panel_id) {
                  movePanelToRegion(dragItem.panelId, dragItem.from, region, idx);
                  setDragItem(null);
                }
              }}
              className="space-y-2 rounded-md border border-zinc-200 bg-white px-2.5 py-2 text-xs"
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-1.5">
                  <span className="cursor-grab text-zinc-400 select-none" aria-hidden="true">⋮⋮</span>
                  <span className="font-semibold text-zinc-900">{def?.title ?? pl.panel_id}</span>
                  <span className="font-mono text-zinc-400">({pl.panel_id})</span>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    disabled={idx === 0}
                    aria-label={__("Move panel {{value1}} up", { value1: def?.title ?? pl.panel_id })}
                    onClick={() => stepPanel(pl.panel_id, region, -1)}
                    className="inline-flex h-6 w-6 items-center justify-center rounded border border-zinc-300 bg-white text-zinc-700 hover:bg-zinc-50 disabled:opacity-40"
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    disabled={idx === layout.regions[region].length - 1}
                    aria-label={__("Move panel {{value1}} down", { value1: def?.title ?? pl.panel_id })}
                    onClick={() => stepPanel(pl.panel_id, region, 1)}
                    className="inline-flex h-6 w-6 items-center justify-center rounded border border-zinc-300 bg-white text-zinc-700 hover:bg-zinc-50 disabled:opacity-40"
                  >
                    ↓
                  </button>
                  <label className="inline-flex items-center gap-1 text-zinc-700">
                    <input
                      type="checkbox"
                      checked={pl.enabled}
                      disabled={Boolean(def?.required_core)}
                      onChange={() => updatePlacement(pl.panel_id, region, { enabled: !pl.enabled })}
                    />
                    {__("Show")}
                  </label>
                  {def?.collapsible !== false && (
                    <label className="inline-flex items-center gap-1 text-zinc-700">
                      <input
                        type="checkbox"
                        checked={pl.collapsed_by_default}
                        onChange={e => updatePlacement(pl.panel_id, region, { collapsed_by_default: e.target.checked })}
                      />
                      {__("Collapsed")}
                    </label>
                  )}
                  <select
                    aria-label={__("Region for {{value1}}", { value1: pl.panel_id })}
                    className="h-7 rounded border border-zinc-300 bg-white px-1.5 text-xs text-zinc-800 focus-visible:outline-2 focus-visible:outline-od-accent"
                    value={region}
                    onChange={e => movePanelToRegion(pl.panel_id, region, e.target.value as RegionKey)}
                  >
                    {allowedRegions.map(rk => (
                      <option key={rk} value={rk}>{rk}</option>
                    ))}
                  </select>
                  {isContentFieldsPanel && (
                    <button
                      type="button"
                      onClick={() => setExpandedSubgroupsPanel(prev => (prev === pl.panel_id ? null : pl.panel_id))}
                      className="h-7 rounded border border-zinc-300 bg-white px-2 text-xs font-medium text-zinc-700 hover:bg-zinc-50"
                    >
                      {__("Subgroups ({{value1}})", { value1: pl.subgroups.length })}
                    </button>
                  )}
                </div>
              </div>

              {isContentFieldsPanel && subgroupsOpen && (
                <div className="space-y-2 rounded border border-zinc-200 bg-zinc-50 p-2.5">
                  <div className="flex flex-wrap items-center gap-2">
                    <input
                      type="text"
                      className="h-7 flex-1 rounded border border-zinc-300 bg-white px-2 text-xs"
                      placeholder={__("New subgroup title…")}
                      value={newSubgroupTitle}
                      maxLength={80}
                      onChange={e => setNewSubgroupTitle(e.target.value)}
                    />
                    <AdminButton
                      type="button"
                      variant="secondary"
                      size="sm"
                      disabled={!newSubgroupTitle.trim()}
                      onClick={() => addSubgroup(pl.panel_id, region)}
                    >
                      {__("+ Add subgroup")}
                    </AdminButton>
                  </div>
                  {pl.subgroups.map(sg => (
                    <div key={sg.subgroup_id} className="space-y-1.5 rounded border border-zinc-200 bg-white p-2">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-semibold text-zinc-800">{sg.title}</span>
                        <button
                          type="button"
                          onClick={() => removeSubgroup(pl.panel_id, region, sg.subgroup_id)}
                          className="text-xs font-medium text-rose-600 hover:underline"
                        >
                          {__("Remove subgroup")}
                        </button>
                      </div>
                      {panelFieldKeys.length === 0 ? (
                        <p className="text-[11px] text-zinc-500">{__("No active fields available to assign.")}</p>
                      ) : (
                        <div className="flex flex-wrap gap-2">
                          {panelFieldKeys.map(fk => (
                            <label key={fk} className="inline-flex items-center gap-1 rounded border border-zinc-200 bg-zinc-50 px-2 py-0.5 text-[11px] text-zinc-700">
                              <input
                                type="checkbox"
                                checked={sg.field_keys.includes(fk)}
                                onChange={e => toggleSubgroupField(pl.panel_id, region, sg.subgroup_id, fk, e.target.checked)}
                              />
                              <span>{effectiveCfg?.effective_fields[fk]?.definition.label ?? fk} ({fk})</span>
                            </label>
                          ))}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );

  return (
    <div className="space-y-5 rounded-xl border border-zinc-200 bg-white p-5" data-testid="editor-layout-configurator">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-zinc-100 pb-3">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-base font-semibold text-zinc-950">{__("Single Edit layout")}</h3>
            <AdminBadge tone="neutral">v{snapshot.version}</AdminBadge>
            {dirty && <AdminBadge tone="warning">{__("Unsaved changes")}</AdminBadge>}
          </div>
          <p className="mt-0.5 text-xs text-zinc-500">
            {__("Drag or reorder core and field-group panels across the four editor regions, toggle default collapse state, and organize subgroups.")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {dirty && (
            <AdminButton type="button" variant="secondary" size="md" onClick={handleReset} disabled={saving}>
              {__("Discard changes")}
            </AdminButton>
          )}
          <AdminButton variant="primary" size="md" onClick={() => void handleSave()} disabled={!dirty || saving || !csrfToken}>
            {saving ? __("Saving layout...") : __("Save layout")}
          </AdminButton>
        </div>
      </div>

      <span className="sr-only" aria-live="polite">{liveStatus}</span>
      {message && <AdminAlert tone="success">{message}</AdminAlert>}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(280px,1fr)]">
        <div className="space-y-4">
          {(['main_top', 'main_primary', 'main_bottom'] as const).map(renderRegionCard)}
        </div>
        <div className="space-y-4">
          {renderRegionCard('sidebar')}
        </div>
      </div>
    </div>
  );
};

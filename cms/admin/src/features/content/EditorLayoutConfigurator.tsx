import { tStatic, useI18n } from '../../i18n/index.tsx';
import React, { useEffect, useRef, useState } from 'react';
import { getEditorLayout, putEditorLayout } from '../../api/content.ts';
import type {
  BaseFieldSlicePanelConfig,
  FieldSpec,
  EditorLayoutConfigV1,
  EditorLayoutSnapshot,
  PanelPlacementConfig,
} from './types.ts';
import { AdminAlert, AdminBadge, AdminButton } from '../../components/AdminPrimitives.tsx';

interface EditorLayoutConfiguratorProps {
  typeKey: string;
  baseFields: FieldSpec[];
  csrfToken: string | null;
  onApiError: (error: unknown, fallback: string) => void;
  onVersionChange?: (version: number | null) => void;
}

type RegionKey = 'main_top' | 'main_primary' | 'main_bottom' | 'sidebar';

const REGION_LABELS: Record<RegionKey, string> = {
  get main_top() { return tStatic("feature.content.editorlayoutconfigurator.top_region_main_top"); },
  get main_primary() { return tStatic("feature.content.editorlayoutconfigurator.main_region_main_primary"); },
  get main_bottom() { return tStatic("feature.content.editorlayoutconfigurator.bottom_region_main_bottom"); },
  get sidebar() { return tStatic("feature.content.editorlayoutconfigurator.sidebar_sidebar"); },
};

export const EditorLayoutConfigurator: React.FC<EditorLayoutConfiguratorProps> = ({
  typeKey,
  baseFields,
  csrfToken,
  onApiError,
  onVersionChange,
}) => {
  useI18n();
  const [snapshot, setSnapshot] = useState<EditorLayoutSnapshot | null>(null);
  const [layout, setLayout] = useState<EditorLayoutConfigV1 | null>(null);
  const [showAddSliceForm, setShowAddSliceForm] = useState(false);
  const [newSliceKey, setNewSliceKey] = useState('');
  const [newSliceTitle, setNewSliceTitle] = useState('');
  const [newSliceRegion, setNewSliceRegion] = useState<RegionKey>('main_bottom');
  const [newSliceFields, setNewSliceFields] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const onVersionChangeRef = useRef(onVersionChange);
  onVersionChangeRef.current = onVersionChange;

  const load = async () => {
    try {
      const data = await getEditorLayout(typeKey);
      setSnapshot(data);
      setLayout(data.layout);
    } catch (err) {
      onApiError(err, tStatic("feature.content.editorlayoutconfigurator.could_not_load_editor_layout_configuration"));
    }
  };

  useEffect(() => {
    setMessage(null);
    setShowAddSliceForm(false);
    void load();
  }, [typeKey]);

  useEffect(() => {
    onVersionChangeRef.current?.(snapshot?.version ?? null);
  }, [snapshot?.version]);

  if (!snapshot || !layout) {
    return <p className="text-sm text-zinc-600">{tStatic("feature.content.editorlayoutconfigurator.loading_editor_layout_configuration")}</p>;
  }

  const movePanelToRegion = (panelId: string, from: RegionKey, to: RegionKey) => {
    if (from === to) return;
    setLayout(prev => {
      if (!prev) return prev;
      const item = prev.regions[from].find(p => p.panel_id === panelId);
      if (!item) return prev;
      const nextFrom = prev.regions[from].filter(p => p.panel_id !== panelId);
      const nextOrder = (prev.regions[to].reduce((m, p) => Math.max(m, p.order), 0)) + 10;
      const nextTo = [...prev.regions[to], { ...item, order: nextOrder }];
      return {
        ...prev,
        regions: {
          ...prev.regions,
          [from]: nextFrom,
          [to]: nextTo,
        },
      };
    });
  };

  const togglePanelEnabled = (panelId: string, region: RegionKey) => {
    setLayout(prev => {
      if (!prev) return prev;
      return {
        ...prev,
        regions: {
          ...prev.regions,
          [region]: prev.regions[region].map(p =>
            p.panel_id === panelId ? { ...p, enabled: !p.enabled } : p,
          ),
        },
      };
    });
  };

  const addBaseSlicePanel = (e: React.FormEvent) => {
    e.preventDefault();
    const cleanKey = newSliceKey.trim();
    if (!cleanKey || !newSliceTitle.trim() || newSliceFields.length === 0) return;
    const panelId = cleanKey.startsWith('layout:') ? cleanKey : `layout:${cleanKey}`;
    setLayout(prev => {
      if (!prev) return prev;
      const slice: BaseFieldSlicePanelConfig = {
        panel_id: panelId,
        title: newSliceTitle.trim(),
        collapsible: true,
        field_keys: newSliceFields,
        subgroups: [],
      };
      const placement: PanelPlacementConfig = {
        panel_id: panelId,
        enabled: true,
        order: (prev.regions[newSliceRegion].reduce((m, p) => Math.max(m, p.order), 0)) + 10,
        collapsed_by_default: false,
        subgroups: [],
      };
      return {
        ...prev,
        base_field_panels: [...prev.base_field_panels, slice],
        regions: {
          ...prev.regions,
          [newSliceRegion]: [...prev.regions[newSliceRegion], placement],
        },
      };
    });
    setNewSliceKey('');
    setNewSliceTitle('');
    setNewSliceFields([]);
    setShowAddSliceForm(false);
  };

  const removeBaseSlicePanel = (panelId: string) => {
    setLayout(prev => {
      if (!prev) return prev;
      return {
        ...prev,
        base_field_panels: prev.base_field_panels.filter(s => s.panel_id !== panelId),
        regions: {
          main_top: prev.regions.main_top.filter(p => p.panel_id !== panelId),
          main_primary: prev.regions.main_primary.filter(p => p.panel_id !== panelId),
          main_bottom: prev.regions.main_bottom.filter(p => p.panel_id !== panelId),
          sidebar: prev.regions.sidebar.filter(p => p.panel_id !== panelId),
        },
      };
    });
  };

  const handleSave = async () => {
    if (!csrfToken || !layout) return;
    setSaving(true);
    setMessage(null);
    try {
      const updated = await putEditorLayout(typeKey, snapshot.version, layout, csrfToken);
      setSnapshot(updated);
      setLayout(updated.layout);
      setMessage(tStatic("feature.content.editorlayoutconfigurator.editor_layout_saved_vvalue", { value1: updated.version }));
    } catch (err) {
      onApiError(err, tStatic("api.content.save_the_editor_layout"));
    } finally {
      setSaving(false);
    }
  };

  const renderRegionCard = (region: RegionKey) => (
    <div key={region} className="rounded-lg border border-zinc-200 bg-zinc-50/60 p-3.5">
      <div className="mb-2.5 flex items-center justify-between gap-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-zinc-600">
          {REGION_LABELS[region]}
        </span>
        <span className="text-xs text-zinc-400">{layout.regions[region].length}</span>
      </div>
      <ul className="space-y-2">
        {layout.regions[region].map(pl => {
          const def = snapshot.panels.find(p => p.panel_id === pl.panel_id);
          return (
            <li key={pl.panel_id} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-zinc-200 bg-white px-2.5 py-2 text-xs">
              <div>
                <span className="font-semibold text-zinc-900">{def?.title ?? pl.panel_id}</span>
                <span className="ml-1.5 font-mono text-zinc-400">({pl.panel_id})</span>
              </div>
              <div className="flex items-center gap-2">
                <label className="inline-flex items-center gap-1 text-zinc-700">
                  <input
                    type="checkbox"
                    checked={pl.enabled}
                    onChange={() => togglePanelEnabled(pl.panel_id, region)}
                  />
                  {tStatic("feature.content.editorlayoutconfigurator.show")}
                </label>
                <select
                  aria-label={tStatic("feature.content.editorlayoutconfigurator.region_for_value", { value1: pl.panel_id })}
                  className="h-7 rounded border border-zinc-300 bg-white px-1.5 text-xs text-zinc-800 focus-visible:outline-2 focus-visible:outline-od-accent"
                  value={region}
                  onChange={e => movePanelToRegion(pl.panel_id, region, e.target.value as RegionKey)}
                >
                  {(Object.keys(REGION_LABELS) as RegionKey[]).map(rk => (
                    <option key={rk} value={rk}>{rk}</option>
                  ))}
                </select>
                {pl.panel_id.startsWith('layout:') && (
                  <AdminButton variant="danger" size="sm" className="h-7 px-2 text-xs" onClick={() => removeBaseSlicePanel(pl.panel_id)}>
                    {tStatic("multilingual.delete")}
                  </AdminButton>
                )}
              </div>
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
            <h3 className="text-base font-semibold text-zinc-950">{tStatic("feature.content.editorlayoutconfigurator.single_edit_layout")}</h3>
            <AdminBadge tone="neutral">v{snapshot.version}</AdminBadge>
          </div>
          <p className="mt-0.5 text-xs text-zinc-500">
            {tStatic("feature.content.editorlayoutconfigurator.divide_base_fields")}<code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-zinc-700">base_field_panels</code>{tStatic("feature.content.editorlayoutconfigurator.and_arrange_panels_in_four_content_regions")}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {baseFields.length > 0 && (
            <AdminButton
              variant="secondary"
              size="sm"
              onClick={() => setShowAddSliceForm(prev => !prev)}
            >
              {showAddSliceForm
                ? tStatic('content_types.close_base_slice_form')
                : tStatic('content_types.add_base_slice_toggle')}
            </AdminButton>
          )}
          <AdminButton variant="primary" size="md" onClick={() => void handleSave()} disabled={saving || !csrfToken}>
            {saving ? tStatic("feature.content.editorlayoutconfigurator.saving_layout") : tStatic("feature.content.editorlayoutconfigurator.save_layout")}
          </AdminButton>
        </div>
      </div>

      {message && <AdminAlert tone="success">{message}</AdminAlert>}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(280px,1fr)]">
        <div className="space-y-4">
          {(['main_top', 'main_primary', 'main_bottom'] as const).map(renderRegionCard)}
        </div>
        <div className="space-y-4">
          {renderRegionCard('sidebar')}
        </div>
      </div>

      {baseFields.length > 0 && showAddSliceForm && (
        <form onSubmit={addBaseSlicePanel} className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/50 p-4">
          <div className="text-sm font-semibold text-zinc-900">
            {tStatic("feature.content.editorlayoutconfigurator.create_a_base_field_slice")}<code className="rounded bg-zinc-100 px-1 py-0.5 font-mono text-xs text-zinc-700">base_field_panels</code>)
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <input
              type="text"
              placeholder={tStatic("feature.common.ui_slice_key_vd_project_info")}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
              value={newSliceKey}
              onChange={e => setNewSliceKey(e.target.value)}
              required
            />
            <input
              type="text"
              placeholder={tStatic("feature.content.editorlayoutconfigurator.panel_title")}
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
              value={newSliceTitle}
              onChange={e => setNewSliceTitle(e.target.value)}
              required
            />
            <select
              className="h-9 w-full rounded-md border border-zinc-300 bg-white px-2.5 text-sm text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
              value={newSliceRegion}
              onChange={e => setNewSliceRegion(e.target.value as RegionKey)}
            >
              {(Object.keys(REGION_LABELS) as RegionKey[]).map(rk => (
                <option key={rk} value={rk}>{REGION_LABELS[rk]}</option>
              ))}
            </select>
          </div>
          <div className="flex flex-wrap gap-2 text-xs">
            {baseFields.map(f => (
              <label key={f.key} className="inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-2.5 py-1.5 text-zinc-700 hover:border-zinc-300">
                <input
                  type="checkbox"
                  checked={newSliceFields.includes(f.key)}
                  onChange={e => {
                    setNewSliceFields(prev =>
                      e.target.checked ? [...prev, f.key] : prev.filter(k => k !== f.key),
                    );
                  }}
                />
                {f.label} ({f.key})
              </label>
            ))}
          </div>
          <AdminButton type="submit" variant="secondary" size="sm">
            {tStatic("feature.content.editorlayoutconfigurator.add_panel_to_layout_remember_to_save_the_layout")}
          </AdminButton>
        </form>
      )}
    </div>
  );
};

import React, { useEffect, useState } from 'react';
import { UnifiedFieldForm } from '../fields/UnifiedFieldForm.tsx';
import { __ } from '../i18n/index.tsx';
import type { BlockCatalogItemDto } from '../types.ts';
import {
  type BuilderDocument,
  type BuilderViewport,
  type NodeMeta,
  canDeleteNode,
  findActiveCatalogItem,
  hasLockedAncestor,
  isLockedNode,
  locate,
  nodeTitle,
  nodeTypeTitle,
} from './document.ts';

export interface BuilderInspectorProps {
  document: BuilderDocument;
  selectedId: string | null;
  blockSchemas: BlockCatalogItemDto[];
  csrfToken: string;
  viewport: BuilderViewport;
  contextTypeKey?: string | null;
  locale?: string | null;
  siteTimezone?: string;
  requiresContentSlot?: boolean;
  hideHeader?: boolean;
  hideDeleteButton?: boolean;
  onChangeProps: (id: string, nextProps: Record<string, unknown>) => void;
  onChangeMeta: (id: string, patch: Partial<NodeMeta>) => void;
  onDelete?: (id: string) => void;
  onApiError?: (error: unknown, fallback: string) => void;
}

export const BuilderInspector: React.FC<BuilderInspectorProps> = ({
  document,
  selectedId,
  blockSchemas,
  csrfToken,
  viewport,
  contextTypeKey,
  locale,
  siteTimezone,
  requiresContentSlot = false,
  hideHeader = false,
  hideDeleteButton = false,
  onChangeProps,
  onChangeMeta,
  onDelete,
  onApiError,
}) => {
  const item = locate(document, selectedId);
  const node = item?.node;
  const [nameInput, setNameInput] = useState('');
  const [htmlIdInput, setHtmlIdInput] = useState('');
  const [classInput, setClassInput] = useState('');
  const [metaError, setMetaError] = useState<string | null>(null);

  useEffect(() => {
    setNameInput(node?.meta?.name ?? '');
    setHtmlIdInput(node?.meta?.html_id ?? '');
    setClassInput(node?.meta?.class_name ?? '');
    setMetaError(null);
  }, [node?.id, node?.meta?.name, node?.meta?.html_id, node?.meta?.class_name]);

  if (!item || !node) {
    return (
      <div className="p-4 text-xs text-slate-500">
        {__("Select a block on the tree or canvas to edit its settings.")}
      </div>
    );
  }

  const locked = isLockedNode(node, blockSchemas) || hasLockedAncestor(document, node.id, blockSchemas);
  const schema = findActiveCatalogItem(blockSchemas, node.block_id)?.schema;
  const deletable = canDeleteNode(document, node.id, { requiresContentSlot, blockCatalog: blockSchemas });

  const commitTextMeta = (
    field: 'name' | 'html_id' | 'class_name',
    rawValue: string,
    finalTrim: boolean,
  ): void => {
    const trimmed = rawValue.trim();
    if (!finalTrim && rawValue !== rawValue.trimEnd()) return;
    if (field === 'html_id' && trimmed && !/^[a-zA-Z0-9_-]{1,64}$/.test(trimmed)) {
      setMetaError(__("HTML ID may only contain letters, numbers, '-' or '_' (max 64)."));
      return;
    }
    if (field === 'class_name' && trimmed && !/^[a-zA-Z0-9_ -]{1,256}$/.test(trimmed)) {
      setMetaError(__("CSS classes may only contain ASCII letters, numbers, spaces, '-' or '_' (max 256)."));
      return;
    }
    setMetaError(null);
    onChangeMeta(node.id, { [field]: trimmed });
  };

  const visibility = node.meta?.visibility ?? {};
  const toggleVisibility = (key: 'hide_desktop' | 'hide_tablet' | 'hide_mobile', checked: boolean): void => {
    onChangeMeta(node.id, { visibility: { ...visibility, [key]: checked } });
  };

  return (
    <div className="space-y-4">
      {!hideHeader && (
        <div className="flex items-start justify-between gap-2 border-b border-slate-200 pb-3">
          <div className="min-w-0">
            <h3 className="truncate text-sm font-semibold text-slate-900">{nodeTitle(item, blockSchemas)}</h3>
            <p className="truncate font-mono text-[11px] text-slate-500">{node.block_id}</p>
          </div>
          {!hideDeleteButton && onDelete && deletable && (
            <button
              type="button"
              onClick={() => onDelete(node.id)}
              className="shrink-0 rounded border border-rose-200 px-2 py-1 text-xs font-medium text-rose-700 hover:bg-rose-50"
            >
              {__("Delete")}
            </button>
          )}
        </div>
      )}

      {locked || !schema ? (
        <div className="space-y-3">
          <div className="rounded-md border border-amber-200 bg-amber-50 p-3 text-xs text-amber-900">
            {__("This block belongs to an inactive or unavailable provider and is preserved read-only.")}
          </div>
          {node.props && Object.keys(node.props).length > 0 && (
            <pre className="max-h-60 overflow-auto rounded border border-slate-200 bg-slate-50 p-2 font-mono text-[11px] text-slate-700">
              {JSON.stringify(node.props, null, 2)}
            </pre>
          )}
        </div>
      ) : (
        <>
          {schema.fields.length > 0 ? (
            <UnifiedFieldForm
              fields={schema.fields}
              context="studio_block"
              layoutMode="sidebar"
              viewport={viewport}
              values={node.props ?? {}}
              onChange={nextProps => onChangeProps(node.id, nextProps)}
              csrfToken={csrfToken}
              siteTimezone={siteTimezone}
              contextTypeKey={contextTypeKey ?? undefined}
              locale={locale ?? undefined}
              onApiError={onApiError ?? (() => {})}
            />
          ) : (
            <p className="text-xs text-slate-500">{__("This block has no configurable fields.")}</p>
          )}

          <details className="rounded-md border border-slate-200 bg-slate-50/60 p-3">
            <summary className="cursor-pointer select-none text-xs font-semibold text-slate-700">
              {__("Block attributes & visibility")}
            </summary>
            <div className="mt-3 space-y-3">
              {metaError && (
                <p className="rounded border border-rose-200 bg-rose-50 px-2 py-1 text-[11px] text-rose-700">
                  {metaError}
                </p>
              )}
              <label className="block space-y-1 text-xs">
                <span className="font-medium text-slate-700">{__("Custom label")}</span>
                <input
                  type="text"
                  maxLength={80}
                  value={nameInput}
                  placeholder={nodeTypeTitle(item, blockSchemas)}
                  onChange={e => { setNameInput(e.target.value); commitTextMeta('name', e.target.value, false); }}
                  onBlur={() => commitTextMeta('name', nameInput, true)}
                  className="w-full rounded border border-slate-300 bg-white px-2 py-1 text-xs text-slate-900"
                />
              </label>
              <label className="block space-y-1 text-xs">
                <span className="font-medium text-slate-700">{__("HTML anchor ID")}</span>
                <input
                  type="text"
                  maxLength={64}
                  value={htmlIdInput}
                  placeholder="hero-section"
                  onChange={e => { setHtmlIdInput(e.target.value); commitTextMeta('html_id', e.target.value, false); }}
                  onBlur={() => commitTextMeta('html_id', htmlIdInput, true)}
                  className="w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono text-xs text-slate-900"
                />
              </label>
              <label className="block space-y-1 text-xs">
                <span className="font-medium text-slate-700">{__("CSS classes")}</span>
                <input
                  type="text"
                  maxLength={256}
                  value={classInput}
                  placeholder="my-card shadow-sm"
                  onChange={e => { setClassInput(e.target.value); commitTextMeta('class_name', e.target.value, false); }}
                  onBlur={() => commitTextMeta('class_name', classInput, true)}
                  className="w-full rounded border border-slate-300 bg-white px-2 py-1 font-mono text-xs text-slate-900"
                />
              </label>
              <div className="space-y-1.5 pt-1">
                <span className="block text-xs font-medium text-slate-700">{__("Responsive visibility")}</span>
                <div className="grid grid-cols-3 gap-2 text-[11px] text-slate-700">
                  {([
                    ['hide_desktop', __("Hide desktop")],
                    ['hide_tablet', __("Hide tablet")],
                    ['hide_mobile', __("Hide mobile")],
                  ] as const).map(([key, label]) => (
                    <label key={key} className="flex items-center gap-1.5">
                      <input
                        type="checkbox"
                        checked={Boolean(visibility[key])}
                        onChange={e => toggleVisibility(key, e.target.checked)}
                      />
                      <span>{label}</span>
                    </label>
                  ))}
                </div>
              </div>
            </div>
          </details>
        </>
      )}
    </div>
  );
};

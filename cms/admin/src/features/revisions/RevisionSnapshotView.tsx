import React from 'react';
import { __ } from '../../i18n/index.tsx';
import type { RevisionDetailDto } from '../../api/revisions.ts';
import { RevisionMissingRefsNotice } from './RevisionMissingRefsNotice.tsx';

export interface RevisionSnapshotViewProps {
  loadingDetail: boolean;
  detailError: string | null;
  detailData: RevisionDetailDto | null;
}

export const RevisionSnapshotView: React.FC<RevisionSnapshotViewProps> = ({
  loadingDetail,
  detailError,
  detailData,
}) => (
  <>
    {loadingDetail && (
      <p className="text-xs text-zinc-500">{__("Loading snapshot details…")}</p>
    )}
    {detailError && (
      <p role="alert" className="text-xs font-medium text-rose-700">
        {detailError}
      </p>
    )}
    {detailData && (
      <div className="space-y-3 text-xs" data-testid="revision-snapshot-detail">
        <div className="grid grid-cols-1 gap-2 rounded-lg border border-zinc-200 bg-zinc-50 p-3 sm:grid-cols-2">
          <p>
            <strong>{__("Title:")}</strong> {detailData.title || '—'}
          </p>
          <p>
            <strong>{__("Slug:")}</strong> <code>{detailData.slug}</code>
          </p>
          <p>
            <strong>{__("Content mode:")}</strong> <code>{detailData.content_mode}</code>
          </p>
          <p>
            <strong>{__("Author:")}</strong> {detailData.revision.author_username}
          </p>
          {detailData.page_layout_key && (
            <p>
              <strong>{__("Page layout:")}</strong> <code>{detailData.page_layout_key}</code>
            </p>
          )}
          {detailData.custom_template_key && (
            <p>
              <strong>{__("Custom template:")}</strong>{' '}
              <code>{detailData.custom_template_key}</code>
            </p>
          )}
        </div>
        {detailData.has_hidden_admin_fields && (
          <p className="rounded border border-amber-200 bg-amber-50 px-3 py-2 text-amber-900">
            {__(
              "Administrator-only fields in this snapshot are hidden from your role.",
            )}
          </p>
        )}
        <RevisionMissingRefsNotice report={detailData.missing_refs} variant="info" />
        {detailData.fields.length > 0 && (
          <div className="space-y-1.5 rounded-lg border border-zinc-200 bg-white p-3">
            <h2 className="font-semibold text-zinc-800">{__("Fields")}</h2>
            {detailData.fields.map(f => (
              <div key={f.key} className="border-t border-zinc-100 pt-1.5">
                <div className="font-medium text-zinc-900">
                  {f.label}{' '}
                  <code className="text-[11px] text-zinc-500">({f.key})</code>
                </div>
                <pre className="mt-1 whitespace-pre-wrap break-words rounded bg-zinc-50 p-2 font-mono text-[11px] text-zinc-700">
                  {typeof f.value === 'string'
                    ? f.value
                    : JSON.stringify(f.value, null, 2)}
                </pre>
              </div>
            ))}
          </div>
        )}
        {detailData.builder_document && (
          <div className="rounded-lg border border-zinc-200 bg-white p-3">
            <strong>{__("Builder sections:")}</strong>{' '}
            {detailData.builder_document.sections.length}
          </div>
        )}
        {detailData.terms.length > 0 && (
          <div className="rounded-lg border border-zinc-200 bg-white p-3">
            <strong>{__("Taxonomy terms:")}</strong>{' '}
            {detailData.terms.map(t => t.name).join(', ')}
          </div>
        )}
        {detailData.seo && (
          <div className="space-y-1 rounded-lg border border-zinc-200 bg-white p-3">
            <h2 className="font-semibold text-zinc-800">{__("SEO metadata")}</h2>
            <pre className="whitespace-pre-wrap break-words rounded bg-zinc-50 p-2 font-mono text-[11px] text-zinc-700">
              {JSON.stringify(detailData.seo, null, 2)}
            </pre>
          </div>
        )}
      </div>
    )}
  </>
);

import React from 'react';
import { __, _x } from '../../i18n/index.tsx';
import {
  hasAnyMissingRefsInfo,
  type BlockDiffItem,
  type DiffOpKind,
  type FieldDiffItem,
  type RevisionCompareResponse,
  type TextDiffResult,
  type WordSpan,
} from '../../api/revisions.ts';
import { RevisionMissingRefsNotice } from './RevisionMissingRefsNotice.tsx';

export interface RevisionDiffViewProps {
  compare: RevisionCompareResponse;
}

function formatDiffValue(val: unknown): string {
  if (val === undefined) return '';
  if (val === null) return _x("(empty)", "revision.diff_empty");
  if (typeof val === 'string') return val === '' ? _x("(empty)", "revision.diff_empty") : val;
  if (typeof val === 'number' || typeof val === 'boolean') return String(val);
  try {
    return JSON.stringify(val, null, 2);
  } catch {
    return String(val);
  }
}

function spanClass(kind: DiffOpKind): string {
  switch (kind) {
    case 'added':
      return 'rounded bg-emerald-200/80 px-0.5 text-emerald-950';
    case 'removed':
      return 'rounded bg-rose-200/80 px-0.5 text-rose-950 line-through';
    case 'modified':
      return 'rounded bg-amber-200/80 px-0.5 text-amber-950';
    default:
      return '';
  }
}

const WordSpanText: React.FC<{ spans?: WordSpan[]; fallback: string }> = ({
  spans,
  fallback,
}) => {
  if (!spans || spans.length === 0) {
    return <span>{fallback}</span>;
  }
  return (
    <span>
      {spans.map((span, idx) => (
        <span key={idx} className={spanClass(span.kind)}>
          {span.text}
        </span>
      ))}
    </span>
  );
};

const TextDiffSection: React.FC<{
  title: string;
  diff: TextDiffResult;
}> = ({ title, diff }) => (
  <section className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
        {title}
      </h2>
      {diff.unchanged_collapsed_count > 0 && (
        <span className="text-[11px] text-zinc-500">
          {__("{{value1}} unchanged lines collapsed", {
            value1: diff.unchanged_collapsed_count,
          })}
        </span>
      )}
    </div>
    {diff.truncated_diff && (
      <p className="rounded border border-amber-200 bg-amber-50 px-2.5 py-1.5 text-[11px] text-amber-900">
        {__(
          "Large content diff was truncated to keep comparison fast. Inspect individual snapshots for the complete text.",
        )}
      </p>
    )}
    {diff.hunks.length === 0 ? (
      <p className="text-xs text-zinc-500">{__("No text changes.")}</p>
    ) : (
      <div className="space-y-2 overflow-x-auto font-mono text-xs">
        {diff.hunks.map((hunk, hunkIdx) => (
          <div
            key={hunkIdx}
            className="overflow-hidden rounded border border-zinc-200 bg-zinc-50"
          >
            <div className="border-b border-zinc-200 bg-zinc-100 px-2.5 py-1 text-[11px] text-zinc-600">
              {hunk.header}
            </div>
            <div className="divide-y divide-zinc-100">
              {hunk.lines.map((line, lineIdx) => {
                const rowClass =
                  line.kind === 'added'
                    ? 'bg-emerald-50/90 text-emerald-950'
                    : line.kind === 'removed'
                      ? 'bg-rose-50/90 text-rose-950'
                      : 'bg-white text-zinc-700';
                const prefix =
                  line.kind === 'added' ? '+' : line.kind === 'removed' ? '-' : ' ';
                return (
                  <div
                    key={lineIdx}
                    className={`flex items-start gap-2 px-2 py-1 ${rowClass}`}
                  >
                    <span className="w-8 shrink-0 select-none text-right text-[10px] text-zinc-400">
                      {line.line_no_before ?? ''}
                    </span>
                    <span className="w-8 shrink-0 select-none text-right text-[10px] text-zinc-400">
                      {line.line_no_after ?? ''}
                    </span>
                    <span className="w-3 shrink-0 select-none font-bold">{prefix}</span>
                    <div className="min-w-0 flex-1 whitespace-pre-wrap break-words">
                      <WordSpanText spans={line.word_spans} fallback={line.text} />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    )}
  </section>
);

function blockKindBadgeClass(kind: BlockDiffItem['kind']): string {
  switch (kind) {
    case 'added':
      return 'bg-emerald-100 text-emerald-900 border-emerald-200';
    case 'removed':
      return 'bg-rose-100 text-rose-900 border-rose-200';
    case 'moved':
      return 'bg-sky-100 text-sky-900 border-sky-200';
    case 'modified':
      return 'bg-amber-100 text-amber-900 border-amber-200';
  }
}

const FieldDiffTable: React.FC<{ title: string; items: FieldDiffItem[] }> = ({
  title,
  items,
}) => (
  <section className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3">
    <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
      {title}
    </h2>
    <div className="space-y-2">
      {items.map(item => (
        <div
          key={item.key}
          className="rounded-md border border-zinc-200 bg-zinc-50/70 p-2.5 text-xs space-y-1.5"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="font-semibold text-zinc-900">
              {item.label}{' '}
              <code className="font-mono text-[11px] font-normal text-zinc-500">
                ({item.key} · {item.field_type})
              </code>
            </span>
            {!item.editable && (
              <span className="rounded bg-zinc-200 px-1.5 py-0.5 text-[10px] font-medium text-zinc-700">
                {__("Read-only")}
              </span>
            )}
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            <div className="rounded border border-rose-200 bg-rose-50/70 p-2">
              <div className="text-[10px] font-semibold uppercase text-rose-800">
                {__("Before")}
              </div>
              <pre className="mt-1 whitespace-pre-wrap break-words font-mono text-[11px] text-rose-950">
                {formatDiffValue(item.before)}
              </pre>
            </div>
            <div className="rounded border border-emerald-200 bg-emerald-50/70 p-2">
              <div className="text-[10px] font-semibold uppercase text-emerald-800">
                {__("After")}
              </div>
              <pre className="mt-1 whitespace-pre-wrap break-words font-mono text-[11px] text-emerald-950">
                {formatDiffValue(item.after)}
              </pre>
            </div>
          </div>
        </div>
      ))}
    </div>
  </section>
);

export const RevisionDiffView: React.FC<RevisionDiffViewProps> = ({ compare }) => {
  const { diff, from_meta, to_meta, from_missing_refs, to_missing_refs } = compare;
  const taxDiff = diff.taxonomy_diff;
  const primaryChanged =
    (taxDiff.primary_category_before?.id ?? null) !==
    (taxDiff.primary_category_after?.id ?? null);
  const hasTaxChanges =
    taxDiff.added_terms.length > 0 ||
    taxDiff.removed_terms.length > 0 ||
    primaryChanged;

  const hasAnyDiff =
    diff.title_diff.hunks.length > 0 ||
    diff.mode_transition !== null ||
    (diff.body_text_diff !== null && diff.body_text_diff.hunks.length > 0) ||
    (diff.body_block_diff !== null && diff.body_block_diff.items.length > 0) ||
    diff.field_diffs.length > 0 ||
    hasTaxChanges ||
    diff.layout_diff !== null ||
    diff.template_diff !== null ||
    Boolean(diff.seo_diff && diff.seo_diff.length > 0);

  return (
    <div className="space-y-3" data-testid="revision-diff-view">
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-zinc-200 bg-zinc-50 px-3 py-2 text-xs text-zinc-700">
        <div>
          <span className="font-semibold text-zinc-900">{from_meta.label}</span>
          <span className="mx-1.5 text-zinc-400">-&gt;</span>
          <span className="font-semibold text-zinc-900">{to_meta.label}</span>
        </div>
        <span className="text-[11px] text-zinc-500">
          {__("v{{value1}} -> v{{value2}}", {
            value1: from_meta.entry_version,
            value2: to_meta.entry_version,
          })}
        </span>
      </div>

      {diff.has_hidden_admin_changes && (
        <div
          role="note"
          className="rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-950"
        >
          {__(
            "Administrator-only fields differ between these snapshots and are hidden from this view.",
          )}
        </div>
      )}

      {diff.mode_transition && (
        <div className="rounded-lg border border-sky-200 bg-sky-50 px-3 py-2 text-xs text-sky-950">
          {__("Content mode changed: {{value1}} -> {{value2}}", {
            value1: diff.mode_transition.from,
            value2: diff.mode_transition.to,
          })}
        </div>
      )}

      {!hasAnyDiff && (
        <p className="rounded-lg border border-zinc-200 bg-white px-3 py-4 text-center text-xs text-zinc-500">
          {__("No visible differences between the selected snapshots.")}
        </p>
      )}

      {diff.title_diff.hunks.length > 0 && (
        <TextDiffSection title={__("Title")} diff={diff.title_diff} />
      )}

      {diff.body_text_diff && diff.body_text_diff.hunks.length > 0 && (
        <TextDiffSection title={__("Body content")} diff={diff.body_text_diff} />
      )}

      {diff.body_block_diff && diff.body_block_diff.items.length > 0 && (
        <section className="space-y-2 rounded-lg border border-zinc-200 bg-white p-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
              {__("Builder blocks")}
            </h2>
            <span className="text-[11px] text-zinc-500">
              {__("Nodes: {{value1}} -> {{value2}} ({{value3}} unchanged)", {
                value1: diff.body_block_diff.total_nodes_before,
                value2: diff.body_block_diff.total_nodes_after,
                value3: diff.body_block_diff.unchanged_count,
              })}
            </span>
          </div>
          {diff.body_block_diff.truncated && (
            <p className="rounded border border-amber-200 bg-amber-50 px-2.5 py-1.5 text-[11px] text-amber-900">
              {__("Block comparison was truncated due to document size.")}
            </p>
          )}
          <div className="space-y-2">
            {diff.body_block_diff.items.map((item, idx) => (
              <div
                key={`${item.node_id}-${idx}`}
                className="rounded-md border border-zinc-200 bg-zinc-50/70 p-2.5 text-xs space-y-1.5"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span
                    className={`rounded border px-1.5 py-0.5 text-[10px] font-semibold uppercase ${blockKindBadgeClass(item.kind)}`}
                  >
                    {item.kind}
                  </span>
                  <span className="font-semibold text-zinc-900">{item.label}</span>
                  <code className="font-mono text-[11px] text-zinc-500">
                    {item.node_kind} · {item.block_type} ({item.node_id})
                  </code>
                </div>
                {(item.before_path || item.after_path) && (
                  <p className="text-[11px] text-zinc-600">
                    {item.before_path ?? '—'} {'->'} {item.after_path ?? '—'}
                  </p>
                )}
                {item.prop_changes && item.prop_changes.length > 0 && (
                  <ul className="space-y-1 border-t border-zinc-200 pt-1.5 font-mono text-[11px]">
                    {item.prop_changes.map(pc => (
                      <li key={pc.prop_path} className="break-words text-zinc-700">
                        <strong>{pc.prop_path}:</strong>{' '}
                        <span className="text-rose-800">{formatDiffValue(pc.before)}</span>{' '}
                        {'->'}{' '}
                        <span className="text-emerald-800">{formatDiffValue(pc.after)}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            ))}
          </div>
        </section>
      )}

      {diff.field_diffs.length > 0 && (
        <FieldDiffTable title={__("Fields")} items={diff.field_diffs} />
      )}

      {hasTaxChanges && (
        <section className="space-y-1.5 rounded-lg border border-zinc-200 bg-white p-3 text-xs">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
            {__("Taxonomy terms")}
          </h2>
          {taxDiff.added_terms.length > 0 && (
            <p className="text-emerald-800">
              +{taxDiff.added_terms
                .map(t => (t.missing ? `${t.name} [${__("missing")}]` : t.name))
                .join(', ')}
            </p>
          )}
          {taxDiff.removed_terms.length > 0 && (
            <p className="text-rose-800">
              -{taxDiff.removed_terms
                .map(t => (t.missing ? `${t.name} [${__("missing")}]` : t.name))
                .join(', ')}
            </p>
          )}
          {primaryChanged && (
            <p className="text-zinc-700">
              {__("Primary category:")}{' '}
              {taxDiff.primary_category_before?.name ?? _x("(none)", "revision.none")}{' '}
              {'->'}{' '}
              {taxDiff.primary_category_after?.name ?? _x("(none)", "revision.none")}
            </p>
          )}
        </section>
      )}

      {(diff.layout_diff || diff.template_diff) && (
        <section className="space-y-1 rounded-lg border border-zinc-200 bg-white p-3 text-xs text-zinc-700">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-700">
            {__("Theme & Layout")}
          </h2>
          {diff.layout_diff && (
            <p>
              {__("Page layout:")}{' '}
              <code>{diff.layout_diff.before ?? _x("(default)", "revision.default")}</code>{' '}
              {'->'}{' '}
              <code>{diff.layout_diff.after ?? _x("(default)", "revision.default")}</code>
            </p>
          )}
          {diff.template_diff && (
            <p>
              {__("Custom template:")}{' '}
              <code>{diff.template_diff.before ?? _x("(default)", "revision.default")}</code>{' '}
              {'->'}{' '}
              <code>{diff.template_diff.after ?? _x("(default)", "revision.default")}</code>
            </p>
          )}
        </section>
      )}

      {diff.seo_diff && diff.seo_diff.length > 0 && (
        <FieldDiffTable title={__("SEO metadata")} items={diff.seo_diff} />
      )}

      {hasAnyMissingRefsInfo(from_missing_refs) && (
        <RevisionMissingRefsNotice
          report={from_missing_refs}
          heading={__("References unavailable in source snapshot ({{value1}})", {
            value1: from_meta.label,
          })}
          variant="info"
        />
      )}

      {hasAnyMissingRefsInfo(to_missing_refs) && (
        <RevisionMissingRefsNotice
          report={to_missing_refs}
          heading={__("References unavailable in target snapshot ({{value1}})", {
            value1: to_meta.label,
          })}
          variant="info"
        />
      )}
    </div>
  );
};

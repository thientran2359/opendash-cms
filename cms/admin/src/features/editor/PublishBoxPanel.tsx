import React, { useState } from 'react';
import { __, _x } from '../../i18n/index.tsx';
import { formatDateTime } from '../../i18n/format.ts';

export interface PublishBoxPanelProps {
  entryId: number | null;
  publicationStatus: 'draft' | 'published';
  isDraftPublish: boolean;
  isPublic: boolean;
  draftPreviewPath?: string | null;
  createdAt?: string | null;
  updatedAt?: string | null;
  onCreatedAtChange?: (isoUtc: string | null) => void;
  onUpdatedAtChange?: (isoUtc: string | null) => void;
  revisionsControlNode?: React.ReactNode;
  onPublicationStatusChange: (status: 'draft' | 'published') => void;
  onSave: (intent?: 'draft' | 'published') => void;
  onDelete?: () => void;
  onOpenBuilder: () => void;
  canOpenBuilder: boolean;
  saving: boolean;
  dirty: boolean;
}

function parseUtcTimestamp(raw?: string | null): Date | null {
  if (!raw) return null;
  const iso = raw.includes('T') ? raw : `${raw.replace(' ', 'T')}Z`;
  const parsed = new Date(iso);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function formatTimestamp(raw?: string | null): string {
  const parsed = parseUtcTimestamp(raw);
  if (!parsed) return raw || __("On save");
  return formatDateTime(parsed, {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function toDatetimeLocalValue(raw?: string | null): string {
  const parsed = parseUtcTimestamp(raw) ?? new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${parsed.getFullYear()}-${pad(parsed.getMonth() + 1)}-${pad(parsed.getDate())}T${pad(parsed.getHours())}:${pad(parsed.getMinutes())}`;
}

function fromDatetimeLocalValue(localVal: string): string | null {
  if (!localVal) return null;
  const parsed = new Date(localVal);
  if (Number.isNaN(parsed.getTime())) return null;
  return parsed.toISOString().slice(0, 19).replace('T', ' ');
}

export const PublishBoxPanel: React.FC<PublishBoxPanelProps> = ({
  entryId,
  publicationStatus,
  isDraftPublish,
  isPublic,
  draftPreviewPath,
  createdAt,
  updatedAt,
  onCreatedAtChange,
  onUpdatedAtChange,
  revisionsControlNode,
  onPublicationStatusChange,
  onSave,
  onDelete,
  onOpenBuilder,
  canOpenBuilder,
  saving,
  dirty,
}) => {
  const [editingStatus, setEditingStatus] = useState(false);
  const [editingCreatedAt, setEditingCreatedAt] = useState(false);
  const [editingUpdatedAt, setEditingUpdatedAt] = useState(false);

  return (
    <div className="space-y-3.5">
      {((isDraftPublish && publicationStatus === 'draft') || draftPreviewPath) && (
        <div className="flex flex-wrap items-center justify-between gap-2">
          {isDraftPublish && publicationStatus === 'draft' ? (
            <button
              type="button"
              onClick={() => onSave('draft')}
              disabled={saving}
              className="inline-flex h-8 items-center justify-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 transition-colors hover:border-zinc-400 hover:bg-zinc-50 disabled:opacity-50"
            >
              {saving ? __("Saving...") : _x("Save draft", "feature.editor")}
            </button>
          ) : (
            <span />
          )}
          {draftPreviewPath && (
            <a
              href={draftPreviewPath}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex h-8 items-center justify-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 no-underline transition-colors hover:border-zinc-400 hover:bg-zinc-50"
            >
              {__("Preview draft")}
            </a>
          )}
        </div>
      )}

      <div className="space-y-2 border-y border-zinc-100 py-2.5 text-xs text-zinc-600">
        <div className="flex flex-wrap items-center justify-between gap-1.5">
          <span>
            {__("Status:")}{' '}
            <strong className="font-semibold text-zinc-900">
              {publicationStatus === 'published' ? __("Published") : __("Draft")}
            </strong>
          </span>
          {isDraftPublish && (
            <button
              type="button"
              onClick={() => setEditingStatus(prev => !prev)}
              className="text-od-accent hover:underline"
            >
              {editingStatus ? __("Close") : _x("Edit", "shell.edit")}
            </button>
          )}
        </div>
        {isDraftPublish && editingStatus && (
          <div className="flex items-center gap-2 pt-1">
            <select
              aria-label={__("Publication status")}
              className="h-8 flex-1 rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
              value={publicationStatus}
              onChange={e => onPublicationStatusChange(e.target.value as 'draft' | 'published')}
            >
              <option value="draft">{__("Draft")}</option>
              <option value="published">{__("Published")}</option>
            </select>
            <button
              type="button"
              onClick={() => setEditingStatus(false)}
              className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2.5 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
            >
              OK
            </button>
          </div>
        )}

        <div className="flex items-center justify-between gap-1.5">
          <span>
            {__("Visibility:")}{' '}
            <strong className="font-semibold text-zinc-900">
              {isPublic ? __("Public") : __("Private")}
            </strong>
          </span>
          {dirty && <span className="font-medium text-amber-700">{__("● Unsaved")}</span>}
        </div>

        <div className="space-y-1.5">
          <div className="flex items-center justify-between gap-1.5">
            <span>
              {__("Created:")}{' '}
              <strong className="font-medium text-zinc-800">
                {formatTimestamp(createdAt)}
              </strong>
            </span>
            {onCreatedAtChange && (
              <button
                type="button"
                onClick={() => setEditingCreatedAt(prev => !prev)}
                className="text-od-accent hover:underline"
              >
                {editingCreatedAt ? __("Close") : _x("Edit", "shell.edit")}
              </button>
            )}
          </div>
          {editingCreatedAt && onCreatedAtChange && (
            <div className="flex items-center gap-1.5">
              <input
                type="datetime-local"
                aria-label={__("Created date")}
                className="h-8 flex-1 rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                value={toDatetimeLocalValue(createdAt)}
                onChange={e => onCreatedAtChange(fromDatetimeLocalValue(e.target.value))}
              />
              <button
                type="button"
                onClick={() => setEditingCreatedAt(false)}
                className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
              >
                OK
              </button>
            </div>
          )}
        </div>

        {entryId !== null && (
          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-1.5">
              <span>
                {__("Updated:")}{' '}
                <strong className="font-medium text-zinc-800">
                  {formatTimestamp(updatedAt)}
                </strong>
              </span>
              {onUpdatedAtChange && (
                <button
                  type="button"
                  onClick={() => setEditingUpdatedAt(prev => !prev)}
                  className="text-od-accent hover:underline"
                >
                  {editingUpdatedAt ? __("Close") : _x("Edit", "shell.edit")}
                </button>
              )}
            </div>
            {editingUpdatedAt && onUpdatedAtChange && (
              <div className="flex items-center gap-1.5">
                <input
                  type="datetime-local"
                  aria-label={__("Updated date")}
                  className="h-8 flex-1 rounded-md border border-zinc-300 bg-white px-2 text-xs text-zinc-900 focus-visible:outline-2 focus-visible:outline-od-accent"
                  value={toDatetimeLocalValue(updatedAt)}
                  onChange={e => onUpdatedAtChange(fromDatetimeLocalValue(e.target.value))}
                />
                <button
                  type="button"
                  onClick={() => setEditingUpdatedAt(false)}
                  className="inline-flex h-8 items-center rounded-md border border-zinc-300 bg-white px-2 text-xs font-medium text-zinc-800 hover:bg-zinc-50"
                >
                  OK
                </button>
              </div>
            )}
          </div>
        )}

        {revisionsControlNode}
      </div>

      {canOpenBuilder && (
        <button
          type="button"
          onClick={onOpenBuilder}
          className="inline-flex h-8 w-full items-center justify-center rounded-md border border-zinc-300 bg-white px-3 text-xs font-medium text-zinc-800 transition-colors hover:border-zinc-400 hover:bg-zinc-50"
        >
          {__("Edit with Builder")}
        </button>
      )}

      <div className="flex flex-wrap items-center justify-between gap-2 pt-0.5">
        {onDelete ? (
          <button
            type="button"
            disabled={saving}
            onClick={onDelete}
            data-testid="single-edit-move-to-trash"
            className="text-xs font-medium text-rose-600 hover:text-rose-700 hover:underline disabled:opacity-50"
          >
            {__("Move to Trash")}
          </button>
        ) : (
          <span />
        )}
        <button
          type="button"
          disabled={saving}
          onClick={() =>
            onSave(
              isDraftPublish
                ? publicationStatus === 'draft' && !editingStatus
                  ? 'published'
                  : publicationStatus
                : undefined,
            )
          }
          className="inline-flex h-9 items-center justify-center rounded-md bg-od-accent px-4 text-sm font-medium text-white shadow-2xs transition-colors hover:bg-od-accent-hover disabled:opacity-50"
        >
          {saving
            ? __("Saving...")
            : isDraftPublish
              ? publicationStatus === 'published'
                ? __("Update")
                : __("Publish post")
              : entryId === null
                ? __("Create new")
                : __("Update")}
        </button>
      </div>
    </div>
  );
};

import React, { useCallback, useEffect, useState } from 'react';
import { listFieldGroups } from '../../api/content.ts';
import type { FieldGroupRecord } from '../content/types.ts';
import { __, useI18n } from '../../i18n/index.tsx';
import { FieldGroupEditorPage } from './FieldGroupEditorPage.tsx';
import { FieldGroupsListPage } from './FieldGroupsListPage.tsx';

interface FieldGroupsWorkspaceProps {
  selectedGroupKey?: string;
  csrfToken: string | null;
  onNavigateHash: (hash: string) => void;
  onApiError: (error: unknown, fallback: string) => void;
  onDirtyChange: (dirty: boolean) => void;
}

export const FieldGroupsWorkspace: React.FC<FieldGroupsWorkspaceProps> = ({
  selectedGroupKey,
  csrfToken,
  onNavigateHash,
  onApiError,
  onDirtyChange,
}) => {
  useI18n();
  const [groups, setGroups] = useState<FieldGroupRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const [duplicateSource, setDuplicateSource] = useState<FieldGroupRecord | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await listFieldGroups();
      setGroups(list);
    } catch (cause) {
      onApiError(cause, __("Could not load field groups."));
    } finally {
      setLoading(false);
    }
  }, [onApiError]);

  useEffect(() => {
    void load();
  }, [revision, load]);

  if (selectedGroupKey === 'new') {
    return (
      <FieldGroupEditorPage
        key={`new-${duplicateSource?.group_id ?? 'blank'}`}
        existing={null}
        duplicateSource={duplicateSource}
        csrfToken={csrfToken}
        onSaved={() => {
          setDuplicateSource(null);
          setRevision(r => r + 1);
          onNavigateHash('#/content-structure/field-groups');
        }}
        onCancel={() => {
          setDuplicateSource(null);
          onNavigateHash('#/content-structure/field-groups');
        }}
        onApiError={onApiError}
        onDirtyChange={onDirtyChange}
      />
    );
  }

  if (selectedGroupKey) {
    const found = groups.find(g => g.group_id === selectedGroupKey || g.local_key === selectedGroupKey) ?? null;
    if (loading && !found) {
      return <p role="status" className="text-sm text-zinc-500">{__("Loading field group…")}</p>;
    }
    if (!found) {
      return (
        <div className="space-y-3">
          <p className="text-sm text-rose-700">{__("Field group \"{{value1}}\" was not found.", { value1: selectedGroupKey })}</p>
          <button type="button" onClick={() => onNavigateHash('#/content-structure/field-groups')}
            className="text-xs font-medium text-od-accent hover:underline">
            {__("Back to list")}
          </button>
        </div>
      );
    }
    return (
      <FieldGroupEditorPage
        key={`${found.group_id}:${found.version}`}
        existing={found}
        csrfToken={csrfToken}
        onSaved={() => {
          setRevision(r => r + 1);
          onNavigateHash('#/content-structure/field-groups');
        }}
        onCancel={() => onNavigateHash('#/content-structure/field-groups')}
        onApiError={onApiError}
        onDirtyChange={onDirtyChange}
      />
    );
  }

  return (
    <FieldGroupsListPage
      groups={groups}
      loading={loading}
      csrfToken={csrfToken}
      onRefresh={() => setRevision(r => r + 1)}
      onCreateNew={() => {
        setDuplicateSource(null);
        onNavigateHash('#/content-structure/field-groups/new');
      }}
      onEdit={gid => {
        setDuplicateSource(null);
        onNavigateHash(`#/content-structure/field-groups/${encodeURIComponent(gid)}`);
      }}
      onDuplicate={g => {
        setDuplicateSource(g);
        onNavigateHash('#/content-structure/field-groups/new');
      }}
      onApiError={onApiError}
    />
  );
};

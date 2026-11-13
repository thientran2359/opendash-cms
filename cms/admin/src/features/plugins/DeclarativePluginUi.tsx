import React, { useEffect, useRef, useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { getPluginUiSchema, invokePluginAction } from '../../api/plugins.ts';
import {
  CheckboxField,
  SelectField,
  TextAreaField,
  TextField,
} from '../../components/FormPrimitives.tsx';
import {
  buildDraftKey,
  InMemoryDraftStore,
  validateDeclarativeSchemaV1,
  type DeclarativeUiSchemaV1,
  type UiNode,
} from './declarativeSchema.ts';

interface DeclarativePluginUiProps {
  pluginId: string;
  csrfToken: string | null;
  onUnauthorized: () => void;
}

const sharedDraftStore = new InMemoryDraftStore();

export const DeclarativePluginUi: React.FC<DeclarativePluginUiProps> = ({
  pluginId,
  csrfToken,
  onUnauthorized,
}) => {
  const [schema, setSchema] = useState<DeclarativeUiSchemaV1 | null>(null);
  const [activeScreenId, setActiveScreenId] = useState<string>('');
  const [loadError, setLoadError] = useState<string>('');
  const [actionResult, setActionResult] = useState<string>('');
  const [actionError, setActionError] = useState<string>('');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [, setDraftTick] = useState<number>(0);
  const submittingRef = useRef<boolean>(false);

  useEffect(() => {
    if (!pluginId) {
      setSchema(null);
      return;
    }
    const controller = new AbortController();
    setLoadError('');
    setActionResult('');
    setActionError('');
    getPluginUiSchema(pluginId, controller.signal)
      .then(data => {
        const validated = validateDeclarativeSchemaV1(data);
        setSchema(validated);
        setActiveScreenId(validated.screens[0]?.id ?? '');
      })
      .catch(err => {
        if (controller.signal.aborted) return;
        if (err instanceof ApiClientError && err.status === 401) {
          onUnauthorized();
          setLoadError('Phiên đăng nhập đã hết hạn (401). Bản nháp được giữ nguyên.');
          return;
        }
        setLoadError(err instanceof Error ? err.message : 'Không tải được declarative UI.');
      });
    return () => controller.abort();
  }, [pluginId, onUnauthorized]);

  if (!pluginId) return null;
  if (loadError) {
    return <div className="notice notice-error">{loadError}</div>;
  }
  if (!schema || schema.screens.length === 0) {
    return null;
  }

  const activeScreen =
    schema.screens.find(s => s.id === activeScreenId) ?? schema.screens[0];

  const dispatchAction = async (actionId: string, formId: string) => {
    if (submittingRef.current) return;
    submittingRef.current = true;
    setIsSubmitting(true);
    setActionError('');
    setActionResult('');

    const draftKey = buildDraftKey(
      schema.plugin_id,
      activeScreen.id,
      formId,
      null,
      schema.schema_version,
    );
    const payload = sharedDraftStore.get(draftKey);
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), 10000);

    try {
      const data = await invokePluginAction(
        schema.plugin_id,
        actionId,
        payload,
        csrfToken,
        controller.signal,
      );
      setActionResult(JSON.stringify(data, null, 2));
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 401) {
        // Keep draft in memory; never auto-resubmit after re-login.
        onUnauthorized();
        setActionError('Phiên đăng nhập hết hạn (401). Vui lòng đăng nhập lại; bản nháp đã được giữ nguyên.');
      } else if (err instanceof ApiClientError) {
        const codeLabel = [403, 412, 422].includes(err.status) ? ` (${err.status})` : '';
        setActionError(`${err.message}${codeLabel}`);
      } else if (err instanceof DOMException && err.name === 'AbortError') {
        setActionError('Yêu cầu quá thời gian chờ (timeout). Bản nháp vẫn được giữ.');
      } else {
        setActionError(err instanceof Error ? err.message : 'Lỗi kết nối.');
      }
    } finally {
      window.clearTimeout(timer);
      submittingRef.current = false;
      setIsSubmitting(false);
    }
  };

  const renderNode = (node: UiNode, formId: string): React.ReactNode => {
    const draftKey = buildDraftKey(
      schema.plugin_id,
      activeScreen.id,
      formId,
      null,
      schema.schema_version,
    );
    const draft = sharedDraftStore.get(draftKey);

    if (node.kind === 'form') {
      const currentFormId = node.id;
      return (
        <form
          key={node.id}
          onSubmit={e => {
            e.preventDefault();
            if (node.action_id) {
              void dispatchAction(node.action_id, currentFormId);
            }
          }}
        >
          {node.label && <h4>{node.label}</h4>}
          {(node.children ?? []).map(child => renderNode(child, currentFormId))}
        </form>
      );
    }
    if (node.kind === 'field') {
      const ft = node.field_type ?? 'text';
      const val = draft[node.id];
      if (ft === 'checkbox') {
        return (
          <CheckboxField
            key={node.id}
            id={`plugin-field-${node.id}`}
            label={node.label ?? node.id}
            checked={Boolean(val)}
            onChange={checked => {
              sharedDraftStore.setField(draftKey, node.id, checked);
              setDraftTick(t => t + 1);
            }}
          />
        );
      }
      if (ft === 'textarea') {
        return (
          <TextAreaField
            key={node.id}
            id={`plugin-field-${node.id}`}
            label={node.label ?? node.id}
            value={typeof val === 'string' ? val : ''}
            onChange={next => {
              sharedDraftStore.setField(draftKey, node.id, next);
              setDraftTick(t => t + 1);
            }}
          />
        );
      }
      if (ft === 'select') {
        return (
          <SelectField
            key={node.id}
            id={`plugin-field-${node.id}`}
            label={node.label ?? node.id}
            value={typeof val === 'string' ? val : (node.options?.[0] ?? '')}
            options={(node.options ?? []).map(opt => ({ value: opt, label: opt }))}
            onChange={event => {
              sharedDraftStore.setField(draftKey, node.id, event.target.value);
              setDraftTick(t => t + 1);
            }}
          />
        );
      }
      return (
        <TextField
          key={node.id}
          id={`plugin-field-${node.id}`}
          name={`plugin-field-${node.id}`}
          label={node.label ?? node.id}
          value={typeof val === 'string' ? val : ''}
          onChange={next => {
            sharedDraftStore.setField(draftKey, node.id, next);
            setDraftTick(t => t + 1);
          }}
        />
      );
    }
    if (node.kind === 'button') {
      return (
        <button
          key={node.id}
          type="button"
          className="btn-primary"
          disabled={isSubmitting}
          onClick={() => {
            if (node.action_id) {
              void dispatchAction(node.action_id, formId);
            }
          }}
        >
          {isSubmitting ? 'Đang thực thi...' : (node.label ?? node.action_id ?? 'Thực thi')}
        </button>
      );
    }
    if (node.kind === 'tabs') {
      return (
        <div key={node.id} className="declarative-tabs">
          {(node.children ?? []).map(child => renderNode(child, formId))}
        </div>
      );
    }
    if (node.kind === 'table' || node.kind === 'list') {
      return (
        <div key={node.id} className="declarative-collection">
          {node.label && <strong>{node.label}</strong>}
          {node.columns && node.columns.length > 0 && (
            <div className="muted">Cột: {node.columns.join(', ')}</div>
          )}
        </div>
      );
    }
    return null;
  };

  return (
    <section aria-label="Declarative Plugin UI" style={{ marginTop: 16 }}>
      <h3>Giao diện khai báo ({schema.plugin_id})</h3>
      {schema.screens.length > 1 && (
        <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
          {schema.screens.map(scr => (
            <button
              key={scr.id}
              type="button"
              onClick={() => setActiveScreenId(scr.id)}
            >
              {scr.title}
            </button>
          ))}
        </div>
      )}
      {activeScreen.nodes.map(node => renderNode(node, 'default'))}
      {actionError && <div className="notice notice-error">{actionError}</div>}
      {actionResult && <pre>{actionResult}</pre>}
    </section>
  );
};

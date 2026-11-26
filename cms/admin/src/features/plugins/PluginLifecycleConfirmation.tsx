import React, { useState } from 'react';
import { ApiClientError } from '../../api/client.ts';
import { confirmPluginLifecycleAction } from '../../api/plugins.ts';
import { AdminAlert, AdminButton } from '../../components/AdminPrimitives.tsx';
import { useI18n } from '../../i18n/index.tsx';
import type { PluginLifecycleConfirmation as PluginLifecycleConfirmationDto, PluginRunResult } from '../../types.ts';

export interface PluginLifecycleConfirmationProps {
  pluginId: string;
  actionId: string;
  confirmation: PluginLifecycleConfirmationDto;
  csrfToken: string | null;
  onResolved: (result: PluginRunResult) => void;
  onCancelled: (result: PluginRunResult | null) => void;
  onFailed: (message: string, status?: number) => void;
  onUnauthorized?: () => void;
}

/**
 * Core-owned, host-controlled confirmation UI for privileged plugin lifecycle actions
 * (`purge` or immediate public `restore`). Displays ONLY host-verified metadata and never
 * renders guest HTML or guest response confirmation objects.
 */
export const PluginLifecycleConfirmation: React.FC<PluginLifecycleConfirmationProps> = ({
  pluginId,
  actionId,
  confirmation,
  csrfToken,
  onResolved,
  onCancelled,
  onFailed,
  onUnauthorized,
}) => {
  const { t } = useI18n();
  const [submitting, setSubmitting] = useState<'confirm' | 'cancel' | null>(null);

  const isPurge = confirmation.operation === 'purge';

  const submitDecision = async (decision: 'confirm' | 'cancel') => {
    if (!csrfToken || submitting) return;
    setSubmitting(decision);
    try {
      const result = await confirmPluginLifecycleAction(
        pluginId,
        actionId,
        confirmation.confirmation_id,
        decision,
        csrfToken,
      );
      if (decision === 'cancel' || result.code === 'confirmation_cancelled') {
        onCancelled(result);
      } else {
        onResolved(result);
      }
    } catch (err) {
      if (err instanceof ApiClientError && err.status === 401) {
        onUnauthorized?.();
        onFailed(t('plugin_lifecycle.confirm_session_expired'), 401);
        return;
      }
      const message = err instanceof Error ? err.message : t('api.plugins.confirm_lifecycle_action');
      onFailed(message, err instanceof ApiClientError ? err.status : undefined);
    } finally {
      setSubmitting(null);
    }
  };

  return (
    <AdminAlert
      tone={isPurge ? 'error' : 'warning'}
      role="alertdialog"
      aria-live="assertive"
      className="space-y-3"
      data-testid="plugin-lifecycle-confirmation"
      data-confirmation-operation={confirmation.operation}
    >
      <div className="space-y-1">
        <p className="text-sm font-semibold">
          {isPurge
            ? t('plugin_lifecycle.confirm_purge_title')
            : t('plugin_lifecycle.confirm_public_restore_title')}
        </p>
        <p className="text-xs leading-5">
          {isPurge
            ? t('plugin_lifecycle.confirm_purge_warning')
            : t('plugin_lifecycle.confirm_public_restore_warning')}
        </p>
      </div>

      <dl className="grid grid-cols-1 gap-x-4 gap-y-1 rounded-md border border-current/15 bg-white/80 p-2.5 text-xs text-zinc-800 sm:grid-cols-2">
        <div>
          <dt className="font-semibold text-zinc-500">{t('plugin_lifecycle.field_plugin_action')}</dt>
          <dd className="font-mono break-all">{pluginId} / {actionId}</dd>
        </div>
        <div>
          <dt className="font-semibold text-zinc-500">{t('plugin_lifecycle.field_target')}</dt>
          <dd className="break-words">{confirmation.title} (#{confirmation.entry_id})</dd>
        </div>
        <div>
          <dt className="font-semibold text-zinc-500">{t('plugin_lifecycle.field_type_locale')}</dt>
          <dd className="font-mono">{confirmation.type_key} · {confirmation.locale} · v{confirmation.version}</dd>
        </div>
        <div>
          <dt className="font-semibold text-zinc-500">{t('plugin_lifecycle.field_slug')}</dt>
          <dd className="font-mono break-all">{confirmation.slug}</dd>
        </div>
        <div className="sm:col-span-2">
          <dt className="font-semibold text-zinc-500">{t('plugin_lifecycle.field_expires_at')}</dt>
          <dd className="font-mono">{confirmation.expires_at}</dd>
        </div>
      </dl>

      <div className="flex flex-wrap items-center gap-2">
        <AdminButton
          type="button"
          variant={isPurge ? 'danger' : 'primary'}
          size="sm"
          disabled={!csrfToken || submitting !== null}
          data-testid="plugin-lifecycle-confirm-btn"
          onClick={() => void submitDecision('confirm')}
        >
          {submitting === 'confirm'
            ? t('plugin_lifecycle.confirming')
            : isPurge
              ? t('plugin_lifecycle.confirm_purge_button')
              : t('plugin_lifecycle.confirm_restore_button')}
        </AdminButton>
        <AdminButton
          type="button"
          variant="secondary"
          size="sm"
          disabled={!csrfToken || submitting !== null}
          data-testid="plugin-lifecycle-cancel-btn"
          onClick={() => void submitDecision('cancel')}
        >
          {submitting === 'cancel' ? t('plugin_lifecycle.cancelling') : t('multilingual.cancel')}
        </AdminButton>
      </div>
    </AdminAlert>
  );
};

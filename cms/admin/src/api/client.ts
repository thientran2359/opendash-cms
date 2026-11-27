import { __ } from '../i18n/index.tsx';
import type { PluginLifecycleConfirmation } from '../types.ts';
import type { MissingRefsReport } from './revisions.ts';

export interface PermalinkConflictItem {
  path: string;
  new_target: string;
  existing_target: string;
}

export class ApiClientError extends Error {
  status: number;
  readonly conflicts?: PermalinkConflictItem[];
  readonly errorCode?: string;
  readonly bodyCode?: string;
  readonly confirmation?: PluginLifecycleConfirmation;
  readonly missingRefs?: MissingRefsReport;
  readonly fieldErrors?: Record<string, string>;

  constructor(
    status: number,
    message: string,
    conflicts?: PermalinkConflictItem[],
    errorCode?: string,
    confirmation?: PluginLifecycleConfirmation,
    bodyCode?: string,
    missingRefs?: MissingRefsReport,
    fieldErrors?: Record<string, string>,
  ) {
    super(message);
    this.name = 'ApiClientError';
    this.status = status;
    this.conflicts = conflicts;
    this.errorCode = errorCode;
    this.bodyCode = bodyCode;
    this.confirmation = confirmation;
    this.missingRefs = missingRefs;
    this.fieldErrors = fieldErrors;
  }
}

export interface ApiRequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  csrfToken?: string | null;
  contentType?: string;
  body?: BodyInit | null;
  signal?: AbortSignal;
  keepalive?: boolean;
  /** M3 optimistic edit version, serialized as If-Match: "vN". */
  ifMatchVersion?: number;
  /** Explicit If-Match header value (e.g. "gv1-v1" for media lifecycle). */
  ifMatch?: string;
  /** Encoded display name for raw binary media uploads (never a server path). */
  uploadFilename?: string;
  /** Explicit irreversible intent header (`X-Confirm-Purge: true`) for Admin Purge only. */
  confirmPurge?: boolean;
  /** Enable typed host confirmation parsing only on designated authenticated plugin action endpoints. */
  expectLifecycleConfirmation?: boolean;
  defaultErrorMessage: string;
  oversizeErrorMessage?: string;
}

function buildRequestHeaders(
  csrfToken?: string | null,
  contentType?: string,
  ifMatchVersion?: number,
  uploadFilename?: string,
  ifMatch?: string,
  confirmPurge?: boolean,
): Record<string, string> {
  const headers: Record<string, string> = {};
  if (contentType) {
    headers['Content-Type'] = contentType;
  }
  if (csrfToken) {
    headers['X-CSRF-Token'] = csrfToken;
  }
  if (ifMatch !== undefined) {
    headers['If-Match'] = ifMatch;
  } else if (ifMatchVersion !== undefined) {
    headers['If-Match'] = `"v${ifMatchVersion}"`;
  }
  if (uploadFilename) {
    headers['X-File-Name'] = encodeURIComponent(uploadFilename);
  }
  if (confirmPurge === true) {
    headers['X-Confirm-Purge'] = 'true';
  }
  return headers;
}

function parseTrustedLifecycleConfirmation(
  res: Response,
  errObj: unknown,
  expectLifecycleConfirmation?: boolean,
): PluginLifecycleConfirmation | undefined {
  if (!expectLifecycleConfirmation || res.status !== 409 || !errObj || typeof errObj !== 'object' || Array.isArray(errObj)) {
    return undefined;
  }
  const envelope = errObj as Record<string, unknown>;
  if (envelope.code !== 'confirmation_required') return undefined;
  const raw = envelope.confirmation;
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return undefined;
  const c = raw as Record<string, unknown>;
  if (
    typeof c.confirmation_id !== 'string' || !c.confirmation_id.trim() || c.confirmation_id.length > 128 ||
    (c.operation !== 'purge' && c.operation !== 'restore') ||
    typeof c.type_key !== 'string' || !c.type_key.trim() ||
    typeof c.entry_id !== 'number' || !Number.isSafeInteger(c.entry_id) || c.entry_id <= 0 ||
    typeof c.locale !== 'string' || !c.locale.trim() ||
    typeof c.title !== 'string' ||
    typeof c.slug !== 'string' || !c.slug.trim() ||
    typeof c.version !== 'number' || !Number.isSafeInteger(c.version) || c.version <= 0 ||
    typeof c.expires_at !== 'string' || !c.expires_at.trim()
  ) {
    return undefined;
  }
  return {
    confirmation_id: c.confirmation_id,
    operation: c.operation,
    type_key: c.type_key,
    entry_id: c.entry_id,
    locale: c.locale,
    title: c.title,
    slug: c.slug,
    version: c.version,
    expires_at: c.expires_at,
  };
}

function responseError(
  res: Response,
  text: string,
  defaultMessage: string,
  oversizeMessage?: string,
  expectLifecycleConfirmation?: boolean,
): ApiClientError {
  let msg = text;
  let conflicts: PermalinkConflictItem[] | undefined;
  let confirmation: PluginLifecycleConfirmation | undefined;
  let bodyCode: string | undefined;
  let missingRefs: MissingRefsReport | undefined;
  let fieldErrors: Record<string, string> | undefined;
  try {
    const errObj = JSON.parse(text);
    if (errObj && typeof errObj.message === 'string') msg = errObj.message;
    else if (errObj && typeof errObj.error === 'string') msg = errObj.error;
    if (errObj && typeof errObj.code === 'string' && errObj.code.trim()) {
      bodyCode = errObj.code;
    }
    if (res.status === 409 && Array.isArray(errObj?.conflicts)) {
      conflicts = errObj.conflicts.filter((item: unknown): item is PermalinkConflictItem => {
        if (!item || typeof item !== 'object') return false;
        const row = item as Record<string, unknown>;
        return typeof row.path === 'string' && typeof row.new_target === 'string'
          && typeof row.existing_target === 'string';
      }).slice(0, 20);
    }
    if (
      res.status === 409 &&
      errObj?.code === 'revision_missing_refs' &&
      errObj.missing_refs &&
      typeof errObj.missing_refs === 'object' &&
      !Array.isArray(errObj.missing_refs)
    ) {
      missingRefs = errObj.missing_refs as MissingRefsReport;
    }
    if (
      res.status === 422 &&
      errObj?.field_errors &&
      typeof errObj.field_errors === 'object' &&
      !Array.isArray(errObj.field_errors)
    ) {
      const entries = Object.entries(errObj.field_errors as Record<string, unknown>)
        .filter((pair): pair is [string, string] => typeof pair[1] === 'string');
      if (entries.length > 0) {
        fieldErrors = Object.fromEntries(entries);
      }
    }
    confirmation = parseTrustedLifecycleConfirmation(res, errObj, expectLifecycleConfirmation);
  } catch {
    // Plain text error response.
  }
  if (res.status === 413 && oversizeMessage) msg = oversizeMessage;
  return new ApiClientError(
    res.status,
    msg || defaultMessage,
    conflicts,
    res.headers.get('X-OpenDash-Error-Code') ?? bodyCode,
    confirmation,
    bodyCode,
    missingRefs,
    fieldErrors,
  );
}

export async function handleResponseText(
  res: Response,
  defaultMessage: string,
  expectLifecycleConfirmation?: boolean,
): Promise<string> {
  const text = await res.text();
  if (!res.ok) {
    throw responseError(res, text, defaultMessage, undefined, expectLifecycleConfirmation);
  }
  return text;
}

export async function requestText(url: string, options: ApiRequestOptions): Promise<string> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(
      options.csrfToken,
      options.contentType,
      options.ifMatchVersion,
      options.uploadFilename,
      options.ifMatch,
      options.confirmPurge,
    ),
    body: options.body,
    signal: options.signal,
    keepalive: options.keepalive,
  });
  return handleResponseText(res, options.defaultErrorMessage, options.expectLifecycleConfirmation);
}

/** Authenticated binary GET for private Admin media. Keep HTTP and session
 * handling in this shared client, including AbortSignal and error statuses. */
export async function requestBlob(url: string, options: ApiRequestOptions): Promise<Blob> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(
      options.csrfToken,
      options.contentType,
      options.ifMatchVersion,
      options.uploadFilename,
      options.ifMatch,
      options.confirmPurge,
    ),
    body: options.body,
    signal: options.signal,
    cache: 'no-store',
  });
  if (!res.ok) await handleResponseText(res, options.defaultErrorMessage, options.expectLifecycleConfirmation);
  return res.blob();
}

export async function requestJson<T>(url: string, options: ApiRequestOptions): Promise<T> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(
      options.csrfToken,
      options.contentType,
      options.ifMatchVersion,
      options.uploadFilename,
      options.ifMatch,
      options.confirmPurge,
    ),
    body: options.body,
    signal: options.signal,
    keepalive: options.keepalive,
  });

  const text = await res.text();
  if (!res.ok) {
    throw responseError(
      res,
      text,
      options.defaultErrorMessage,
      options.oversizeErrorMessage,
      options.expectLifecycleConfirmation,
    );
  }

  try {
    return JSON.parse(text) as T;
  } catch {
    throw new ApiClientError(res.status, __("The server response is not valid JSON."));
  }
}

export interface HealthResponse {
  status: string;
  database: string;
  scaffold: boolean;
  safe_mode?: boolean;
  dev_mode?: boolean;
  dev_theme?: string | null;
  dev_plugins?: string[];
}

export async function getHealth(signal?: AbortSignal): Promise<HealthResponse> {
  return requestJson<HealthResponse>('/api/health', {
    signal,
    defaultErrorMessage: __("Could not check server status."),
  });
}

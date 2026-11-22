export interface PermalinkConflictItem {
  path: string;
  new_target: string;
  existing_target: string;
}

export class ApiClientError extends Error {
  status: number;
  readonly conflicts?: PermalinkConflictItem[];

  constructor(status: number, message: string, conflicts?: PermalinkConflictItem[]) {
    super(message);
    this.name = 'ApiClientError';
    this.status = status;
    this.conflicts = conflicts;
  }
}

export interface ApiRequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  csrfToken?: string | null;
  contentType?: string;
  body?: BodyInit | null;
  signal?: AbortSignal;
  /** M3 optimistic edit version, serialized as If-Match: "vN". */
  ifMatchVersion?: number;
  /** Explicit If-Match header value (e.g. "gv1-v1" for media lifecycle). */
  ifMatch?: string;
  /** Encoded display name for raw binary media uploads (never a server path). */
  uploadFilename?: string;
  defaultErrorMessage: string;
  oversizeErrorMessage?: string;
}

function buildRequestHeaders(
  csrfToken?: string | null,
  contentType?: string,
  ifMatchVersion?: number,
  uploadFilename?: string,
  ifMatch?: string,
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
  return headers;
}

export async function handleResponseText(res: Response, defaultMessage: string): Promise<string> {
  const text = await res.text();
  if (!res.ok) {
    throw new ApiClientError(res.status, text || defaultMessage);
  }
  return text;
}

export async function requestText(url: string, options: ApiRequestOptions): Promise<string> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(options.csrfToken, options.contentType, options.ifMatchVersion, options.uploadFilename, options.ifMatch),
    body: options.body,
    signal: options.signal,
  });
  return handleResponseText(res, options.defaultErrorMessage);
}

/** Authenticated binary GET for private Admin media. Keep HTTP and session
 * handling in this shared client, including AbortSignal and error statuses. */
export async function requestBlob(url: string, options: ApiRequestOptions): Promise<Blob> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(options.csrfToken, options.contentType, options.ifMatchVersion, options.uploadFilename, options.ifMatch),
    body: options.body,
    signal: options.signal,
    cache: 'no-store',
  });
  if (!res.ok) await handleResponseText(res, options.defaultErrorMessage);
  return res.blob();
}

/** Keep the preview HTML plus server visibility diagnostic outside the sandbox. */
export async function requestBuilderHtml(
  url: string, options: ApiRequestOptions,
): Promise<{ html: string; visibility: 'rendered' | 'unsupported' }> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(options.csrfToken, options.contentType, options.ifMatchVersion, options.uploadFilename, options.ifMatch),
    body: options.body,
    signal: options.signal,
  });
  const html = await handleResponseText(res, options.defaultErrorMessage);
  const visibility = res.headers.get('X-OpenDash-Block-Visibility');
  return { html, visibility: visibility === 'unsupported' ? 'unsupported' : 'rendered' };
}

export async function requestJson<T>(url: string, options: ApiRequestOptions): Promise<T> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(options.csrfToken, options.contentType, options.ifMatchVersion, options.uploadFilename, options.ifMatch),
    body: options.body,
    signal: options.signal,
  });

  const text = await res.text();
  if (!res.ok) {
    let msg = text;
    let conflicts: PermalinkConflictItem[] | undefined;
    try {
      const errObj = JSON.parse(text);
      if (errObj && typeof errObj.error === 'string') {
        msg = errObj.error;
      }
      if (res.status === 409 && Array.isArray(errObj?.conflicts)) {
        // Only expose the known structured permalink conflict shape. Other
        // errors keep their existing message/status handling.
        conflicts = errObj.conflicts.filter((item: unknown): item is PermalinkConflictItem => {
          if (!item || typeof item !== 'object') return false;
          const row = item as Record<string, unknown>;
          return typeof row.path === 'string' && typeof row.new_target === 'string'
            && typeof row.existing_target === 'string';
        }).slice(0, 20);
      }
    } catch {
      // plain text error response
    }
    if (res.status === 413 && options.oversizeErrorMessage) {
      msg = options.oversizeErrorMessage;
    }
    throw new ApiClientError(res.status, msg || options.defaultErrorMessage, conflicts);
  }

  try {
    return JSON.parse(text) as T;
  } catch {
    throw new ApiClientError(res.status, 'Phản hồi từ máy chủ không phải là JSON hợp lệ.');
  }
}

export interface HealthResponse {
  status: string;
  database: string;
  scaffold: boolean;
}

export async function getHealth(signal?: AbortSignal): Promise<HealthResponse> {
  return requestJson<HealthResponse>('/api/health', {
    signal,
    defaultErrorMessage: 'Không kiểm tra được trạng thái máy chủ.',
  });
}

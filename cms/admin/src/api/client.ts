export class ApiClientError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'ApiClientError';
    this.status = status;
  }
}

export interface ApiRequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
  csrfToken?: string | null;
  contentType?: string;
  body?: BodyInit | null;
  signal?: AbortSignal;
  defaultErrorMessage: string;
  oversizeErrorMessage?: string;
}

function buildRequestHeaders(
  csrfToken?: string | null,
  contentType?: string,
): Record<string, string> {
  const headers: Record<string, string> = {};
  if (contentType) {
    headers['Content-Type'] = contentType;
  }
  if (csrfToken) {
    headers['X-CSRF-Token'] = csrfToken;
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
    headers: buildRequestHeaders(options.csrfToken, options.contentType),
    body: options.body,
    signal: options.signal,
  });
  return handleResponseText(res, options.defaultErrorMessage);
}

export async function requestJson<T>(url: string, options: ApiRequestOptions): Promise<T> {
  const res = await fetch(url, {
    method: options.method ?? 'GET',
    credentials: 'same-origin',
    headers: buildRequestHeaders(options.csrfToken, options.contentType),
    body: options.body,
    signal: options.signal,
  });

  const text = await res.text();
  if (!res.ok) {
    let msg = text;
    try {
      const errObj = JSON.parse(text);
      if (errObj && typeof errObj.error === 'string') {
        msg = errObj.error;
      }
    } catch {
      // plain text error response
    }
    if (res.status === 413 && options.oversizeErrorMessage) {
      msg = options.oversizeErrorMessage;
    }
    throw new ApiClientError(res.status, msg || options.defaultErrorMessage);
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

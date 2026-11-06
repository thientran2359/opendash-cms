export class ApiClientError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'ApiClientError';
    this.status = status;
  }
}

export async function handleResponseText(res: Response, defaultMessage: string): Promise<string> {
  const text = await res.text();
  if (!res.ok) {
    throw new ApiClientError(res.status, text || defaultMessage);
  }
  return text;
}

export interface HealthResponse {
  status: string;
  database: string;
  scaffold: boolean;
}

export async function getHealth(signal?: AbortSignal): Promise<HealthResponse> {
  const response = await fetch("/api/health", {
    credentials: "same-origin",
    signal,
  });

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }

  return (await response.json()) as HealthResponse;
}

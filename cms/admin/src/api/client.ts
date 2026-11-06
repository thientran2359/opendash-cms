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

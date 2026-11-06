import { useEffect, useState } from "react";
import { getHealth, type HealthResponse } from "../../api/client";

export function SystemStatus() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    getHealth(controller.signal)
      .then((result) => {
        setHealth(result);
        setError(null);
      })
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) {
          setError(reason instanceof Error ? reason.message : String(reason));
        }
      });
    return () => controller.abort();
  }, []);

  return (
    <section className="panel">
      <span className="eyebrow">Kết nối backend</span>
      <h2>Trạng thái hệ thống</h2>
      {health ? (
        <p className="status success">
          HTTP: {health.status} · SQLite: {health.database}
        </p>
      ) : error ? (
        <p className="status error">Không kết nối được: {error}</p>
      ) : (
        <p className="status">Đang kiểm tra /api/health…</p>
      )}
    </section>
  );
}

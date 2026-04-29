import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Worker, apiGet } from "../api";

export function Workers() {
  const [workers, setWorkers] = useState<Worker[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadWorkers() {
    setLoading(true);
    setError("");
    try {
      setWorkers(await apiGet<Worker[]>("/api/workers"));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadWorkers();
  }, []);

  return (
    <>
      <header className="page-header">
        <h2>Workers</h2>
        <button type="button" onClick={loadWorkers} title="Refresh workers">
          <RefreshCw size={15} />
          Refresh
        </button>
      </header>
      {error && <div className="notice error">{error}</div>}
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Worker</th>
              <th>Online</th>
              <th>Labels</th>
              <th>Capacity</th>
              <th>Active</th>
            </tr>
          </thead>
          <tbody>
            {workers.length === 0 ? (
              <tr>
                <td colSpan={5}>
                  {loading ? "Loading..." : "No workers have reported heartbeat."}
                </td>
              </tr>
            ) : (
              workers.map((worker) => (
                <tr key={worker.worker_id}>
                  <td>{worker.worker_id}</td>
                  <td>
                    <span className={worker.online ? "badge ok" : "badge"}>
                      {worker.online ? "online" : "offline"}
                    </span>
                  </td>
                  <td>{formatLabels(worker.labels)}</td>
                  <td>{worker.capacity}</td>
                  <td>{worker.active_count}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </section>
    </>
  );
}

function formatLabels(labels: Record<string, string>) {
  const entries = Object.entries(labels);
  if (entries.length === 0) {
    return "-";
  }
  return entries.map(([key, value]) => `${key}=${value}`).join(", ");
}

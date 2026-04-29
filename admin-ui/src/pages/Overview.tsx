import { useEffect, useMemo, useState } from "react";
import { Execution, Worker, apiGet } from "../api";

export function Overview() {
  const [workers, setWorkers] = useState<Worker[]>([]);
  const [executions, setExecutions] = useState<Execution[]>([]);
  const [healthy, setHealthy] = useState("Unknown");
  const [error, setError] = useState("");

  async function loadOverview() {
    setError("");
    try {
      await apiGet<{ status: string }>("/api/health");
      const [loadedWorkers, loadedExecutions] = await Promise.all([
        apiGet<Worker[]>("/api/workers"),
        apiGet<Execution[]>("/api/executions"),
      ]);
      setHealthy("OK");
      setWorkers(loadedWorkers);
      setExecutions(loadedExecutions);
    } catch (err) {
      setHealthy("Error");
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    void loadOverview();
  }, []);

  const running = executions.filter((execution) => execution.status === "running").length;
  const failed = executions.filter((execution) => execution.status === "failed").length;
  const recentFailures = useMemo(
    () => executions.filter((execution) => execution.status === "failed").slice(0, 5),
    [executions],
  );

  return (
    <>
      <header className="page-header">
        <h2>Overview</h2>
      </header>
      {error && <div className="notice error">{error}</div>}
      <section className="metrics">
        <div>
          Scheduler health
          <br />
          <strong>{healthy}</strong>
        </div>
        <div>
          Live workers
          <br />
          <strong>{workers.length}</strong>
        </div>
        <div>
          Running
          <br />
          <strong>{running}</strong>
        </div>
        <div>
          Failed
          <br />
          <strong>{failed}</strong>
        </div>
      </section>
      <section className="panel">
        <h3>Recent failures</h3>
        <table>
          <thead>
            <tr>
              <th>Execution</th>
              <th>Status</th>
              <th>Worker</th>
              <th>Created</th>
            </tr>
          </thead>
          <tbody>
            {recentFailures.length === 0 ? (
              <tr>
                <td colSpan={4}>No failures reported.</td>
              </tr>
            ) : (
              recentFailures.map((execution) => (
                <tr key={execution.id}>
                  <td className="mono">{execution.id.slice(0, 8)}</td>
                  <td>
                    <span className="badge failed">{execution.status}</span>
                  </td>
                  <td>{execution.selected_worker_id ?? "-"}</td>
                  <td>{new Date(execution.created_at).toLocaleString()}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </section>
    </>
  );
}

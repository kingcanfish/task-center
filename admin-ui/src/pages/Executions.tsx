import { RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Execution, apiGet } from "../api";

export function Executions() {
  const [executions, setExecutions] = useState<Execution[]>([]);
  const [status, setStatus] = useState("all");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadExecutions() {
    setLoading(true);
    setError("");
    try {
      setExecutions(await apiGet<Execution[]>("/api/executions"));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadExecutions();
  }, []);

  const visibleExecutions = useMemo(
    () =>
      status === "all"
        ? executions
        : executions.filter((execution) => execution.status === status),
    [executions, status],
  );

  return (
    <>
      <header className="page-header">
        <h2>Executions</h2>
        <button type="button" onClick={loadExecutions} title="Refresh executions">
          <RefreshCw size={15} />
          Refresh
        </button>
      </header>
      {error && <div className="notice error">{error}</div>}
      <section className="toolbar">
        <label>
          Status
          <select value={status} onChange={(event) => setStatus(event.target.value)}>
            <option value="all">All</option>
            <option value="running">Running</option>
            <option value="failed">Failed</option>
            <option value="succeeded">Succeeded</option>
            <option value="scheduled">Scheduled</option>
            <option value="queued">Queued</option>
          </select>
        </label>
      </section>
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Execution</th>
              <th>Job</th>
              <th>Status</th>
              <th>Attempt</th>
              <th>Worker</th>
              <th>Created</th>
            </tr>
          </thead>
          <tbody>
            {visibleExecutions.length === 0 ? (
              <tr>
                <td colSpan={6}>{loading ? "Loading..." : "No executions found."}</td>
              </tr>
            ) : (
              visibleExecutions.map((execution) => (
                <tr key={execution.id}>
                  <td className="mono">{shortId(execution.id)}</td>
                  <td className="mono">{shortId(execution.job_id)}</td>
                  <td>
                    <span className={`badge ${execution.status}`}>{execution.status}</span>
                  </td>
                  <td>{execution.attempt_count}</td>
                  <td>{execution.selected_worker_id ?? "-"}</td>
                  <td>{formatTime(execution.created_at)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </section>
    </>
  );
}

function shortId(value: string) {
  return value.slice(0, 8);
}

function formatTime(value: string) {
  return new Date(value).toLocaleString();
}

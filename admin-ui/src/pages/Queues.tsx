import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Queue, apiGet } from "../api";

export function Queues() {
  const [queues, setQueues] = useState<Queue[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function loadQueues() {
    setLoading(true);
    setError("");
    try {
      setQueues(await apiGet<Queue[]>("/api/queues"));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void loadQueues();
  }, []);

  return (
    <>
      <header className="page-header">
        <h2>Queues</h2>
        <button type="button" onClick={loadQueues} title="Refresh queues">
          <RefreshCw size={15} />
          Refresh
        </button>
      </header>
      {error && <div className="notice error">{error}</div>}
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Queue</th>
              <th>Ready</th>
            </tr>
          </thead>
          <tbody>
            {queues.length === 0 ? (
              <tr>
                <td colSpan={2}>{loading ? "Loading..." : "No queue metrics found."}</td>
              </tr>
            ) : (
              queues.map((queue) => (
                <tr key={queue.name}>
                  <td>{queue.name}</td>
                  <td>{queue.depth}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </section>
    </>
  );
}

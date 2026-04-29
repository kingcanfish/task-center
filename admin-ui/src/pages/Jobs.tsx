import { Plus, RefreshCw, Save } from "lucide-react";
import { FormEvent, useEffect, useState } from "react";
import { Job, apiGet, apiPost } from "../api";

const defaultConfig = JSON.stringify(
  { method: "GET", url: "https://example.com" },
  null,
  2,
);

export function Jobs() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [taskType, setTaskType] = useState<Job["task_type"]>("http");
  const [cronExpr, setCronExpr] = useState("0 0 * * * *");
  const [labelSelector, setLabelSelector] = useState("executor=http");
  const [configJson, setConfigJson] = useState(defaultConfig);

  async function loadJobs() {
    setLoading(true);
    setError("");
    try {
      setJobs(await apiGet<Job[]>("/api/jobs"));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function createJob(event: FormEvent) {
    event.preventDefault();
    setError("");
    try {
      await apiPost<Job>("/api/jobs", {
        name,
        task_type: taskType,
        config_json: JSON.parse(configJson),
        cron_expr: cronExpr,
        label_selector: labelSelector,
      });
      setName("");
      setShowCreate(false);
      await loadJobs();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  useEffect(() => {
    void loadJobs();
  }, []);

  return (
    <>
      <header className="page-header">
        <h2>Jobs</h2>
        <div className="button-row">
          <button type="button" onClick={loadJobs} title="Refresh jobs">
            <RefreshCw size={15} />
            Refresh
          </button>
          <button
            type="button"
            onClick={() => setShowCreate((value) => !value)}
            title="Create job"
          >
            <Plus size={15} />
            Create
          </button>
        </div>
      </header>
      {error && <div className="notice error">{error}</div>}
      {showCreate && (
        <form className="panel form-grid" onSubmit={createJob}>
          <label>
            Name
            <input value={name} onChange={(event) => setName(event.target.value)} required />
          </label>
          <label>
            Type
            <select
              value={taskType}
              onChange={(event) => setTaskType(event.target.value as Job["task_type"])}
            >
              <option value="http">http</option>
              <option value="shell">shell</option>
              <option value="builtin">builtin</option>
            </select>
          </label>
          <label>
            Cron
            <input
              value={cronExpr}
              onChange={(event) => setCronExpr(event.target.value)}
              required
            />
          </label>
          <label>
            Labels
            <input
              value={labelSelector}
              onChange={(event) => setLabelSelector(event.target.value)}
            />
          </label>
          <label className="span-2">
            Config JSON
            <textarea value={configJson} onChange={(event) => setConfigJson(event.target.value)} />
          </label>
          <div className="form-actions">
            <button type="submit" title="Save job">
              <Save size={15} />
              Save
            </button>
          </div>
        </form>
      )}
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Cron</th>
              <th>Route</th>
              <th>Next Fire</th>
              <th>State</th>
            </tr>
          </thead>
          <tbody>
            {jobs.length === 0 ? (
              <tr>
                <td colSpan={6}>{loading ? "Loading..." : "No jobs configured."}</td>
              </tr>
            ) : (
              jobs.map((job) => (
                <tr key={job.id}>
                  <td>{job.name}</td>
                  <td>{job.task_type}</td>
                  <td>{job.cron_expr}</td>
                  <td>{job.label_selector || "shared"}</td>
                  <td>{formatTime(job.next_fire_at)}</td>
                  <td>
                    <span className={job.enabled && !job.paused ? "badge ok" : "badge"}>
                      {job.enabled && !job.paused ? "active" : "paused"}
                    </span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </section>
    </>
  );
}

function formatTime(value: string) {
  return new Date(value).toLocaleString();
}

export async function apiGet<T>(path: string): Promise<T> {
  const token = localStorage.getItem("task-center-token") ?? "";
  const response = await fetch(path, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

export async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const token = localStorage.getItem("task-center-token") ?? "";
  const response = await fetch(path, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

export type Job = {
  id: string;
  name: string;
  task_type: "http" | "shell" | "builtin";
  enabled: boolean;
  paused: boolean;
  cron_expr: string;
  timezone: string;
  next_fire_at: string;
  label_selector: string;
};

export type Execution = {
  id: string;
  job_id: string;
  scheduled_at: string;
  status: string;
  attempt_count: number;
  selected_worker_id: string | null;
  created_at: string;
};

export type Worker = {
  worker_id: string;
  online: boolean;
  labels: Record<string, string>;
  capacity: number;
  active_count: number;
};

export type Queue = {
  name: string;
  depth: number;
};

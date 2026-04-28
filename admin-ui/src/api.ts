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

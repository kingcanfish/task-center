import { Save } from "lucide-react";
import { FormEvent, useState } from "react";

export function SettingsPage() {
  const [token, setToken] = useState(localStorage.getItem("task-center-token") ?? "");

  function saveToken(event: FormEvent) {
    event.preventDefault();
    localStorage.setItem("task-center-token", token);
  }

  return (
    <>
      <header className="page-header">
        <h2>Settings</h2>
      </header>
      <section className="panel settings-grid">
        <form onSubmit={saveToken}>
          <span>Access token</span>
          <input
            value={token}
            onChange={(event) => setToken(event.target.value)}
            type="password"
            autoComplete="current-password"
          />
          <button type="submit" title="Save token">
            <Save size={15} />
            Save
          </button>
        </form>
        <div>
          <span>Shell executor</span>
          <strong>Disabled by default</strong>
        </div>
        <div>
          <span>Notifications</span>
          <strong>Telegram compatible</strong>
        </div>
      </section>
    </>
  );
}

export function SettingsPage() {
  return (
    <>
      <header className="page-header">
        <h2>Settings</h2>
      </header>
      <section className="panel settings-grid">
        <div>
          <span>Access token</span>
          <strong>Stored in browser local storage</strong>
        </div>
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

export function Queues() {
  return (
    <>
      <header className="page-header">
        <h2>Queues</h2>
      </header>
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Queue</th>
              <th>Ready</th>
              <th>Leased</th>
              <th>Retries</th>
              <th>Oldest</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td colSpan={5}>Queue metrics are not loaded yet.</td>
            </tr>
          </tbody>
        </table>
      </section>
    </>
  );
}

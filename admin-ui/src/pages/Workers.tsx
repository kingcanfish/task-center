export function Workers() {
  return (
    <>
      <header className="page-header">
        <h2>Workers</h2>
      </header>
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
            <tr>
              <td colSpan={5}>No workers have reported heartbeat.</td>
            </tr>
          </tbody>
        </table>
      </section>
    </>
  );
}

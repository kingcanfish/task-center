export function Jobs() {
  return (
    <>
      <header className="page-header">
        <h2>Jobs</h2>
        <button type="button">Create</button>
      </header>
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Cron</th>
              <th>Route</th>
              <th>State</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td colSpan={5}>No jobs configured.</td>
            </tr>
          </tbody>
        </table>
      </section>
    </>
  );
}

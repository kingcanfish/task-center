export function Overview() {
  return (
    <>
      <header className="page-header">
        <h2>Overview</h2>
      </header>
      <section className="metrics">
        <div>
          Scheduler health
          <br />
          <strong>Unknown</strong>
        </div>
        <div>
          Live workers
          <br />
          <strong>0</strong>
        </div>
        <div>
          Running
          <br />
          <strong>0</strong>
        </div>
        <div>
          Failed
          <br />
          <strong>0</strong>
        </div>
      </section>
      <section className="panel">
        <h3>Recent failures</h3>
        <table>
          <thead>
            <tr>
              <th>Job</th>
              <th>Status</th>
              <th>Worker</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td colSpan={4}>No failures reported.</td>
            </tr>
          </tbody>
        </table>
      </section>
    </>
  );
}

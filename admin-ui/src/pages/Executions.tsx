export function Executions() {
  return (
    <>
      <header className="page-header">
        <h2>Executions</h2>
      </header>
      <section className="toolbar">
        <label>
          Status
          <select>
            <option>All</option>
            <option>Running</option>
            <option>Failed</option>
            <option>Succeeded</option>
          </select>
        </label>
      </section>
      <section className="panel">
        <table>
          <thead>
            <tr>
              <th>Execution</th>
              <th>Job</th>
              <th>Status</th>
              <th>Attempt</th>
              <th>Worker</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td colSpan={5}>No executions found.</td>
            </tr>
          </tbody>
        </table>
      </section>
    </>
  );
}

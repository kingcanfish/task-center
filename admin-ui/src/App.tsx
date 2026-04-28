import {
  Activity,
  Briefcase,
  Clock,
  HardDrive,
  ListTree,
  Settings,
} from "lucide-react";
import "./styles.css";
import { Overview } from "./pages/Overview";
import { Jobs } from "./pages/Jobs";
import { Executions } from "./pages/Executions";
import { Workers } from "./pages/Workers";
import { Queues } from "./pages/Queues";
import { SettingsPage } from "./pages/Settings";

const pages = [
  ["Overview", Activity],
  ["Jobs", Briefcase],
  ["Executions", Clock],
  ["Workers", HardDrive],
  ["Queues", ListTree],
  ["Settings", Settings],
] as const;

export function App() {
  const current = new URLSearchParams(location.search).get("page") ?? "Overview";

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Task Center</h1>
        <nav className="nav-list" aria-label="Admin sections">
          {pages.map(([name, Icon]) => (
            <a
              className={current === name ? "active nav-item" : "nav-item"}
              href={`?page=${name}`}
              key={name}
            >
              <Icon size={16} />
              <span>{name}</span>
            </a>
          ))}
        </nav>
      </aside>
      <main className="main">
        {current === "Overview" && <Overview />}
        {current === "Jobs" && <Jobs />}
        {current === "Executions" && <Executions />}
        {current === "Workers" && <Workers />}
        {current === "Queues" && <Queues />}
        {current === "Settings" && <SettingsPage />}
      </main>
    </div>
  );
}

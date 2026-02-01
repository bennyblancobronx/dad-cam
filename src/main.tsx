import React from "react";
import ReactDOM from "react-dom/client";
import { attachConsole } from "@tauri-apps/plugin-log";
import App from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";

// Forward frontend console.log/warn/error to the same log file as Rust logs.
// Best-effort: if the plugin is unavailable, console output still works normally.
attachConsole().catch(() => {});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);

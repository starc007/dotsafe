import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { followSystemTheme } from "./lib/theme";
import "./index.css";

async function start() {
  // In a plain browser during development, fake the Rust backend so the UI
  // can be worked on without the native app. Stripped from production builds.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }

  followSystemTheme();
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

start();

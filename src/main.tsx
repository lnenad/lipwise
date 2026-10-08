import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { openUrl } from "@tauri-apps/plugin-opener";
import "@fontsource-variable/inter/opsz.css";
import "./styles.css";
import "./theme";

// On macOS the content runs under a transparent title bar; leave room for the traffic lights.
if (navigator.userAgent.includes("Mac")) document.documentElement.classList.add("mac");
// On Windows the native title bar spans the window; styles.css joins it with the sidebar.
if (navigator.userAgent.includes("Windows")) document.documentElement.classList.add("windows");

// Open web links in the user's browser instead of inside the app window.
document.addEventListener("click", (e) => {
  const link = (e.target as HTMLElement).closest("a");
  if (link?.href.startsWith("http") && !link.href.startsWith(location.origin)) {
    e.preventDefault();
    openUrl(link.href);
  }
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

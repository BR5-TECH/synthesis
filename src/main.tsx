import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsWindowApp } from "./SettingsWindowApp";
import { readSettingsWindowBoot } from "./settingsWindow";
import { installDropGuard } from "./dropGuard";

/**
 * Two windows, one entry point (`../specifications/ui/SWN-settings-windows.md`
 * SWN-FR-01).
 *
 * The main window — the Project picker and the shell in its two states — and the
 * Settings child window both boot this file. Which one a webview is comes from
 * the query string the backend built it with, so the routing is a read of the
 * location rather than anything the application has to be asked.
 */
const boot = readSettingsWindowBoot(window.location.search);

// SNV-FR-UKER: the main window hands every drag to its web content
// (SNV-FR-RQLF), so a drop no surface takes must not load the dropped file.
if (!boot) installDropGuard(window);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {boot ? (
      <SettingsWindowApp initialSection={boot.section} />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);

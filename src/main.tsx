import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsWindowApp } from "./SettingsWindowApp";
import { readSettingsWindowBoot, readSettingsWindowProject } from "./settingsWindow";
import { setWindowProjectId } from "./api/invoke";
import { readProjectWindowBoot } from "./projectWindowBoot";
import { markPlatform } from "./platform";
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

// PJT-FR-PBQT: the top chrome meets the system-drawn bar of macOS.
markPlatform(document.documentElement, navigator.platform);

// SNV-FR-UKER: the main window hands every drag to its web content
// (SNV-FR-RQLF), so a drop no surface takes must not load the dropped file.
if (!boot) installDropGuard(window);

// PWN-FR-VIYR: the Settings window serves the project of the window that opened
// it, so its calls carry that project's id.
if (boot) setWindowProjectId(readSettingsWindowProject(window.location.search));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {boot ? (
      <SettingsWindowApp initialSection={boot.section} />
    ) : (
      <App projectWindowBoot={readProjectWindowBoot(window.location.search)} />
    )}
  </React.StrictMode>,
);

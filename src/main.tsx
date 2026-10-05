import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsWindowApp } from "./SettingsWindowApp";
import { readSettingsWindowBoot } from "./settingsWindow";

/**
 * Three windows, one entry point (`../specifications/ui/SWN-settings-windows.md`
 * SWN-FR-01).
 *
 * The main window — the Project picker and the shell in its two states — and the
 * two settings child windows all boot this file. Which one a webview is comes
 * from the query string the backend built it with, so the routing is a read of
 * the location rather than anything the application has to be asked.
 */
const boot = readSettingsWindowBoot(window.location.search);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {boot ? (
      <SettingsWindowApp kind={boot.kind} initialSection={boot.section} />
    ) : (
      <App />
    )}
  </React.StrictMode>,
);

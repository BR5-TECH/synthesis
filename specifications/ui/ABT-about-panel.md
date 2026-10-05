# About panel

**Spec code:** `ABT`

## Intent
The About panel tells the user what Synthesis is and where its project lives. The user opens it from the application menu at any time, before or after a project is open. It is informational only: it shows one description and one link, and it holds no setting and no state.

## Functional requirements
1. **ABT-FR-KMVD** The application menu offers an **About** entry whenever the application runs: in the Project picker and in the main window. Activating it opens the About panel in the window that is showing. Where the entry sits in each menu is owned by `SNV-shell-navigation.md` SNV-FR-23 and `PPK-project-picker.md` PPK-FR-15.
2. **ABT-FR-QZHW** The About panel is a centred modal overlay drawn inside the window that opened it. It is not a native child window, a tab, or a separate page. While it is open, the owning window accepts no pointer or keyboard interaction outside the panel. Closing it returns the user to the window unchanged.
3. **ABT-FR-TNRB** The panel shows the exact description **AI-powered IDE for spec-driven development.** and one link to the Synthesis GitHub project. The panel shows nothing else of substance: no version, no license, no editable control.
4. **ABT-FR-WPLJ** The link points to `https://github.com/BR5-TECH/synthesis`. Activating it opens that address in the operating system's default browser. The address never loads inside Synthesis. The panel stays open after the activation.
5. **ABT-FR-DXGC** The panel has a visible **Close** control. Activating the control or pressing Escape closes the panel. Each action leaves the owning window exactly as it was before the panel opened.
6. **ABT-FR-HYFE** When the panel opens, keyboard focus moves into it. While the panel is open, Tab and Shift+Tab cycle focus within the panel only. When the panel closes, focus returns to the element that held focus when the About entry was activated.
   - *Why:* A native menu item cannot hold keyboard focus in the page, so the page element focused before the menu was used is the nearest place to return to.
7. **ABT-FR-RUSV** The panel exposes the role of a modal dialog with the accessible name **About Synthesis**. The link has the accessible name **Synthesis on GitHub**, which is also its visible text. The Close control has the accessible name **Close**.
8. **ABT-FR-NVQT** Activating the About entry while the panel is open opens no second panel and changes nothing. While a settings window is open over the owning window, the About entry has no effect (per `SWN-settings-windows.md` SWN-FR-02).
9. **ABT-FR-LBNA** The panel never outlives the surface it overlays. When the window it overlays is replaced — a project opens, or the project closes and the Project picker returns — the panel closes with it.
10. **ABT-FR-GCPK** In the main window, the panel is a floating overlay under the mutual-exclusion rule of `SNV-shell-navigation.md` SNV-FR-56. Opening the panel closes any other open overlay. Opening another overlay closes the panel.
11. **ABT-FR-ZEJM** In the Project picker, the panel overlays and blocks the picker. The picker keeps its recent list, its inputs, and its errors unchanged while the panel is open and after it closes (per `PPK-project-picker.md` PPK-FR-EWQH).
12. **ABT-FR-SDFA** Opening, using, and closing the panel makes no network read and writes no persisted state. The panel reads no setting. It has no state that survives its closing.

## User stories
- As a user who has just installed Synthesis, I want to open About from the application menu before I open a project so that I can see what the application is and where its project lives.
- As a user working in a project, I want to open the Synthesis GitHub project in my browser from About so that I can read its documentation or report a problem.

## Wireframes
```
┌──────────────────────────────────────────────┐
│  (owning window, blocked)                    │
│        ┌──────────────────────────┐          │
│        │ About Synthesis      [ ✕ ]│          │
│        │                          │          │
│        │ AI-powered IDE for       │          │
│        │ spec-driven development. │          │
│        │                          │          │
│        │ Synthesis on GitHub      │          │
│        └──────────────────────────┘          │
└──────────────────────────────────────────────┘
```
Layout notes:
- The panel is centred horizontally and vertically in the owning window.
- A scrim covers the rest of the owning window.
- The panel fits inside the fixed Project picker window without scrolling.

## UI contract boundary
- **Owned by the UI**: the panel, its content, its modal blocking, its keyboard behavior and focus handling, its accessible names, and the activation of its link.
- **Delegated to backend (abstract)**: none. The About menu entry relays its activation to the window as an event, as the File menu entries do (per `SNV-shell-navigation.md` SNV-FR-23). The panel introduces no backend command. The link opens through the platform opener that the main window already holds permission for.

## Non-functional requirements
- The panel opens in one frame, with no loading state.
- The panel needs no network connectivity to render.

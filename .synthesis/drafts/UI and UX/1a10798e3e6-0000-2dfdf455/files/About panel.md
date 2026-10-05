## Intent

Add an always-available About entry and information panel to Synthesis. Users can open it from the platform-native application menu before or after they open a project.

## Expected Menu Layout

Keep each platform's existing menu entries, labels, and order, including the native quit item. Add a separator and **About** immediately before that quit item:

- **macOS:** the app-name menu, which already carries Global settings and Project settings.
- **Windows and Linux:** the File menu, which keeps its existing entries and settings placement.

Show **About** both while the Project picker is open and while a project is open. In the Project picker, keep Project settings absent and Global settings available, as specified by `PPK-project-picker.md`.

## User story

- The user opens the application menu: the app-name menu on macOS, or the File menu on Windows and Linux.
- The user selects **About**.
- A centered in-app modal overlay opens over the current window and presents information about Synthesis.
- The user can open the Synthesis GitHub project in the operating system's default browser.

## Requirements

- The About panel is a centered modal overlay in the window that opened it. It is not a native child window, tab, or separate page. While it is open, the owning window is blocked; closing it returns the user to the unchanged window.
- The panel shows:
  - The exact description: **AI-powered IDE for spec-driven development.**
  - A link to <https://github.com/BR5-TECH/synthesis>. Activating it opens that address in the operating system's default browser, not inside Synthesis.
- Provide a visible close control and support Escape to dismiss the panel. Move keyboard focus into the panel when it opens, keep focus within it while it is open, and return focus to the About menu item when it closes. Give the dialog and link accessible names.
- In the main window, the About panel follows the floating-overlay mutual-exclusion rule in `SNV-shell-navigation.md`. In the Project picker, it overlays and blocks the picker, which remains unchanged when About closes.
- The panel is informational. It adds no editable settings, version or license details, network reads, or persistent state.
- Add or update UI tests for menu availability in the Project picker and main window, platform-specific menu placement, panel content and keyboard dismissal, and opening the repository link.

## Specifications to update

- Add `specifications/ui/ABT-about-panel.md` for the panel's content, modal behavior, and link.
- Update `specifications/ui/SNV-shell-navigation.md` for the main-window menu entry, overlay behavior, and navigation.
- Update `specifications/ui/PPK-project-picker.md` for the pre-project menu entry and modal behavior.
- Update `specifications/ui/OVW-overview.md` for the About surface inventory and routes to it.
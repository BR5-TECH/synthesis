/**
 * The control that invoked the Settings window, so focus can go back to it when
 * the window closes (`specifications/ui/SWN-settings-windows.md` SWN-FR-FCRT).
 *
 * The Settings window is a webview of its own, so the main window's document is
 * left exactly as it was while the window is open: the element that held focus
 * is still the document's active element when the window goes. What the main
 * window has to remember is which control to give the focus to, because a
 * control that was only clicked may not hold focus (a button click does not
 * focus it in every webview).
 *
 * Module state rather than React state: the control is noted at the moment of
 * the request, from an event handler, and spent at the moment of the close, from
 * an event listener. Neither is a render.
 */

let invoker: Element | null = null;

/**
 * Note the control the request came from. A request with no control noted — the
 * application menu's entry, or a route from a surface — leaves the choice to
 * `captureActiveInvoker`.
 */
export function noteSettingsInvoker(element: Element | null): void {
  invoker = element;
}

/**
 * Called as the window opens. A control already noted by the request stands;
 * otherwise the element that holds focus in this document is the one that
 * invoked the window.
 */
export function captureActiveInvoker(): void {
  if (invoker) return;
  const active = document.activeElement;
  invoker = active && active !== document.body ? active : null;
}

/**
 * Called as the window closes: give the focus back to the noted control, and
 * forget it. A control that has left the document in the meantime is skipped
 * rather than focused, since focusing a detached element does nothing.
 */
export function restoreSettingsInvoker(): void {
  const target = invoker;
  invoker = null;
  if (target instanceof HTMLElement && target.isConnected) target.focus();
}

/** Forget any noted control. Tests only. */
export function resetSettingsInvoker(): void {
  invoker = null;
}

/**
 * SNV-FR-56: what the shell closes when a dialog of the shared `Modal` opens
 * (`../../specifications/ui/SNV-shell-navigation.md` SNV-FR-56).
 *
 * The dialog opens from a tab or a panel, several layers below the shell, so
 * the shell registers its handler here, on the terms of the Export dialog and
 * the diagram editor. A window that registers nothing (a settings window)
 * closes nothing.
 */
let opening: (() => void) | null = null;

/** The shell sets what an opening dialog closes. `null` removes it. */
export function setModalOpening(next: (() => void) | null): void {
  opening = next;
}

/** A dialog opens: every other floating overlay of the window closes. */
export function announceModalOpening(): void {
  opening?.();
}

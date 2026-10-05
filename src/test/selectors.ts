/**
 * Test helpers for the toggle-button selector rows of `SNV-shell-navigation.md`
 * SNV-FR-62 — the Project panel's artifact-type lens, the Changes panel's mode
 * toggle and lens, the Notes panel's scope selector, and the Drafts panel's
 * status filter.
 *
 * A row is addressed by the same accessible name the panel gives it, so a test
 * names the control the way a user would rather than reaching for a class.
 */
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

/** The row itself, by its group label. */
export function selectorRow(label: string): HTMLElement {
  return screen.getByRole("radiogroup", { name: label });
}

/** The value of the row's active position, or null if it has none. */
export function selectorValue(label: string): string | null {
  const active = selectorRow(label).querySelector<HTMLElement>(
    '[role="radio"][aria-checked="true"]',
  );
  return active?.dataset.value ?? null;
}

/**
 * Every position the row is currently rendering, in order. Clipped positions
 * (SNV-FR-63) are absent from the DOM, so this is what the author can actually
 * reach — which is exactly what the presence rule of LIB-FR-19 is about.
 */
export function selectorValues(label: string): string[] {
  return [...selectorRow(label).querySelectorAll<HTMLElement>("[role=radio]")]
    .map((el) => el.dataset.value)
    .filter((v): v is string => v != null);
}

/** The button for one position, for asserting on its own state. */
export function selectorButton(label: string, value: string): HTMLElement {
  const btn = selectorRow(label).querySelector<HTMLElement>(
    `[role="radio"][data-value="${value}"]`,
  );
  if (!btn) {
    throw new Error(
      `selector "${label}" has no position "${value}" (has: ${selectorValues(label).join(", ")})`,
    );
  }
  return btn;
}

/** Activate a position, the way a click on its button does. */
export async function pickSelector(
  label: string,
  value: string,
): Promise<void> {
  await userEvent.click(selectorButton(label, value));
}

/** The accessible names the row's buttons carry, in order. */
export function selectorNames(label: string): string[] {
  return within(selectorRow(label))
    .getAllByRole("radio")
    .map((el) => el.getAttribute("aria-label") ?? "");
}

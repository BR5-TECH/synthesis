/**
 * Global settings → **Navigation** (GLS-FR-28, GLS-FR-OPNB).
 *
 * A switch: whether a change of the main viewport's active tab moves the
 * vertical panel's selection to the item that tab is a view onto (SNV-FR-64).
 * A selector: where a project opened from the project switcher opens
 * (`PJT-project-tabs.md` PJT-FR-RMBR).
 *
 * The section holds no dirty state and contributes nothing to the discard
 * confirmation of GLS-FR-13 — the switch persists at once, exactly as the theme
 * selector and the notifications switch do. It writes through
 * `patchAppPreferences`, which is what carries every other field of the record
 * through unchanged (GSS-FR-20).
 */
import { useCallback, useEffect, useState } from "react";

import {
  loadAppPreferences,
  patchAppPreferences,
} from "../state/appPreferences";
import { setSelectionFollowsTab } from "../state/selectionFollowsTab";
import { logWarn } from "../logging";
import type { OpenProjectBehaviour } from "../types";

/** GLS-FR-OPNB: the three choices, in their order. */
const OPEN_PROJECT_CHOICES: { value: OpenProjectBehaviour; label: string }[] = [
  { value: "ask", label: "Ask every time" },
  { value: "this_tab", label: "This tab" },
  { value: "new_tab", label: "New tab" },
];

export function NavigationSettings() {
  // Null until the stored value has landed, so the switch is never rendered
  // asserting a value nobody chose.
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [openIn, setOpenIn] = useState<OpenProjectBehaviour | null>(null);
  const [openInError, setOpenInError] = useState<string | null>(null);

  // GLS-FR-28: initialised from "load app preferences" on mount.
  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (cancelled) return;
      // GSS-FR-33: absent means never chosen, and the default is ON. Reading
      // `?? true` rather than `=== true` is what makes the behaviour opt-out for
      // an author mid-upgrade rather than presenting them as opted out.
      const value = prefs.selectionFollowsTab ?? true;
      setEnabled(value);
      // GSS-FR-OPBH: absent means never chosen, which asks.
      setOpenIn(prefs.openProjectBehaviour ?? "ask");
      // SNV-FR-64: the shell reads the gate at activation time, so it learns the
      // stored value here rather than at the next relaunch.
      setSelectionFollowsTab(value);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const onToggle = useCallback(async (next: boolean) => {
    setError(null);
    setEnabled(next); // optimistic; rolled back if the write fails
    // GLS-FR-28: in force from the next qualifying activation, with no relaunch.
    // Set before the write rather than after it, so a slow disk does not leave
    // the switch saying one thing while the shell still does the other.
    setSelectionFollowsTab(next);
    try {
      await patchAppPreferences({ selectionFollowsTab: next });
    } catch (e) {
      setEnabled(!next);
      setSelectionFollowsTab(!next);
      const message = e instanceof Error ? e.message : "could not save";
      setError(message);
      // A preference the author set and the application then silently dropped is
      // exactly the kind of thing that is only ever explained by a log. The
      // message is a filesystem error, so it carries a path at worst.
      logWarn(["frontend"], "selection-follows-tab preference not saved", {
        requested: next,
        error: message,
      });
    }
  }, []);

  // GLS-FR-OPNB: a change saves at once; the section holds no dirty state.
  const onOpenInChange = useCallback(
    async (next: OpenProjectBehaviour) => {
      const previous = openIn;
      setOpenInError(null);
      setOpenIn(next);
      try {
        await patchAppPreferences({ openProjectBehaviour: next });
      } catch (e) {
        setOpenIn(previous);
        const message = e instanceof Error ? e.message : "could not save";
        setOpenInError(message);
        logWarn(["frontend"], "open project behaviour not saved", {
          requested: next,
          error: message,
        });
      }
    },
    [openIn],
  );

  return (
    <div>
      <p className="t-p" style={{ marginBottom: 20 }}>
        Keep the side panel in step with what you are working on, so switching
        to a tab also shows you where its file or change lives.
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <input
          type="checkbox"
          checked={enabled ?? true}
          disabled={enabled === null}
          onChange={(e) => void onToggle(e.target.checked)}
        />
        <span>Selection follows tab</span>
      </label>

      <p className="t-ui-xs t-muted" style={{ marginTop: 12, maxWidth: 520 }}>
        When you switch to a different tab, the panel that holds its item becomes
        the active one and selects it: Project for a file or a Flow, Changes for a
        change, Commits for a diff. Other tabs leave the panel alone, and you can
        always select something else afterwards.
      </p>

      {error && (
        <p className="t-ui-xs" style={{ marginTop: 12, color: "var(--danger)" }} role="alert">
          {error}
        </p>
      )}

      <div style={{ display: "flex", alignItems: "center", gap: 10, marginTop: 24 }}>
        <span id="navigation-open-project-in">Open another project in</span>
        <select
          aria-labelledby="navigation-open-project-in"
          value={openIn ?? "ask"}
          disabled={openIn === null}
          onChange={(e) => void onOpenInChange(e.target.value as OpenProjectBehaviour)}
        >
          {OPEN_PROJECT_CHOICES.map((c) => (
            <option key={c.value} value={c.value}>
              {c.label}
            </option>
          ))}
        </select>
      </div>

      <p className="t-ui-xs t-muted" style={{ marginTop: 12, maxWidth: 520 }}>
        When you choose another project in the project switcher, it can ask where to
        open it, replace the project in this tab, or open it in a new tab. File → New
        Tab always opens a new tab.
      </p>

      {openInError && (
        <p className="t-ui-xs" style={{ marginTop: 12, color: "var(--danger)" }} role="alert">
          {openInError}
        </p>
      )}
    </div>
  );
}

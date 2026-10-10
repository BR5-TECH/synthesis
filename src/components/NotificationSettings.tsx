import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import {
  patchAppPreferences,
  loadAppPreferences,
} from "../state/appPreferences";
import { mintAddress } from "../state/notificationAddress";
import { emitSettingsRehearsalRequested } from "../events";
import { logInfo } from "../logging";
import {
  setNotificationPermissionGranted,
  setNotificationsEnabled,
} from "../state/notifications";
import type { NotificationPermissionState } from "../types";

/**
 * The Global settings **Notifications** section (`GLS-global-settings.md`
 * GLS-FR-25 / GLS-FR-26 / GLS-FR-27).
 *
 * Three things: the one switch that governs whether the application posts OS
 * notifications at all, a statement of what the operating system itself allows,
 * and the rehearsal that exercises the whole path end to end.
 *
 * Every action applies immediately through its own operation, so the section
 * holds no dirty state and never contributes to the window's save-before-close
 * sweep (GLS-FR-10, GLS-FR-13, SWN-FR-08).
 */

/**
 * GLS-FR-27: long enough for the author to switch away before it fires, short
 * enough that they have not forgotten they asked. Stated on the control so they
 * know how long they have.
 */
export const REHEARSAL_DELAY_MS = 5000;

/** The project the rehearsal addresses, so the arrival end is reachable. */
export interface RehearsalTarget {
  projectKey: string;
  worktree: string;
}

export function NotificationSettings({
  rehearsalTarget,
}: {
  /**
   * Null when no project is open. The rehearsal needs a real address to land
   * on. The Global settings window is reachable with no project open
   * (SWN-FR-15), which is exactly when this is null and the rehearsal is
   * greyed out (GLS-FR-27).
   */
  rehearsalTarget: RehearsalTarget | null;
}) {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [permission, setPermission] =
    useState<NotificationPermissionState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingRehearsal, setPendingRehearsal] = useState(false);

  // GLS-FR-25: initialised from "load app preferences" on mount. GLS-FR-26: and
  // the platform's disposition read alongside it, because a switch that is on
  // while the OS refuses would otherwise describe nothing the author observes.
  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (cancelled) return;
      // GSS-FR-32: absent means never chosen, and the default is ON. Reading
      // `?? true` rather than `=== true` is what keeps an upgrade from
      // presenting every existing install as opted out.
      const value = prefs.notificationsEnabled ?? true;
      setEnabled(value);
      // NTF-FR-12: the facility reads the gate at raise time, so it learns the
      // stored value here rather than at the next relaunch.
      setNotificationsEnabled(value);
    });
    void api
      .getNotificationPermission()
      .then((state) => {
        if (cancelled) return;
        setPermission(state);
        setNotificationPermissionGranted(state === "granted");
      })
      .catch(() => {
        // A platform that cannot answer is one with no centre to post into.
        if (cancelled) return;
        setPermission("unsupported");
        setNotificationPermissionGranted(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const granted = permission === "granted";

  const onToggle = useCallback(
    async (next: boolean) => {
      setError(null);
      setEnabled(next); // optimistic; rolled back if the write fails
      setNotificationsEnabled(next);
      try {
        await patchAppPreferences({ notificationsEnabled: next });
      } catch (e) {
        setEnabled(!next);
        setNotificationsEnabled(!next);
        setError(e instanceof Error ? e.message : "could not save");
        return;
      }
      // GLS-FR-26 / NTF-FR-13: turning the switch ON is the author asking for
      // notifications, which is one of exactly two places a permission prompt
      // may originate. Turning it off never asks for anything.
      if (next && permission === "not_requested") {
        try {
          const state = await api.requestNotificationPermission();
          setPermission(state);
          setNotificationPermissionGranted(state === "granted");
        } catch {
          setPermission("unsupported");
          setNotificationPermissionGranted(false);
        }
      }
    },
    [permission],
  );

  const onRequestPermission = useCallback(async () => {
    setError(null);
    try {
      const state = await api.requestNotificationPermission();
      setPermission(state);
      // GLS-FR-26, GLS-FR-27: granting here is what enables the rehearsal — and the
      // facility must know too, or the rehearsal the author immediately
      // presses would be suppressed for want of a permission they just gave.
      setNotificationPermissionGranted(state === "granted");
    } catch {
      setPermission("unsupported");
      setNotificationPermissionGranted(false);
    }
  }, []);

  // GLS-FR-27 / NTF-FR-25: the rehearsal goes through the ordinary facility on
  // exactly the terms every other raise does — same shape, same key, same
  // policy, same routing — so exercising it exercises the whole path rather
  // than a special case of it. In particular it is correctly *suppressed* when
  // the author stays on this very window (NTF-FR-08, NTF-FR-25), which is the
  // behaviour a dedicated test-only path would have hidden.
  //
  // The delay and the raise belong to the MAIN window rather than to this
  // section (SWN-FR-01): the whole point of the rehearsal is that the author
  // leaves before it fires, and closing this window is one of the ways they
  // may leave — a timer held here would go with the window and take the
  // notification with it (NTF-FR-25, NTF-FR-17, GLS-FR-27). So this asks, and the main window owns
  // the wait.
  const rehearsalTimer = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (rehearsalTimer.current !== null)
        window.clearTimeout(rehearsalTimer.current);
    },
    [],
  );

  const onRehearse = useCallback(() => {
    if (!rehearsalTarget) return;
    setPendingRehearsal(true);
    const address = mintAddress(
      rehearsalTarget.projectKey,
      rehearsalTarget.worktree,
      { kind: "settings", which: "global" },
    );
    void emitSettingsRehearsalRequested(address);
    logInfo(["frontend"], "notification rehearsal requested", {
      delayMs: REHEARSAL_DELAY_MS,
    });
    // Local only, so the control reads "Sending…" for as long as the author
    // has to leave. It governs nothing: the raise happens in the main window
    // whether or not this one is still here to re-enable its own button.
    rehearsalTimer.current = window.setTimeout(() => {
      rehearsalTimer.current = null;
      setPendingRehearsal(false);
    }, REHEARSAL_DELAY_MS);
  }, [rehearsalTarget]);

  const rehearsalDisabled =
    !enabled || !granted || !rehearsalTarget || pendingRehearsal;

  return (
    <div>
      <p className="t-p" style={{ marginBottom: 20 }}>
        Synthesis can tell you when something finishes or needs you while you
        are looking elsewhere. Notifications are shown by your operating system,
        and clicking one brings you back to where it happened.
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <input
          type="checkbox"
          checked={enabled ?? true}
          disabled={enabled === null || permission === "unsupported"}
          onChange={(e) => void onToggle(e.target.checked)}
        />
        <span>Enable notifications</span>
      </label>

      {/* GLS-FR-26: state what the operating system allows, and offer the
          request only while it is a thing that can succeed. */}
      <div style={{ marginTop: 16 }}>
        {permission === "not_requested" && (
          <>
            <p className="t-ui-xs t-muted" style={{ marginBottom: 8 }}>
              Your operating system has not been asked yet.
            </p>
            <button
              type="button"
              className="btn btn--default btn--sm"
              onClick={() => void onRequestPermission()}
            >
              Allow notifications
            </button>
          </>
        )}
        {permission === "denied" && (
          <p className="t-ui-xs t-muted">
            Notifications are turned off for Synthesis in your operating
            system's settings. Turn them back on there to receive them.
          </p>
        )}
        {permission === "unsupported" && (
          <p className="t-ui-xs t-muted">
            {/* GLS-FR-26: also a development run outside an application bundle. */}
            This build of Synthesis cannot post notifications: the system has no
            notification centre it can reach, or the application is not running
            from an application bundle.
          </p>
        )}
        {permission === "granted" && (
          <p className="t-ui-xs t-muted">
            Your operating system permits notifications from Synthesis.
          </p>
        )}
      </div>

      {/* GLS-FR-27: the rehearsal. */}
      <h3 className="t-eyebrow" style={{ margin: "28px 0 10px" }}>
        Try it
      </h3>
      <p className="t-p" style={{ marginBottom: 12 }}>
        Send yourself a notification in {Math.round(REHEARSAL_DELAY_MS / 1000)}{" "}
        seconds — enough time to switch to another window. Clicking it brings
        you back here.
      </p>
      <button
        type="button"
        className="btn btn--default btn--sm"
        disabled={rehearsalDisabled}
        onClick={onRehearse}
      >
        {pendingRehearsal
          ? "Sending…"
          : `Send a test notification in ${Math.round(REHEARSAL_DELAY_MS / 1000)}s`}
      </button>

      {error && (
        <span
          className="picker-error"
          style={{ display: "block", marginTop: 8 }}
        >
          ✗ {error}
        </span>
      )}
    </div>
  );
}

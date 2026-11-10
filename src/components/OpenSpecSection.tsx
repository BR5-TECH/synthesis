/**
 * Project settings → **OpenSpec** (SET-FR-FZKL, SET-FR-QVUP, SET-FR-ABXE).
 *
 * Whether the project uses OpenSpec, whether OpenSpec mode is active, the
 * state of the OpenSpec CLI on this machine, and the one **Use OpenSpec mode**
 * checkbox. The checkbox persists at once through `"save project config"`,
 * outside the section dirty state, and the section then reads the status
 * again (SET-FR-QVUP).
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type { OpenSpecCliState, OpenSpecModeSetting, OpenSpecStatus } from "../types";

/** SET-FR-ABXE: the install command the section shows. */
export const OPENSPEC_INSTALL_COMMAND = "npm install -g @fission-ai/openspec";

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** SET-FR-FZKL: the state of the CLI, in words. */
export function cliStatement(cli: OpenSpecCliState): string {
  switch (cli.state) {
    case "available":
      return `OpenSpec CLI ${cli.version} is installed at ${cli.path}.`;
    case "not_found":
      return "The OpenSpec CLI is not installed on this machine.";
    case "unsupported":
      return `OpenSpec CLI ${cli.version} at ${cli.path} is a version that is not supported.`;
    case "failed": {
      const why =
        cli.reason === "timed_out"
          ? "it did not answer in time"
          : cli.reason === "not_executable"
            ? "it could not be started"
            : "it did not report a version";
      return `The OpenSpec CLI at ${cli.path} failed: ${why}.`;
    }
  }
}

export function OpenSpecSection() {
  const [status, setStatus] = useState<OpenSpecStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const mounted = useRef(true);

  const read = () =>
    api
      .getOpenspecStatus()
      .then((next) => {
        if (!mounted.current) return;
        setStatus(next);
        logInfo(["frontend"], "openspec status read", {
          setting: next.setting,
          usesOpenspec: next.usesOpenspec,
          active: next.active,
          cli: next.cli.state,
        });
      })
      .catch((e) => {
        if (!mounted.current) return;
        setError(errorText(e));
        logWarn(["frontend"], "openspec status could not be read", { error: errorText(e) });
      });

  // SET-FR-FZKL: read when the section mounts.
  useEffect(() => {
    mounted.current = true;
    void read();
    return () => {
      mounted.current = false;
    };
  }, []);

  /** SET-FR-QVUP: persist at once, then read the status again. A failed write
   *  keeps the earlier state and shows its error inline. */
  const choose = (next: OpenSpecModeSetting) => {
    if (saving) return;
    setSaving(true);
    setError(null);
    // PSS-FR-17: the write names the line-ending convention as the store
    // holds it, so that a write of the mode leaves the convention as it stands.
    api
      .loadProjectConfig()
      .then((config) => api.saveProjectConfig({ lineEndings: config.lineEndings, openspecMode: next }))
      .then(() => {
        logInfo(["frontend"], "openspec mode setting saved", { setting: next });
        return read();
      })
      .catch((e) => {
        if (!mounted.current) return;
        setError(errorText(e));
        logWarn(["frontend"], "openspec mode setting not saved", {
          setting: next,
          error: errorText(e),
        });
      })
      .finally(() => {
        if (mounted.current) setSaving(false);
      });
  };

  const needsInstall =
    status !== null && (status.cli.state === "not_found" || status.cli.state === "unsupported");

  return (
    <div data-testid="openspec-settings" style={{ maxWidth: 560 }}>
      <p className="t-p" style={{ marginBottom: 20 }}>
        In OpenSpec mode, agents in discussions can read the capabilities and the
        changes of an OpenSpec project. The setting is committed with the project.
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 16 }}>
        <input
          type="checkbox"
          checked={status?.setting === "auto"}
          // `disabled` would move the focus off a checkbox the author just
          // toggled from the keyboard, so a write in flight only marks it.
          disabled={status === null}
          aria-disabled={saving || undefined}
          aria-busy={saving || undefined}
          onChange={(e) => choose(e.target.checked ? "auto" : "off")}
        />
        <span>Use OpenSpec mode</span>
      </label>

      {status && (
        <div data-testid="openspec-status">
          <p className="t-ui-sm">
            {status.usesOpenspec
              ? "This project uses OpenSpec."
              : "This project does not use OpenSpec. It holds no openspec/config.yaml and no capability spec."}
          </p>
          <p className="t-ui-sm">
            {status.active ? "OpenSpec mode is active." : "OpenSpec mode is not active."}
          </p>
          <p className="t-ui-xs t-muted" data-testid="openspec-cli" style={{ overflowWrap: "anywhere" }}>
            {cliStatement(status.cli)}
          </p>
        </div>
      )}

      {/* SET-FR-ABXE: what the mode needs, and how to get it. */}
      {needsInstall && (
        <div data-testid="openspec-install" style={{ marginTop: 12 }}>
          <p className="t-ui-xs">OpenSpec mode needs OpenSpec CLI 1.0 or later. Install it with:</p>
          <pre className="t-ui-xs" style={{ margin: "4px 0 0", userSelect: "text" }}>
            <code>{OPENSPEC_INSTALL_COMMAND}</code>
          </pre>
        </div>
      )}

      {error && (
        <p className="t-ui-xs" role="alert" style={{ marginTop: 12, color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </div>
  );
}

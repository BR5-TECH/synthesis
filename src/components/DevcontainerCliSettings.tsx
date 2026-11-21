/**
 * Global settings → Docker → **Dev container CLI** (GLS-FR-AGPG, GLS-FR-RQSP).
 *
 * A path field, a Verify action and a status line for `@devcontainers/cli` on
 * this machine. A change of the path persists at once through "save
 * devcontainer cli" and makes the status unverified, so the subsection holds no
 * dirty state and takes no part in the save-before-close sweep.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type { DevcontainerCli, DevcontainerCliFailure } from "../types";

/** GLS-FR-RQSP: a verification failure, in words. */
export function devcontainerCliFailureMessage(error: unknown): string {
  const failure = error as Partial<DevcontainerCliFailure> | null;
  switch (failure?.code) {
    case "not_found":
      return "Not found. Install it with npm install -g @devcontainers/cli, or type its path.";
    case "not_executable":
      return "The file at this path is not executable.";
    case "timed_out":
      return "The dev container CLI did not answer in time.";
    case "failed":
      return failure.message ? `The dev container CLI failed: ${failure.message}` : "The dev container CLI failed.";
    default:
      return String(error instanceof Error ? error.message : (error ?? "")) || "The verification failed.";
  }
}

export function DevcontainerCliSettings() {
  const [cli, setCli] = useState<DevcontainerCli | null>(null);
  const [path, setPath] = useState("");
  const [detected, setDetected] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const live = useRef(true);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => {
    api
      .loadDevcontainerCli()
      .then(async (loaded) => {
        if (!live.current) return;
        setCli(loaded);
        setPath(loaded.path);
        // GLS-FR-AGPG: an empty field is filled from detection and marked.
        if (loaded.path === "") {
          const found = await api.detectDevcontainerCli();
          if (live.current && found.path) {
            setPath(found.path);
            setDetected(true);
          }
        }
      })
      .catch((reason) => {
        logWarn(["frontend"], "the dev container CLI could not be read", { reason: String(reason) });
      });
  }, []);

  const commit = async () => {
    if (!cli || path.trim() === cli.path) return;
    setFailure(null);
    try {
      const saved = await api.saveDevcontainerCli(path.trim());
      if (live.current) setCli(saved);
    } catch (reason) {
      if (live.current) setFailure(String(reason));
    }
  };

  const verify = async () => {
    setVerifying(true);
    setFailure(null);
    try {
      if (cli && path.trim() !== cli.path) {
        const saved = await api.saveDevcontainerCli(path.trim());
        if (live.current) setCli(saved);
      }
      const verified = await api.verifyDevcontainerCli();
      logInfo(["frontend"], "dev container CLI verified", { version: verified.version ?? "" });
      if (live.current) {
        setCli(verified);
        setDetected(false);
      }
    } catch (reason) {
      const message = devcontainerCliFailureMessage(reason);
      logWarn(["frontend"], "dev container CLI verification failed", { reason: message });
      if (live.current) {
        setFailure(message);
        setCli((current) => (current ? { ...current, state: "unverified", version: null } : current));
      }
    } finally {
      if (live.current) setVerifying(false);
    }
  };

  return (
    <section aria-label="Dev container CLI" style={{ marginTop: 24 }}>
      <h3 className="t-ui-sm" style={{ margin: "0 0 4px" }}>
        Dev container CLI
      </h3>
      <p className="t-ui-xs" style={{ margin: "0 0 8px" }}>
        Only a project whose agent environment is a dev container needs it.
      </p>
      <label className="t-ui-sm" htmlFor="devcontainer-cli-path">
        Path
      </label>
      <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
        <input
          id="devcontainer-cli-path"
          type="text"
          value={path}
          placeholder="/usr/local/bin/devcontainer"
          onChange={(e) => {
            setPath(e.target.value);
            setDetected(false);
          }}
          onBlur={() => void commit()}
          onKeyDown={(e) => {
            if (e.key === "Enter") void commit();
          }}
          style={{ flex: 1 }}
        />
        <button
          type="button"
          aria-label="Verify the dev container CLI"
          onClick={() => void verify()}
          disabled={verifying || cli === null}
        >
          {verifying ? "Verifying…" : "Verify"}
        </button>
      </div>
      {detected && (
        <p className="t-ui-xs" style={{ marginTop: 6 }}>
          Found on this machine.
        </p>
      )}
      <p className="t-p" aria-live="polite" data-testid="devcontainer-cli-status" style={{ marginTop: 12 }}>
        {failure
          ? failure
          : cli?.state === "verified"
            ? `Verified — dev container CLI ${cli.version ?? ""}.`.replace(" .", ".")
            : "Not verified yet."}
      </p>
    </section>
  );
}

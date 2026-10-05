/**
 * Global settings → **Docker** (GLS-FR-29 through GLS-FR-31).
 *
 * Where the machine's Docker backend is configured: the backend mode, the
 * platform-neutral endpoint the Docker Engine mode reaches, and the executable
 * the Docker CLI mode runs. Verification is explicit and works the way an
 * agentic CLI binary's does (AII-FR-17, AII-FR-20) — a Verify action and a
 * status line that says what the backend is, in the selected mode's own terms.
 *
 * Every change applies at once through "save docker backend", so the section
 * holds no dirty state and contributes nothing to the save-before-close sweep of
 * GLS-FR-13. Changing the mode, the endpoint, or the CLI path returns the status
 * line to unverified without invoking any verification: the success that was
 * earned belonged to the values that earned it (GSS-FR-39).
 */
import { useCallback, useEffect, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import type {
  DockerBackend,
  DockerBackendConfig,
  DockerBackendMode,
  DockerEndpoint,
  DockerEndpointKind,
} from "../types";

/**
 * GLS-FR-29: exactly two mutually-exclusive choices, Bollard / Docker Engine
 * first because it is the default a machine that has configured nothing holds
 * (GSS-FR-35).
 */
export const DOCKER_MODE_CHOICES: [DockerBackendMode, string][] = [
  ["bollard", "Bollard / Docker Engine"],
  ["docker_cli", "Docker CLI"],
];

/** GLS-FR-29: the four platform-neutral endpoint forms (GSS-FR-36). */
export const DOCKER_ENDPOINT_CHOICES: [DockerEndpointKind, string][] = [
  ["automatic", "Automatic (the platform's default)"],
  ["unix_socket", "Unix socket"],
  ["windows_pipe", "Windows named pipe"],
  ["tcp", "TCP endpoint"],
];

/** Which form the endpoint holds, and the location it names. */
export function endpointKind(endpoint: DockerEndpoint): DockerEndpointKind {
  if (endpoint === "automatic") return "automatic";
  if ("unix_socket" in endpoint) return "unix_socket";
  if ("windows_pipe" in endpoint) return "windows_pipe";
  return "tcp";
}

export function endpointValue(endpoint: DockerEndpoint): string {
  if (endpoint === "automatic") return "";
  if ("unix_socket" in endpoint) return endpoint.unix_socket;
  if ("windows_pipe" in endpoint) return endpoint.windows_pipe;
  return endpoint.tcp;
}

export function makeEndpoint(
  kind: DockerEndpointKind,
  value: string,
): DockerEndpoint {
  switch (kind) {
    case "automatic":
      return "automatic";
    case "unix_socket":
      return { unix_socket: value };
    case "windows_pipe":
      return { windows_pipe: value };
    case "tcp":
      return { tcp: value };
  }
}

/**
 * GLS-FR-30: the selected mode's own failures, as sentences.
 *
 * In Docker CLI mode a missing or invalid executable and a Docker daemon that
 * cannot answer are different lines, and in Docker Engine mode an invalid
 * endpoint and a daemon that cannot answer are likewise different lines: each
 * asks the author to correct a different thing, so neither is ever folded into
 * the other (GSS-FR-38).
 */
export function dockerVerifyMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  const known: [string, string][] = [
    ["cli_path_empty", "Name the Docker CLI executable to verify."],
    ["cli_not_found", "There is nothing at that path."],
    [
      "cli_not_executable",
      "That file is there, but this application cannot run it.",
    ],
    ["not_the_docker_cli", "That program is not the Docker CLI."],
    ["endpoint_empty", "Give the Docker Engine endpoint to reach."],
    [
      "endpoint_invalid",
      "That is not a socket path, a named pipe, or a URL Docker can be reached at.",
    ],
    [
      "daemon_unreachable",
      "The Docker daemon did not answer. Start Docker, then verify again.",
    ],
    ["timed_out", "Docker did not answer in time."],
  ];
  for (const [code, sentence] of known) {
    if (raw.includes(code)) return sentence;
  }
  return raw || "Docker could not be verified.";
}

interface DockerSettingsProps {
  /**
   * Reported up whenever the stored record changes, so a host that renders the
   * backend elsewhere reflects it without a reload. Optional: the section
   * stands alone without one.
   */
  onBackendChange?: (backend: DockerBackend) => void;
}

export function DockerSettings({ onBackendChange }: DockerSettingsProps) {
  // Null until the stored record has landed, so nothing is rendered asserting a
  // selection nobody made.
  const [backend, setBackend] = useState<DockerBackend | null>(null);
  const [kind, setKind] = useState<DockerEndpointKind>("automatic");
  const [endpointText, setEndpointText] = useState("");
  const [cliPath, setCliPath] = useState("");
  const [detected, setDetected] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const apply = useCallback(
    (next: DockerBackend) => {
      setBackend(next);
      setKind(endpointKind(next.endpoint));
      setEndpointText(endpointValue(next.endpoint));
      setCliPath(next.cliPath ?? "");
      onBackendChange?.(next);
    },
    [onBackendChange],
  );

  // GLS-FR-29 / GLS-FR-31: initialised from "load docker backend" on mount, and
  // re-read rather than re-probed — a verified backend stays verified across a
  // relaunch and across a daemon that is stopped and started.
  useEffect(() => {
    let cancelled = false;
    void api
      .loadDockerBackend()
      .then((loaded) => {
        if (cancelled) return;
        apply(loaded);
        // GLS-FR-29: when the path field renders empty, detection fills it and
        // the value is marked as detected — exactly as a CLI-kind agentic tab
        // does (AII-FR-17).
        if (!loaded.cliPath) {
          void api
            .detectDockerCliBinary()
            .then(({ path }) => {
              if (cancelled || !path) return;
              setCliPath(path);
              setDetected(true);
            })
            .catch(() => {
              // Detection finding nothing is not an error the author acts on:
              // the field simply stays empty for them to fill.
            });
        }
      })
      .catch((e) => {
        if (cancelled) return;
        const message = e instanceof Error ? e.message : String(e);
        setSaveError(message);
        logWarn(["frontend"], "docker backend could not be read", {
          error: message,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [apply]);

  /** The selection as the two write operations take it. */
  const configFor = useCallback(
    (
      overrides: Partial<{
        mode: DockerBackendMode;
        kind: DockerEndpointKind;
        endpointText: string;
        cliPath: string;
      }> = {},
    ): DockerBackendConfig => {
      const nextKind = overrides.kind ?? kind;
      const nextText = overrides.endpointText ?? endpointText;
      const nextPath = (overrides.cliPath ?? cliPath).trim();
      return {
        mode: overrides.mode ?? backend?.mode ?? "bollard",
        endpoint: makeEndpoint(nextKind, nextText.trim()),
        cliPath: nextPath === "" ? null : nextPath,
      };
    },
    [backend?.mode, cliPath, endpointText, kind],
  );

  /**
   * GLS-FR-31: every change applies immediately, and returns the status line to
   * unverified where it changed one of the three bound values — which the
   * backend decides rather than this surface guessing (GSS-FR-39).
   */
  const persist = useCallback(
    async (config: DockerBackendConfig) => {
      setSaveError(null);
      // A new selection has not been verified, so whatever the last attempt
      // said about the last one no longer describes this one.
      setFailure(null);
      try {
        const saved = await api.saveDockerBackend(config);
        setBackend(saved);
        onBackendChange?.(saved);
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        setSaveError(message);
        logWarn(["frontend"], "docker backend selection not saved", {
          mode: config.mode,
          endpointKind: endpointKind(config.endpoint),
          error: message,
        });
      }
    },
    [onBackendChange],
  );

  const onMode = (mode: DockerBackendMode) => {
    setBackend((current) => (current ? { ...current, mode } : current));
    void persist(configFor({ mode }));
  };

  const onKind = (nextKind: DockerEndpointKind) => {
    setKind(nextKind);
    // Each form keeps what it last held, so switching back and forth does not
    // cost the author what they typed.
    void persist(configFor({ kind: nextKind }));
  };

  const onEndpointBlur = () => void persist(configFor());

  const onCliPathBlur = () => {
    setDetected(false);
    void persist(configFor());
  };

  /**
   * GLS-FR-29: the author may replace the path through a file picker as well as
   * by typing it, exactly as a CLI-kind agentic tab offers (AII-FR-18).
   *
   * A cancelled picker changes nothing at all — not the field, not the detected
   * mark, and not the stored record.
   */
  const browse = async () => {
    try {
      const result = await api.browseForFile();
      if (result === "cancelled" || !result?.selected?.path) return;
      const path = result.selected.path;
      setCliPath(path);
      setDetected(false);
      await persist(configFor({ cliPath: path }));
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setSaveError(message);
      logWarn(["frontend"], "docker cli path could not be chosen", {
        error: message,
      });
    }
  };

  /** GLS-FR-30: verification is explicit, and it is the one action here that
   * runs a program or leaves the machine. */
  const verify = async () => {
    if (verifying) return;
    setVerifying(true);
    setFailure(null);
    setSaveError(null);
    try {
      const verified = await api.verifyDockerBackend(configFor());
      apply(verified);
    } catch (e) {
      // GLS-FR-30: the section never presents a backend as verified on a daemon
      // that did not answer, so a failure leaves the record exactly as the
      // backend reports it and says which correction to make.
      setFailure(dockerVerifyMessage(e));
      const message = e instanceof Error ? e.message : String(e);
      logWarn(["frontend"], "docker backend verification refused", {
        mode: configFor().mode,
        reason: message,
      });
    } finally {
      setVerifying(false);
    }
  };

  if (!backend) {
    return (
      <div>
        <p className="t-p" style={{ marginBottom: 20 }}>
          How this machine reaches Docker, for the containers your agents run
          in.
        </p>
        <p className="t-p">{saveError ?? "Reading the Docker settings…"}</p>
      </div>
    );
  }

  const engineMode = backend.mode === "bollard";

  return (
    <div>
      <p className="t-p" style={{ marginBottom: 20 }}>
        How this machine reaches Docker, for the containers your agents run in.
        It belongs to you and this machine — a project never carries it.
      </p>

      <fieldset style={{ border: 0, padding: 0, margin: "0 0 20px" }}>
        <legend className="t-label">Backend</legend>
        {DOCKER_MODE_CHOICES.map(([value, label]) => (
          <label
            key={value}
            style={{ display: "flex", alignItems: "center", gap: 8 }}
          >
            <input
              type="radio"
              name="docker-backend-mode"
              value={value}
              checked={backend.mode === value}
              onChange={() => onMode(value)}
            />
            <span>{label}</span>
          </label>
        ))}
      </fieldset>

      {engineMode ? (
        <div style={{ marginBottom: 20 }}>
          <label className="t-label" htmlFor="docker-endpoint-kind">
            Docker endpoint
          </label>
          <select
            id="docker-endpoint-kind"
            value={kind}
            onChange={(e) => onKind(e.target.value as DockerEndpointKind)}
          >
            {DOCKER_ENDPOINT_CHOICES.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
          {kind !== "automatic" && (
            <input
              type="text"
              aria-label="Docker endpoint"
              value={endpointText}
              placeholder={
                kind === "unix_socket"
                  ? "/var/run/docker.sock"
                  : kind === "windows_pipe"
                    ? "//./pipe/docker_engine"
                    : "tcp://127.0.0.1:2375"
              }
              onChange={(e) => setEndpointText(e.target.value)}
              onBlur={onEndpointBlur}
              style={{ display: "block", marginTop: 8, width: "100%" }}
            />
          )}
        </div>
      ) : (
        <div style={{ marginBottom: 20 }}>
          <label className="t-label" htmlFor="docker-cli-path">
            Docker CLI path
          </label>
          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <input
              id="docker-cli-path"
              type="text"
              value={cliPath}
              placeholder="/usr/local/bin/docker"
              onChange={(e) => {
                setCliPath(e.target.value);
                setDetected(false);
              }}
              onBlur={onCliPathBlur}
              style={{ flex: 1 }}
            />
            <button type="button" onClick={() => void browse()}>
              Browse…
            </button>
          </div>
          {detected && (
            <p className="t-ui-xs" style={{ marginTop: 6 }}>
              Found on this machine.
            </p>
          )}
        </div>
      )}

      <button type="button" onClick={() => void verify()} disabled={verifying}>
        {verifying ? "Verifying…" : "Verify"}
      </button>

      <p className="t-p" role="status" style={{ marginTop: 12 }}>
        {failure
          ? failure
          : backend.state === "verified"
            ? `Verified — Docker ${backend.serverVersion ?? "answered"}.`
            : "Not verified yet."}
      </p>

      {saveError && (
        <p className="t-p" role="alert" style={{ marginTop: 8 }}>
          {saveError}
        </p>
      )}
    </div>
  );
}

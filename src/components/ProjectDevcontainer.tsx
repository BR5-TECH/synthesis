/**
 * Project settings → Docker → the **dev container panel** (SET-FR-FVIZ,
 * SET-FR-TEZH, SET-FR-PYRJ, SET-FR-CJIA, SET-FR-MVKM, SET-FR-VXQZ,
 * SET-FR-CLXD).
 *
 * It replaces the Docker-in-Docker controls and the vendor tabs while the agent
 * environment is Dev container. In this order it shows the Configuration
 * select, one agent CLI row for each vendor that can run, the Build control
 * with its Without cache checkbox, and the region the build renders in.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { onDevcontainerBuildFinished, onDevcontainerBuildProgress } from "../events";
import { logInfo, logWarn } from "../logging";
import { formatSectionAddress } from "../settingsWindow";
import type { AgentCliRow, AgentEnvironmentState, DevcontainerCli } from "../types";

/** SET-FR-26: the output region keeps this many lines, oldest dropped first. */
const KEPT_LINES = 500;

const VENDOR_LABELS: Record<string, string> = { claude_code: "Claude Code", codex: "Codex" };
const CLI_NAMES: Record<string, string> = { claude_code: "claude", codex: "codex" };

/** DVC-FR-WXYK / DVC-FR-JVKA / DVC-FR-BCDZ: a refusal, in words that say what to do. */
export function agentEnvironmentErrorMessage(error: unknown): string {
  const value = error as { code?: unknown; reason?: unknown; message?: unknown; line?: unknown };
  const code = typeof value?.code === "string" ? value.code : null;
  switch (code) {
    case "config_path_invalid":
      return value.reason === "absolute"
        ? "The config path must be relative to the project. Nothing was saved."
        : value.reason === "leaves_project"
          ? "The config path leaves the project. Nothing was saved."
          : "The config path names no file in the project. Nothing was saved.";
    case "config_missing":
      return "Select a devcontainer.json first. Nothing was saved.";
    case "config_invalid": {
      const line = typeof value.line === "number" ? ` (line ${value.line})` : "";
      return `The devcontainer.json is not valid JSON with comments${line}: ${String(value.message ?? "")}. The file was not changed.`;
    }
    case "config_exists":
      return "The project already has .devcontainer/devcontainer.json. Nothing was overwritten.";
    case "no_known_feature":
      return "This agent has no known Feature. Add its CLI to the dev container yourself.";
    case "store_failed":
      return `The setting could not be written: ${String(value.message ?? "")}.`;
    default:
      return String(error instanceof Error ? error.message : (error ?? "")) || "That failed. Nothing was changed.";
  }
}

/** DVC-FR-OXCZ: a build refusal, in words. */
export function devcontainerBuildRefusalMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  if (raw.includes("build_in_flight")) return "A build of this dev container already runs.";
  if (raw.includes("devcontainer_cli_missing")) {
    return "The dev container CLI is not verified. Verify it in Global settings, in the Docker section.";
  }
  if (raw.includes("docker_cli_required")) {
    return "No Docker CLI was found. The dev container CLI needs one. Set it in Global settings, in the Docker section.";
  }
  if (raw.includes("devcontainer_config_missing")) return "Select a devcontainer.json first.";
  return raw || "The build could not start.";
}

/** SET-FR-PYRJ: the state of a row, in words. */
function stateWords(row: AgentCliRow): string {
  switch (row.state) {
    case "found":
      return "Found";
    case "missing":
      return "Missing";
    default:
      return "Unknown until a build";
  }
}

interface BuildReport {
  operationId: string;
  running: boolean;
  lines: string[];
  outcome?: "succeeded" | "failed" | "cancelled";
  image?: string | null;
  error?: string | null;
}

interface ProjectDevcontainerProps {
  state: AgentEnvironmentState;
  onState: (next: AgentEnvironmentState) => void;
}

export function ProjectDevcontainer({ state, onState }: ProjectDevcontainerProps) {
  const [configs, setConfigs] = useState<string[] | null>(null);
  const [cli, setCli] = useState<DevcontainerCli | null>(null);
  const [sandboxed, setSandboxed] = useState(false);
  const [resolvedVendor, setResolvedVendor] = useState<string | null>(null);
  const [panelError, setPanelError] = useState<string | null>(null);
  const [rowError, setRowError] = useState<Record<string, string>>({});
  const [noCache, setNoCache] = useState(false);
  const [build, setBuild] = useState<BuildReport | null>(null);
  const live = useRef(true);
  const logRef = useRef<HTMLDivElement | null>(null);
  const selectRef = useRef<HTMLSelectElement | null>(null);
  /** SET-FR-TEZH: after Create, focus goes to the select that replaced the button. */
  const focusSelect = useRef(false);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  /** The state of the agent CLI rows comes from a reload after a build. */
  const reloadState = () =>
    api
      .loadAgentEnvironment()
      .then((next) => {
        if (live.current) onState(next);
      })
      .catch(() => {
        // The rows keep what they showed; the next action reads them again.
      });

  useEffect(() => {
    api
      .listDevcontainerConfigs()
      .then((found) => live.current && setConfigs(found))
      .catch(() => live.current && setConfigs([]));
    api
      .loadDevcontainerCli()
      .then((loaded) => live.current && setCli(loaded))
      .catch(() => live.current && setCli(null));
    // SET-FR-CLXD: the isolation mode decides one statement only.
    api
      .loadIsolationSettings()
      .then((loaded) => live.current && setSandboxed(loaded?.settings.mode === "microsandbox"))
      .catch(() => live.current && setSandboxed(false));
    // SET-FR-PYRJ: the vendor this project resolves to, as SET-FR-13 names it.
    api
      .getProjectAgenticIntegration()
      .then((resolved) => live.current && setResolvedVendor(resolved.vendor ?? null))
      .catch(() => {
        // A resolution this surface cannot read marks no row.
      });
    // SET-FR-MVKM / DVC-FR-LFQQ: a build outlives this panel.
    api
      .loadDevcontainerBuildInFlight()
      .then((running) => {
        if (live.current && running) {
          setBuild({ operationId: running.operationId, running: true, lines: running.lines });
        }
      })
      .catch(() => {
        // No build in flight is what a failed read means for this panel.
      });
  }, []);

  // SET-FR-MVKM: the lines and the result come from the backend's own events.
  useEffect(() => {
    const subscriptions = [
      onDevcontainerBuildProgress((event) => {
        if (!live.current) return;
        setBuild((current) => ({
          operationId: event.operationId,
          running: true,
          lines: [...(current?.operationId === event.operationId ? current.lines : []), event.message].slice(
            -KEPT_LINES,
          ),
        }));
      }),
      onDevcontainerBuildFinished((event) => {
        if (!live.current) return;
        setBuild((current) => ({
          operationId: event.operationId,
          running: false,
          lines: current?.operationId === event.operationId ? current.lines : [],
          outcome: event.outcome,
          image: event.image,
          error: event.error,
        }));
        void reloadState();
      }),
    ];
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
    // `reloadState` reads only refs and the stable `onState` of the parent.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (focusSelect.current && selectRef.current) {
      focusSelect.current = false;
      selectRef.current.focus();
    }
  }, [configs]);

  const shownLines = build?.lines.length ?? 0;
  useEffect(() => {
    const node = logRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [shownLines]);

  const selectConfig = async (path: string) => {
    setPanelError(null);
    try {
      onState(await api.saveAgentEnvironment({ source: "devcontainer", devcontainerConfig: path }));
      logInfo(["frontend"], "dev container config selected", { config: path });
    } catch (reason) {
      if (live.current) setPanelError(agentEnvironmentErrorMessage(reason));
    }
  };

  const create = async () => {
    setPanelError(null);
    try {
      const next = await api.createDevcontainerConfig();
      onState(next);
      const found = await api.listDevcontainerConfigs();
      if (live.current) {
        focusSelect.current = true;
        setConfigs(found);
      }
    } catch (reason) {
      if (live.current) setPanelError(agentEnvironmentErrorMessage(reason));
    }
  };

  const addFeature = async (vendor: string) => {
    setRowError((current) => ({ ...current, [vendor]: "" }));
    try {
      onState(await api.addDevcontainerAgentFeature(vendor));
    } catch (reason) {
      const message = agentEnvironmentErrorMessage(reason);
      logWarn(["frontend"], "dev container feature not added", { vendor, reason: message });
      if (live.current) setRowError((current) => ({ ...current, [vendor]: message }));
    }
  };

  const startBuild = async () => {
    setPanelError(null);
    setBuild({ operationId: "", running: true, lines: [] });
    try {
      const started = await api.buildDevcontainer(noCache);
      if (!live.current) return;
      setBuild((current) =>
        current && current.operationId === ""
          ? { ...current, operationId: started.operationId }
          : current,
      );
    } catch (reason) {
      if (!live.current) return;
      setBuild(null);
      setPanelError(devcontainerBuildRefusalMessage(reason));
    }
  };

  const cancelBuild = async (operationId: string) => {
    try {
      await api.cancelDevcontainerBuild(operationId);
    } catch (reason) {
      logWarn(["frontend"], "dev container build could not be cancelled", {
        reason: String(reason),
      });
    }
  };

  const openGlobalDocker = () => {
    void api.openSettingsWindow(formatSectionAddress("global", "docker")).catch((e) => {
      logWarn(["frontend"], "global docker settings could not be opened", {
        error: e instanceof Error ? e.message : String(e),
      });
    });
  };

  const selected = state.environment.devcontainerConfig;
  const cliVerified = cli?.state === "verified";
  const buildDisabledReason = !selected
    ? "Select a devcontainer.json to build it."
    : !cliVerified
      ? "Verify the dev container CLI in Global settings first."
      : null;

  return (
    <section aria-label="Dev container">
      {/* SET-FR-VXQZ: the CLI governs Build, so its state stands first. */}
      <div className="docker-backend">
        <span
          className="docker-backend__state"
          data-state={cli === null ? "reading" : cliVerified ? "verified" : "unverified"}
        >
          {cli === null
            ? "Reading the dev container CLI…"
            : cliVerified
              ? `Dev container CLI verified${cli.version ? ` (${cli.version})` : ""}`
              : "The dev container CLI is not verified"}
        </span>
        <button type="button" className="docker-backend__link" onClick={openGlobalDocker}>
          Global settings → Docker
        </button>
      </div>

      {/* SET-FR-CLXD: the two statements about what does not apply here. */}
      {sandboxed && (
        <p className="docker-state devcontainer__note" data-isolation="microsandbox">
          Microsandbox does not apply to this project: its agents run in its dev container on
          the host Docker.
        </p>
      )}
      <p className="docker-state devcontainer__note">
        The agents reach Docker only where the dev container gives it, for example through its
        docker-in-docker or docker-outside-of-docker Feature.
      </p>

      <div className="docker-field">
        <label className="picker-field__label" htmlFor="devcontainer-config">
          Configuration
        </label>
        {configs === null ? (
          <p className="docker-state">Looking for devcontainer.json files…</p>
        ) : configs.length === 0 ? (
          /* SET-FR-TEZH: the empty state is an invitation to create one. */
          <div className="devcontainer__empty">
            <p className="docker-state devcontainer__note">
              This project has no devcontainer.json. Create one with a Debian base image, Node
              and Claude Code, then change it to the environment the project needs.
            </p>
            <button type="button" className="btn btn--default btn--sm" onClick={() => void create()}>
              Create devcontainer.json
            </button>
          </div>
        ) : (
          <select
            id="devcontainer-config"
            ref={selectRef}
            className="devcontainer__select"
            value={selected ?? ""}
            onChange={(e) => void selectConfig(e.target.value)}
          >
            {!selected && (
              <option value="" disabled>
                Choose a devcontainer.json
              </option>
            )}
            {configs.map((path) => (
              <option key={path} value={path}>
                {path}
              </option>
            ))}
          </select>
        )}
      </div>
      {panelError && (
        <p className="docker-alert" role="alert">
          {panelError}
        </p>
      )}

      {/* SET-FR-PYRJ / SET-FR-CJIA: one row for each agent that can run. */}
      <ul className="devcontainer__agents" aria-label="Agent CLI in the dev container">
        {state.agentCli.map((row) => {
          const needed = resolvedVendor === row.vendor;
          const notFound = row.state !== "found";
          return (
            <li className="devcontainer__agent" key={row.vendor} data-state={row.state}>
              <span className="devcontainer__agent-name" data-graduates={needed}>
                {VENDOR_LABELS[row.vendor] ?? row.vendor}
              </span>
              <span className="devcontainer__agent-state">{stateWords(row)}</span>
              {needed && <span className="devcontainer__agent-mark">Needed for implementation</span>}
              {notFound && row.knownFeature && selected && (
                <button
                  type="button"
                  className="btn btn--ghost btn--sm devcontainer__agent-action"
                  onClick={() => void addFeature(row.vendor)}
                >
                  Add the {VENDOR_LABELS[row.vendor] ?? row.vendor} feature
                </button>
              )}
              {notFound && !row.knownFeature && (
                <span className="devcontainer__agent-requirement">
                  The dev container must hold <code>{CLI_NAMES[row.vendor]}</code> on the PATH of
                  its remote user.
                </span>
              )}
              {rowError[row.vendor] && (
                <span className="docker-alert devcontainer__agent-error" role="alert">
                  {rowError[row.vendor]}
                </span>
              )}
            </li>
          );
        })}
      </ul>

      <div className="docker-actions">
        <button
          type="button"
          className="btn btn--default btn--sm"
          onClick={() => void startBuild()}
          disabled={buildDisabledReason !== null || build?.running === true}
        >
          {build?.outcome === "failed" ? "Retry" : "Build"}
        </button>
        <label className="devcontainer__nocache">
          <input
            type="checkbox"
            checked={noCache}
            onChange={(e) => setNoCache(e.target.checked)}
          />
          <span>Without cache</span>
        </label>
        {build?.running && build.operationId !== "" && (
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            onClick={() => void cancelBuild(build.operationId)}
          >
            Cancel
          </button>
        )}
        {buildDisabledReason && <span className="docker-actions__hint">{buildDisabledReason}</span>}
      </div>

      {build && (
        <div className="docker-build" role="status">
          {build.running && <progress className="docker-build__bar" aria-label="Build progress" />}
          {!build.running && (
            <p className="docker-build__result" data-outcome={build.outcome}>
              {build.outcome === "succeeded"
                ? `Built ${build.image ?? "the dev container"}.`
                : build.outcome === "cancelled"
                  ? "The build was cancelled. Nothing here changed."
                  : (build.error ?? "The build failed.")}
            </p>
          )}
          {build.lines.length > 0 && (
            <div
              className="docker-build__log"
              ref={logRef}
              role="log"
              aria-live="off"
              aria-label="Build output"
              tabIndex={0}
            >
              {build.lines.map((line, index) => (
                <div className="docker-build__line" key={index}>
                  {line}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

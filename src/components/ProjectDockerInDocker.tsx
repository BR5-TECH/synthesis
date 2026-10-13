/**
 * Project settings → Docker → **Docker-in-Docker** (SET-FR-21, SET-FR-NXDV,
 * SET-FR-JMHE, SET-FR-PQLA).
 *
 * One checkbox above the vendor tabs, because the setting belongs to the whole
 * project and not to one vendor. While it is selected, a socket path field and a
 * Host gateway toggle follow it. Every change persists at once through
 * "save docker in docker settings", so the controls hold no dirty state and take
 * no part in the per-section save model of SET-FR-08.
 *
 * The section checks nothing about the socket and reaches no daemon. The socket
 * is checked when a run starts and when a container launches.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type { DockerInDockerSettings, DockerInDockerState } from "../types";

const NOT_READ_MESSAGE =
  "The Docker-in-Docker settings could not be read, so nothing is shown and nothing was changed.";

/** PSS-FR-VRHC: a save's typed refusal, in words that say what to do. */
export function dockerInDockerRefusalMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  if (raw.includes("docker_socket_path_invalid")) {
    return "The socket path must be a full path, such as /var/run/docker.sock. It must not hold a comma or a line break. Nothing was saved.";
  }
  if (raw.includes("docker_in_docker_unsupported")) {
    return "Docker-in-Docker is not available on this platform. Nothing was saved.";
  }
  return raw || "That could not be saved. Nothing was changed.";
}

/** The code of a refusal alone: the path a refusal carries is the author's own. */
function refusalCode(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  const match = /docker_socket_path_invalid|docker_in_docker_unsupported/.exec(raw);
  return match ? match[0] : "unknown";
}

export function ProjectDockerInDocker() {
  const [state, setState] = useState<DockerInDockerState | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [pathDraft, setPathDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  /** What the backend holds, as far as this window knows. */
  const confirmed = useRef<DockerInDockerSettings | null>(null);
  /**
   * What the controls ask for: every change lands here at once, so two changes
   * made before the first write returns are two writes of one record and the
   * later one carries the earlier one.
   */
  const desired = useRef<DockerInDockerSettings | null>(null);
  /** Writes go one after another, so the last change is the last write. */
  const chain = useRef<Promise<unknown>>(Promise.resolve());
  /**
   * SET-FR-PMHB: whether the machine's isolation is Microsandbox, which
   * ignores this setting. Read once on mount; `false` until it has landed and
   * where it cannot be read.
   */
  const [sandboxed, setSandboxed] = useState(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    api
      .loadIsolationSettings()
      .then((loaded) => {
        if (live) setSandboxed(loaded?.settings.mode === "microsandbox");
      })
      .catch((reason) => {
        logWarn(["frontend"], "the isolation mode could not be read", {
          code: refusalCode(reason),
        });
      });
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    api
      .loadDockerInDockerSettings()
      .then((loaded) => {
        if (!live) return;
        // A backend that answers nothing is read as the defaults it would hold.
        const found: DockerInDockerState = loaded ?? {
          enabled: false,
          socketPath: "",
          hostGateway: false,
          defaultSocketPath: "",
          support: "supported",
        };
        confirmed.current = {
          enabled: found.enabled,
          socketPath: found.socketPath,
          hostGateway: found.hostGateway,
        };
        desired.current = confirmed.current;
        setState(found);
        setPathDraft(found.socketPath);
      })
      .catch((reason) => {
        if (!live) return;
        setLoadFailed(true);
        logWarn(["frontend"], "the docker-in-docker settings could not be read", {
          code: refusalCode(reason),
        });
      });
    return () => {
      live = false;
    };
  }, []);

  /**
   * SET-FR-NXDV: persist the whole record at once. A change is a patch on what
   * the controls already ask for; a refusal restores the control to what the
   * backend holds.
   */
  const persist = useCallback((patch: Partial<DockerInDockerSettings>) => {
    const base = desired.current;
    if (!base) return Promise.resolve();
    const next: DockerInDockerSettings = {
      ...base,
      ...patch,
      socketPath: (patch.socketPath ?? base.socketPath).trim(),
    };
    desired.current = next;
    setState((current) => (current ? { ...current, ...next } : current));
    const write = chain.current.then(async () => {
      try {
        await api.saveDockerInDockerSettings(next);
        confirmed.current = next;
        if (!mounted.current) return;
        // Only the last write of a burst reports success, so an earlier one
        // does not clear a refusal that a later one is about to raise.
        if (desired.current === next) setError(null);
        setPathDraft((draft) => (draft.trim() === next.socketPath ? next.socketPath : draft));
        logInfo(["frontend"], "docker-in-docker settings saved", {
          enabled: next.enabled,
          hostGateway: next.hostGateway,
        });
      } catch (reason) {
        logWarn(["frontend"], "docker-in-docker settings were refused", {
          code: refusalCode(reason),
        });
        const held = confirmed.current;
        if (held) desired.current = held;
        if (!mounted.current) return;
        setError(dockerInDockerRefusalMessage(reason));
        if (held) {
          setState((current) => (current ? { ...current, ...held } : current));
          setPathDraft(held.socketPath);
        }
      }
    });
    chain.current = write;
    return write;
  }, []);

  if (loadFailed) {
    return (
      <p className="docker-alert" role="alert">
        {NOT_READ_MESSAGE}
      </p>
    );
  }
  if (!state) {
    return (
      <p className="docker-state" style={{ margin: 0 }}>
        Reading the Docker-in-Docker settings…
      </p>
    );
  }

  const unsupported = state.support === "unsupported";
  const commitPath = () => {
    if (pathDraft.trim() === state.socketPath) return;
    void persist({ socketPath: pathDraft });
  };

  return (
    <section className="docker-in-docker" aria-label="Docker-in-Docker">
      <label className="docker-in-docker__check">
        <input
          type="checkbox"
          checked={state.enabled && !unsupported}
          disabled={unsupported}
          onChange={(e) => void persist({ enabled: e.target.checked })}
        />
        <span>Docker-in-Docker</span>
      </label>
      <p className="docker-state docker-in-docker__note">
        Gives the agent container access to this machine&apos;s Docker daemon
        through its socket. The container then starts sibling containers on that
        daemon, for example when tests use Testcontainers. This setting is kept
        on this machine and is not committed with the project.
      </p>
      {sandboxed && (
        <p className="docker-state docker-in-docker__note" data-isolation="microsandbox">
          Isolation is Microsandbox in Global settings, so this setting is
          ignored: no host Docker socket is mounted, and a graduation start does
          not check the socket. The setting is kept for when Isolation is None.
        </p>
      )}
      {unsupported && (
        <p className="docker-state docker-in-docker__note" data-platform="unsupported">
          Docker-in-Docker is not available on Windows. A named pipe cannot be
          mounted into the agent container.
        </p>
      )}
      {state.support === "best_effort" && (
        <p className="docker-state docker-in-docker__note" data-platform="best-effort">
          On macOS, access is best effort. The group of the socket may not give
          the agent access to the daemon. If it does not, a run stops with a
          message that names the socket path.
        </p>
      )}

      {state.enabled && !unsupported && (
        <>
          <div className="docker-field">
            <label className="picker-field__label" htmlFor="docker-in-docker-socket">
              Docker socket path
            </label>
            <input
              id="docker-in-docker-socket"
              type="text"
              spellCheck={false}
              autoComplete="off"
              value={pathDraft}
              placeholder={state.defaultSocketPath}
              onChange={(e) => setPathDraft(e.target.value)}
              onBlur={commitPath}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitPath();
              }}
            />
            <span className="docker-state docker-in-docker__note">
              An empty field means the default
              {state.defaultSocketPath ? `, ${state.defaultSocketPath}` : ""}. The
              container always sees the socket at /var/run/docker.sock.
            </span>
          </div>
          <label className="docker-in-docker__check">
            <input
              type="checkbox"
              role="switch"
              checked={state.hostGateway}
              onChange={(e) => void persist({ hostGateway: e.target.checked })}
            />
            <span>Host gateway</span>
          </label>
          <p className="docker-state docker-in-docker__note">
            Maps host.docker.internal to the host and sets
            TESTCONTAINERS_HOST_OVERRIDE for Testcontainers.
          </p>
        </>
      )}

      {error && (
        <p className="docker-alert" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

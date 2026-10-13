/**
 * Global settings → **Isolation** (GLS-FR-YAEA through GLS-FR-GHYV).
 *
 * Whether the agents of graduation runs run inside a Microsandbox micro VM,
 * and with how many CPU cores, how much RAM, and what local network access.
 * The mode governs stream runs, direct runs, and merge runs, and no other
 * agentic CLI launch.
 *
 * Every change applies at once through "save isolation settings", so the
 * section holds no dirty state and contributes nothing to the save-before-close
 * sweep of GLS-FR-13. A refused save keeps the previous value in the control
 * and renders the refusal inline. Mounting reads the stored record and the
 * facts of this machine, and boots no VM.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type {
  IsolationMode,
  IsolationSettings as IsolationRecord,
  IsolationState,
  IsolationUnsupportedReason,
} from "../types";

/** GLS-FR-YAEA: the two choices, in order. */
export const ISOLATION_MODE_CHOICES: [IsolationMode, string][] = [
  ["none", "None"],
  ["microsandbox", "Microsandbox"],
];

/** GLS-FR-EZRL: the reason this machine cannot run Microsandbox, in words. */
export function unsupportedReasonMessage(
  reason: IsolationUnsupportedReason | null,
): string {
  switch (reason) {
    case "unsupported_platform":
      return "Microsandbox runs on Linux and macOS only. This platform cannot run it.";
    case "apple_silicon_required":
      return "Microsandbox needs a Mac with Apple Silicon. This Mac cannot run it.";
    case "kvm_missing":
      return "Microsandbox needs hardware virtualisation, and KVM (/dev/kvm) is missing on this machine.";
    case "kvm_inaccessible":
      return "Microsandbox needs hardware virtualisation, and your user cannot open KVM (/dev/kvm). Add your user to the group that owns it, then open this section again.";
    default:
      return "This machine cannot run Microsandbox.";
  }
}

/** GSS-FR-OAKR / GSS-FR-BVHS: a save's typed refusal, in words. */
export function isolationRefusalMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  if (raw.includes("isolation_unsupported")) {
    return "This machine cannot run Microsandbox. Nothing was saved.";
  }
  if (raw.includes("cpus_out_of_range")) {
    return "The CPU count must be between 1 and the CPU cores of this machine. Nothing was saved.";
  }
  if (raw.includes("memory_out_of_range")) {
    return "The RAM must be between 1 GB and the RAM of this machine. Nothing was saved.";
  }
  if (raw.includes("allowed_hosts_too_many")) {
    return "The list holds at most 64 entries. Nothing was saved.";
  }
  if (raw.includes("allowed_host_duplicate")) {
    return "The list already holds that entry. Nothing was saved.";
  }
  if (raw.includes("allowed_host_invalid")) {
    return "That is not an IP address, a CIDR range, or a host name. Nothing was saved.";
  }
  return raw || "That could not be saved. Nothing was changed.";
}

/** The code of a refusal alone: an entry the author typed can name a host. */
function refusalCode(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  return raw.split(":")[0] || "unknown";
}

const IPV4 = /^(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}$/;
const HOST_LABEL = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;

function isIpv6(text: string): boolean {
  if (!text.includes(":") || !/^[0-9A-Fa-f:.]+$/.test(text)) return false;
  const parts = text.split("::");
  if (parts.length > 2) return false;
  const groups = text.split(":").filter((g) => g !== "");
  return groups.length <= 8 && groups.every((g) => g.length <= 4 || IPV4.test(g));
}

function isHostName(text: string): boolean {
  const name = text.endsWith(".") ? text.slice(0, -1) : text;
  if (name === "" || name.length > 253) return false;
  return name.split(".").every((label) => HOST_LABEL.test(label));
}

/**
 * GLS-FR-QFWZ: the inline check of one entry, the same forms GSS-FR-BVHS
 * accepts. Returns the reason an entry is refused, or `null` for a valid one.
 */
export function allowedHostProblem(entry: string, list: string[]): string | null {
  const text = entry.trim();
  if (text === "") return "Type an IP address, a CIDR range, or a host name.";
  if (list.some((existing) => existing.toLowerCase() === text.toLowerCase())) {
    return "The list already holds that entry.";
  }
  if (list.length >= 64) return "The list holds at most 64 entries.";
  const slash = text.indexOf("/");
  if (slash >= 0) {
    const address = text.slice(0, slash);
    const prefix = text.slice(slash + 1);
    if (!/^\d{1,3}$/.test(prefix)) return "That is not a CIDR range.";
    const bits = Number(prefix);
    if (IPV4.test(address) && bits <= 32) return null;
    if (isIpv6(address) && bits <= 128) return null;
    return "That is not a CIDR range.";
  }
  if (IPV4.test(text) || isIpv6(text) || isHostName(text)) return null;
  return "That is not an IP address, a CIDR range, or a host name.";
}

/** One change: the fields it sets, or a function of the record it edits. */
type IsolationChange =
  | Partial<IsolationRecord>
  | ((base: IsolationRecord) => Partial<IsolationRecord>);

export function IsolationSettings() {
  const [state, setState] = useState<IsolationState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [entry, setEntry] = useState("");
  const [entryProblem, setEntryProblem] = useState<string | null>(null);
  /** The slider values while they move, so a drag does not write each step. */
  const [cpusDraft, setCpusDraft] = useState<number | null>(null);
  const [memoryDraft, setMemoryDraft] = useState<number | null>(null);
  const mounted = useRef(true);
  /** Writes go one after another, so the last change is the last write. */
  const chain = useRef<Promise<unknown>>(Promise.resolve());
  /**
   * GLS-FR-GHYV: the record the backend holds after the last write. Each write
   * starts from it when its turn comes, so it carries every earlier change.
   */
  const held = useRef<IsolationState | null>(null);

  useEffect(() => {
    mounted.current = true;
    void api
      .loadIsolationSettings()
      .then((loaded) => {
        if (!mounted.current) return;
        held.current = loaded;
        setState(loaded);
      })
      .catch((e) => {
        const message = e instanceof Error ? e.message : String(e);
        if (mounted.current) setLoadError(message);
        logWarn(["frontend"], "isolation settings could not be read", {
          error: message,
        });
      });
    return () => {
      mounted.current = false;
    };
  }, []);

  /**
   * GLS-FR-GHYV: every change persists at once. A refused save keeps the value
   * the backend holds and renders the refusal inline. The record to write is
   * built when the write's turn comes, from the record the earlier writes
   * left, so two quick changes are both kept. A list edit passes a function
   * of that record, so it edits the list the earlier writes left.
   */
  const persist = useCallback((change: IsolationChange) => {
    if (!held.current) return;
    setError(null);
    chain.current = chain.current.then(async () => {
      const base = held.current?.settings;
      if (!base) return;
      const next: IsolationRecord = {
        ...base,
        ...(typeof change === "function" ? change(base) : change),
      };
      try {
        const saved = await api.saveIsolationSettings(next);
        held.current = saved;
        if (!mounted.current) return;
        setState(saved);
        logInfo(["frontend"], "isolation settings changed", {
          mode: saved.settings.mode,
          cpus: saved.effectiveCpus,
          memoryGib: saved.effectiveMemoryGib,
          blockLocalNetwork: saved.settings.blockLocalNetwork,
          allowedHosts: saved.settings.allowedLocalHosts.length,
        });
      } catch (e) {
        if (!mounted.current) return;
        setError(isolationRefusalMessage(e));
        logWarn(["frontend"], "isolation settings not saved", {
          reason: refusalCode(e),
        });
      }
    });
  }, []);

  if (loadError) {
    return (
      <div>
        <p className="t-p" style={{ marginBottom: 20 }}>
          How the agents of graduation runs are contained on this machine.
        </p>
        <p className="t-p" role="alert">
          {loadError}
        </p>
      </div>
    );
  }
  if (!state) {
    return (
      <div>
        <p className="t-p" style={{ marginBottom: 20 }}>
          How the agents of graduation runs are contained on this machine.
        </p>
        <p className="t-p">Reading the isolation settings…</p>
      </div>
    );
  }

  const { settings, host } = state;
  const unsupported = !host.microsandboxSupported;
  const sandboxed = settings.mode === "microsandbox";
  const cpus = cpusDraft ?? state.effectiveCpus;
  const memory = memoryDraft ?? state.effectiveMemoryGib;

  /**
   * GLS-FR-USNH / GLS-FR-GHYV: a slider writes once, when the author lets go
   * of it, rather than once for each step of a drag.
   */
  const commitCpus = () => {
    // Every drag writes, also one that ends at the value on screen: a write
    // still in progress can change that value, so it is not compared.
    if (cpusDraft !== null) {
      persist({ cpus: cpusDraft });
    }
    setCpusDraft(null);
  };
  const commitMemory = () => {
    if (memoryDraft !== null) {
      persist({ memoryGib: memoryDraft });
    }
    setMemoryDraft(null);
  };

  const addEntry = () => {
    const problem = allowedHostProblem(entry, settings.allowedLocalHosts);
    setEntryProblem(problem);
    if (problem) return;
    const added = entry.trim();
    // An add still waiting for its write is not in the list on screen, so the
    // list the earlier writes left is checked again when this write's turn
    // comes. A repeated entry then adds nothing.
    persist((base) =>
      allowedHostProblem(added, base.allowedLocalHosts) === null
        ? { allowedLocalHosts: [...base.allowedLocalHosts, added] }
        : {},
    );
    setEntry("");
  };

  return (
    <div>
      <p className="t-p" style={{ marginBottom: 20 }}>
        How the agents of graduation runs are contained on this machine. The
        choice applies to stream, direct, and merge runs, and to no other agent
        call. It belongs to you and this machine — a project never carries it.
      </p>

      <fieldset style={{ border: 0, padding: 0, margin: "0 0 12px" }}>
        <legend className="t-label">Isolation</legend>
        {ISOLATION_MODE_CHOICES.map(([value, label]) => {
          const disabled = value === "microsandbox" && unsupported;
          return (
            <label
              key={value}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                opacity: disabled ? 0.55 : 1,
              }}
            >
              <input
                type="radio"
                name="isolation-mode"
                value={value}
                checked={settings.mode === value}
                disabled={disabled}
                onChange={() => persist({ mode: value })}
              />
              <span>{label}</span>
            </label>
          );
        })}
      </fieldset>

      {unsupported && (
        <p className="t-ui-xs t-muted" data-testid="isolation-unsupported" style={{ marginBottom: 12, maxWidth: 520 }}>
          {unsupportedReasonMessage(host.unsupportedReason)}
        </p>
      )}

      <p className="t-ui-xs t-muted" style={{ marginBottom: 20, maxWidth: 520 }}>
        {sandboxed
          ? "Each work stream runs its agents in one micro VM, booted from the Docker image the agent is configured with."
          : "Agents run in their Docker containers as they always have."}{" "}
        Under Microsandbox, the Docker-in-Docker setting of a project is ignored:
        no host Docker socket is mounted into the sandbox, and a graduation start
        does not check the socket.
      </p>

      {sandboxed && (
        <>
          <div style={{ marginBottom: 20, maxWidth: 520 }}>
            <label className="t-label" htmlFor="isolation-cpus">
              CPU cores — {cpus}
            </label>
            <input
              id="isolation-cpus"
              type="range"
              min={1}
              max={host.logicalCpus}
              step={1}
              value={cpus}
              aria-valuetext={`${cpus} of ${host.logicalCpus} cores`}
              onChange={(e) => setCpusDraft(Number(e.target.value))}
              onPointerUp={() => commitCpus()}
              onKeyUp={() => commitCpus()}
              onBlur={() => commitCpus()}
              style={{ display: "block", width: "100%", marginTop: 8, accentColor: "var(--accent)" }}
            />
            <span className="t-ui-xs t-muted">1 – {host.logicalCpus} cores</span>
          </div>

          <div style={{ marginBottom: 20, maxWidth: 520 }}>
            <label className="t-label" htmlFor="isolation-memory">
              RAM — {memory} GB
            </label>
            <input
              id="isolation-memory"
              type="range"
              min={1}
              max={host.totalMemoryGib}
              step={1}
              value={memory}
              aria-valuetext={`${memory} of ${host.totalMemoryGib} GB`}
              onChange={(e) => setMemoryDraft(Number(e.target.value))}
              onPointerUp={() => commitMemory()}
              onKeyUp={() => commitMemory()}
              onBlur={() => commitMemory()}
              style={{ display: "block", width: "100%", marginTop: 8, accentColor: "var(--accent)" }}
            />
            <span className="t-ui-xs t-muted">1 – {host.totalMemoryGib} GB</span>
          </div>

          <p className="t-ui-xs t-muted" style={{ marginBottom: 12, maxWidth: 520 }}>
            Outbound internet access is always allowed, because the agent needs
            it to reach its service.
          </p>

          <label style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <input
              type="checkbox"
              role="switch"
              checked={settings.blockLocalNetwork}
              onChange={(e) => persist({ blockLocalNetwork: e.target.checked })}
            />
            <span>Block local network access</span>
          </label>
          <p className="t-ui-xs t-muted" style={{ marginTop: 6, marginBottom: 16, maxWidth: 520 }}>
            Blocks loopback, private ranges, link-local addresses, and this
            machine itself. The hosts listed below stay reachable.
          </p>

          <fieldset
            disabled={!settings.blockLocalNetwork}
            style={{
              border: 0,
              padding: 0,
              margin: 0,
              maxWidth: 520,
              opacity: settings.blockLocalNetwork ? 1 : 0.55,
            }}
          >
            <legend className="t-label">Allowed local hosts</legend>
            {settings.allowedLocalHosts.length === 0 ? (
              <p className="t-ui-xs t-muted" style={{ margin: "6px 0" }}>
                No local host is allowed.
              </p>
            ) : (
              <ul aria-label="Allowed local hosts" style={{ listStyle: "none", padding: 0, margin: "6px 0" }}>
                {settings.allowedLocalHosts.map((host) => (
                  <li
                    key={host}
                    style={{ display: "flex", alignItems: "center", gap: 8, padding: "2px 0" }}
                  >
                    <span className="t-code" style={{ flex: 1 }}>
                      {host}
                    </span>
                    <button
                      type="button"
                      aria-label={`Remove ${host}`}
                      onClick={() =>
                        persist((base) => ({
                          allowedLocalHosts: base.allowedLocalHosts.filter((h) => h !== host),
                        }))
                      }
                    >
                      Remove
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
              <input
                type="text"
                aria-label="Allowed local host"
                spellCheck={false}
                autoComplete="off"
                value={entry}
                placeholder="192.168.1.20, 10.0.0.0/8, or gateway.lan"
                aria-invalid={entryProblem !== null}
                onChange={(e) => {
                  setEntry(e.target.value);
                  setEntryProblem(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") addEntry();
                }}
                style={{ flex: 1 }}
              />
              <button type="button" onClick={addEntry}>
                Add
              </button>
            </div>
            {entryProblem && (
              <p className="t-ui-xs" role="alert" style={{ marginTop: 6, color: "var(--danger)" }}>
                {entryProblem}
              </p>
            )}
            <p className="t-ui-xs t-muted" data-testid="isolation-gateway-warning" style={{ marginTop: 8 }}>
              A Custom Gateway whose address is localhost or a local network
              address is blocked until you add its host to this list. The
              application does not add the gateway host for you.
            </p>
          </fieldset>
        </>
      )}

      {error && (
        <p className="t-ui-xs" role="alert" style={{ marginTop: 12, color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </div>
  );
}

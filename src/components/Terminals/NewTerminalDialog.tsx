/**
 * The New terminal dialog (`TRM-terminals.md` TRM-FR-JINY … TRM-FR-IVGX).
 *
 * It reads the targets when it opens, shows a target list and a kind list, and
 * creates the session on Create or Enter. A kind that is not available is
 * greyed out with its reason in plain text, so the author sees why without a
 * hover.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import { errorCode, targetKey, upsertTerminal } from "../../state/terminals";
import { kindName, reasonText, roleName, type TargetRole } from "../../state/terminalGroups";
import type { KindAvailability, TerminalError, TerminalSummary, TerminalTarget } from "../../types";
import { Modal } from "../DraftsPanelParts";
import { Icon } from "../icons";

const KIND_ORDER = ["host_shell", "container_shell", "microsandbox_shell", "agent_cli"] as const;

function roleOf(target: TerminalTarget): TargetRole {
  if (target.target.kind === "work_stream") return "stream";
  return target.isActiveWorktree ? "active" : "worktree";
}

/** TRM-FR-AASW: active worktree, other worktrees by name, work streams by name. */
export function orderTargets(targets: readonly TerminalTarget[]): TerminalTarget[] {
  const byName = (a: TerminalTarget, b: TerminalTarget) => a.name.localeCompare(b.name);
  return [
    ...targets.filter((t) => roleOf(t) === "active"),
    ...targets.filter((t) => roleOf(t) === "worktree").sort(byName),
    ...targets.filter((t) => roleOf(t) === "stream").sort(byName),
  ];
}

/** TRM-FR-CZTR: the kinds in their fixed order, Agent CLI entries last. */
export function orderKinds(kinds: readonly KindAvailability[]): KindAvailability[] {
  return [...kinds].sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind));
}

function kindKey(kind: KindAvailability): string {
  return kind.vendor ? `${kind.kind}:${kind.vendor}` : kind.kind;
}

function placeLabel(kind: KindAvailability): string {
  return kind.runsIn === "devcontainer" ? "dev container" : kind.runsIn === "microsandbox" ? "vm" : kind.runsIn;
}

function failureText(err: unknown): string {
  const code = errorCode(err);
  const typed = err as Partial<TerminalError> | null;
  if (code === "terminal_limit_reached") return "The project holds the most terminals it can hold (16).";
  if (code === "target_gone") return "This target is gone.";
  if (code === "kind_unavailable") return reasonText(typed?.reason);
  if (code === "platform_unsupported") return "Terminals are not available on this platform.";
  return typed?.message ? `The terminal did not start: ${typed.message}` : "The terminal did not start.";
}

export function NewTerminalDialog({
  onClose,
  onCreated,
  returnFocus,
}: {
  onClose: () => void;
  onCreated: (session: TerminalSummary) => void;
  returnFocus?: HTMLElement | null;
}) {
  const [targets, setTargets] = useState<TerminalTarget[] | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [targetIndex, setTargetIndex] = useState(0);
  const [kindChoice, setKindChoice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const targetList = useRef<HTMLFieldSetElement>(null);
  const loaded = targets !== null;

  // TRM-FR-FYYB: once the targets are read, the selected target holds the
  // focus, so Enter creates the terminal at once.
  useEffect(() => {
    if (!loaded) return;
    targetList.current?.querySelector<HTMLInputElement>("input:checked")?.focus();
  }, [loaded]);

  useEffect(() => {
    let live = true;
    api
      .listTerminalTargets()
      .then((list) => {
        if (live) setTargets(orderTargets(list));
      })
      .catch((err) => {
        logWarn(["frontend"], "terminal targets read failed", { code: errorCode(err) });
        if (live) setLoadFailed(true);
      });
    return () => {
      live = false;
    };
  }, []);

  const target = targets?.[targetIndex] ?? null;
  const kinds = useMemo(() => (target ? orderKinds(target.kinds) : []), [target]);
  // TRM-FR-CZTR: the first available entry is selected when the target changes.
  const selected =
    kinds.find((k) => kindKey(k) === kindChoice && k.available) ?? kinds.find((k) => k.available) ?? null;

  const create = async () => {
    if (!target || !selected || busy) return;
    setBusy(true);
    setError(null);
    try {
      const session = await api.createTerminal(target.target, selected.kind, selected.vendor ?? null);
      upsertTerminal(session);
      logDebug(["frontend"], "terminal created", { terminalId: session.id, kind: session.kind });
      onCreated(session);
    } catch (err) {
      // TRM-FR-YVPJ: the dialog stays open and names the failure.
      logWarn(["frontend"], "terminal create failed", { code: errorCode(err), kind: selected.kind });
      setError(failureText(err));
      setBusy(false);
    }
  };

  return (
    <Modal label="New terminal" onClose={onClose} returnFocus={returnFocus} className="new-terminal">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void create();
        }}
      >
        <div className="modal__head">
          <div className="modal__title">New terminal</div>
        </div>
        <div className="modal__body new-terminal__body">
          {loadFailed && (
            <p className="new-terminal__error" role="alert">
              The targets could not be read.
            </p>
          )}
          {!targets && !loadFailed && <p role="status">Reading targets…</p>}
          {targets && (
            <>
              <fieldset className="new-terminal__group" ref={targetList}>
                <legend>Target</legend>
                {targets.map((t, i) => (
                  <label key={targetKey(t.target)} className="new-terminal__option">
                    <input
                      type="radio"
                      name="terminal-target"
                      checked={i === targetIndex}
                      onChange={() => {
                        setTargetIndex(i);
                        setKindChoice(null);
                      }}
                    />
                    <span className="new-terminal__name">{t.name}</span>
                    {t.branch && <span className="new-terminal__meta">{t.branch}</span>}
                    <span className="new-terminal__meta">{roleName(roleOf(t))}</span>
                    {t.busyRunId && (
                      <span className="terminal-busy" title="A graduation run is working in this target">
                        <Icon.AlertTriangle size={11} aria-hidden="true" /> busy
                      </span>
                    )}
                  </label>
                ))}
              </fieldset>
              {target?.busyRunId && (
                <p className="new-terminal__warning" role="note">
                  A graduation run is working in this target.
                </p>
              )}
              <fieldset className="new-terminal__group">
                <legend>Kind</legend>
                {kinds.map((k) => (
                  <label
                    key={kindKey(k)}
                    className="new-terminal__option"
                    data-unavailable={k.available ? undefined : "true"}
                  >
                    <input
                      type="radio"
                      name="terminal-kind"
                      disabled={!k.available}
                      checked={selected !== null && kindKey(selected) === kindKey(k)}
                      onChange={() => setKindChoice(kindKey(k))}
                    />
                    <span className="new-terminal__name">
                      {k.kind === "agent_cli" && k.vendor ? kindName(k.kind, k.vendor).replace("Agent CLI · ", "") : kindName(k.kind)}
                    </span>
                    <span className="new-terminal__meta">
                      {k.available ? placeLabel(k) : reasonText(k.reason)}
                    </span>
                  </label>
                ))}
              </fieldset>
            </>
          )}
          {error && (
            <p className="new-terminal__error" role="alert">
              {error}
            </p>
          )}
        </div>
        <div className="modal__actions">
          <button type="button" className="btn btn--sm" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="btn btn--primary btn--sm" disabled={!selected || busy}>
            Create
          </button>
        </div>
      </form>
    </Modal>
  );
}

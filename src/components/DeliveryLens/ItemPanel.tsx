/**
 * What the Delivery lens adds at the end of the selected item's panel: the
 * **Specs** list (`SDL-specification-delivery-links.md` SDL-FR-KNOP,
 * SDL-FR-BVNS), the claim actions of a GitHub task (`MDL-map-delivery-lens.md`
 * MDL-FR-QEXS, MDL-FR-UKGH), and **Add existing issue** of a container
 * (MDL-FR-PUKL).
 */
import { useState } from "react";

import type { GithubPollingController } from "../../hooks/useGithubPolling";
import { logInfo } from "../../logging";
import type { FlowItem } from "../../state/flowDocument";
import { projectPath, specFolder, type TreeIndex } from "../../state/specMap/tree";
import type { ItemDeliveryState } from "../../types";
import type { TrackerFacts } from "../../types/delivery";

/** SDL-FR-KNOP / SDL-FR-VYIF: the Specs list of the selected item. */
export function SpecsList({
  codes,
  tree,
  indexLoading,
  onOpenSpec,
}: {
  /** The spec codes the item links, or null where the plan has no links yet. */
  codes: readonly string[] | null;
  tree: TreeIndex | null;
  /** SDL-FR-VYIF: the index has not answered yet. */
  indexLoading: boolean;
  onOpenSpec: (path: string) => void;
}) {
  if (!tree) {
    if (!indexLoading) return null;
    return (
      <div className="dlens-panel" data-testid="delivery-specs">
        <span className="flow-panel__label">Specs</span>
        <p className="dlens-panel__loading" role="status">
          Loading the specification index…
        </p>
      </div>
    );
  }
  const specs = (codes ?? []).map((code) => tree.specs.get(code)).filter((s) => s !== undefined);
  if (specs.length === 0) return null;
  return (
    <div className="dlens-panel" data-testid="delivery-specs">
      <span className="flow-panel__label">Specs</span>
      <div className="dlens-specs">
        {specs.map((spec) => (
          <button
            key={spec.code}
            type="button"
            className="dlens-spec"
            aria-label={`Show ${spec.badge ?? spec.code} ${spec.label} on the specification map`}
            onClick={() => onOpenSpec(projectPath(tree.index, spec))}
          >
            <span className="smap-code" data-folder={specFolder(spec)}>
              {spec.badge ?? spec.code}
            </span>
            <span className="dlens-spec__label">{spec.label}</span>
            <span className="smap-dot" data-state={spec.state} aria-label={spec.state} />
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * MDL-FR-QEXS / MDL-FR-UKGH: the claim, retry, and graduate actions of a
 * selected issue of a GitHub plan, on the terms of the Ready tasks section
 * (GIT-FR-NQTZ, GIT-FR-FTHT, GIT-FR-OLNA).
 */
export function ClaimActions({
  item,
  tracker,
  state,
  claims,
  inert,
}: {
  item: FlowItem;
  tracker: TrackerFacts | undefined;
  state: ItemDeliveryState | undefined;
  claims: GithubPollingController | null | undefined;
  inert: boolean;
}) {
  if (!claims || !tracker || tracker.ref.kind !== "issue") return null;
  const number = tracker.ref.number;
  const view = claims.view;
  // A shadow draft of any status names the issue; only one that is still a
  // shadow can graduate.
  const anyShadow = view?.shadows.find((s) => s.issueNumber === number);
  const shadowRow = anyShadow?.status === "github_shadow" ? anyShadow : undefined;
  const pending = view?.pendingClaims.find((p) => p.issueNumber === number);
  const busy = claims.rowBusy.get(number);
  const error = claims.rowErrors.get(number);

  // MDL-FR-UKGH: the item's draft is a GitHub-shadow draft that no run holds.
  const shadowDraft =
    state?.draft?.status === "github_shadow" && state.run === null
      ? { id: state.draft.id, name: state.draft.name }
      : shadowRow && !shadowRow.locked
        ? { id: shadowRow.draftId, name: shadowRow.name }
        : null;

  let action: { label: string; run: () => void } | null = null;
  if (pending) {
    // MDL-FR-QEXS / GIT-FR-FTHT: a pending claim offers Retry.
    action = { label: busy === "retry" ? "Retrying…" : "Retry claim", run: () => void claims.retry(number) };
  } else if (shadowDraft) {
    action = {
      label: "Graduate",
      run: () => {
        logInfo(["frontend"], "delivery lens opens a graduation of a shadow draft", { issueNumber: number });
        claims.graduateShadow(shadowDraft.id, shadowDraft.name);
      },
    };
  } else if (item.kind === "task" && tracker.claimEligible && !anyShadow) {
    action = {
      label: busy === "claim" ? "Claiming…" : "Claim and graduate",
      run: () => void claims.claim(number),
    };
  }
  if (!action) return null;
  return (
    <div className="dm-claim" data-testid="delivery-claim">
      <button
        type="button"
        className="btn btn--primary btn--sm"
        disabled={inert || busy !== undefined}
        onClick={action.run}
      >
        {action.label}
      </button>
      {error && (
        <div className="flow-panel__error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}

/** MDL-FR-PUKL: Add existing issue, into a container or the top level. */
export function AddExistingIssue({
  containerName,
  inert,
  onAdd,
  onDone,
  autoFocus = false,
}: {
  /** The container's name, or null for the top level. */
  containerName: string | null;
  inert: boolean;
  /** Resolves the error text of a refusal, or null where the issue was added. */
  onAdd: (number: number) => Promise<string | null>;
  onDone?: () => void;
  autoFocus?: boolean;
}) {
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const number = /^#?\d+$/.test(text.trim()) ? Number(text.trim().replace("#", "")) : null;
  const submit = async () => {
    if (number === null || number <= 0 || busy) return;
    setBusy(true);
    setError(null);
    const refused = await onAdd(number);
    setBusy(false);
    if (refused) {
      setError(refused);
      return;
    }
    setText("");
    onDone?.();
  };
  const where = containerName === null ? "the top level" : containerName || "Unnamed";
  return (
    <div className="dlens-panel dlens-add-issue" data-testid="delivery-add-issue">
      <span className="flow-panel__label">Add existing issue</span>
      <div className="flow-panel__row">
        <input
          className="input input--mono dlens-add-issue__input"
          aria-label={`Issue number to add into ${where}`}
          placeholder="#123"
          inputMode="numeric"
          value={text}
          disabled={inert || busy}
          autoFocus={autoFocus}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void submit();
            if (e.key === "Escape" && onDone) {
              e.stopPropagation();
              onDone();
            }
          }}
        />
        <button
          type="button"
          className="btn btn--default btn--sm"
          disabled={inert || busy || number === null || number <= 0}
          onClick={() => void submit()}
        >
          {busy ? "Adding…" : "Add"}
        </button>
      </div>
      {error && (
        <div className="flow-panel__error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}

/**
 * The publication chooser — the modal that decides whether a draft is published
 * as a root issue or as a sub-issue, and with which Type and milestone
 * (`NAW-new-artifact.md` NAW-FR-HNVR … NAW-FR-PSMO, served by
 * `../../specifications/core/GHP-github-publication.md` GHP-FR-MDLD).
 *
 * Three lists feed it — the parent issues, the issue Types, and the
 * milestones — and each renders its own loading, empty, unavailable, or loaded
 * state (NAW-FR-WMEP). None of them gates the root option: a list that cannot
 * be read costs the author the choices that needed it and nothing else
 * (NAW-FR-TCQB, GHP-FR-UXOT).
 */
import { useState } from "react";
import { Modal } from "./DraftsPanelParts";
import type { PublicationChooserState } from "../hooks/useDraftPublication";
import type {
  MetadataList,
  PublicationChoiceInput,
  PublicationMetadata,
  PublicationMilestone,
  PublicationParentIssue,
} from "../types";

/** The root option's value in the radio group; a parent's value is its number. */
const ROOT = "root";
const NONE = "";

type ListView<T> =
  | { kind: "loading" }
  | { kind: "unavailable"; error: string }
  | { kind: "empty" }
  | { kind: "loaded"; items: T[] };

/**
 * NAW-FR-WMEP: the four states a list renders. A chooser whose whole read
 * failed has no list, and states each as unavailable with that error.
 */
function viewOf<T>(
  state: PublicationChooserState,
  pick: (metadata: PublicationMetadata) => MetadataList<T>,
): ListView<T> {
  if (state.status === "loading") return { kind: "loading" };
  if (state.status === "failed" || !state.metadata) {
    return { kind: "unavailable", error: state.error ?? "GitHub could not be read." };
  }
  const list = pick(state.metadata);
  if (list.state === "failed") {
    return { kind: "unavailable", error: list.error ?? "GitHub could not be read." };
  }
  return list.items.length === 0 ? { kind: "empty" } : { kind: "loaded", items: list.items };
}

/** Where focus goes back to: the action control's trigger (NAW-FR-VUCK). */
function actionTrigger(): HTMLElement | null {
  return document.querySelector<HTMLElement>('button[aria-label="Draft actions"]');
}

function Notice({ children }: { children: React.ReactNode }) {
  return (
    <p className="t-ui-xs draft-publication-chooser__notice" role="status">
      {children}
    </p>
  );
}

/** NAW-FR-FGUI: a milestone control, or the notice that stands in its place. */
function MilestoneField({
  view,
  value,
  onChange,
  id,
}: {
  view: ListView<PublicationMilestone>;
  value: string;
  onChange: (next: string) => void;
  id: string;
}) {
  switch (view.kind) {
    case "loading":
      return <Notice>Loading milestones…</Notice>;
    case "unavailable":
      return (
        <Notice>
          {`Milestones are unavailable: ${view.error} The issue publishes without a milestone.`}
        </Notice>
      );
    case "empty":
      return (
        <Notice>There is no open milestone. The issue publishes without a milestone.</Notice>
      );
    default:
      return (
        <div className="draft-publication-chooser__field">
          <label className="t-ui-sm" htmlFor={id}>
            Milestone
          </label>
          <select
            id={id}
            className="input input--sm"
            value={value}
            onChange={(e) => onChange(e.target.value)}
          >
            <option value={NONE}>No milestone</option>
            {view.items.map((m) => (
              <option key={m.number} value={String(m.number)}>
                {m.title}
              </option>
            ))}
          </select>
        </div>
      );
  }
}

/** NAW-FR-ZOAS: the root Type control, or the notice that stands in its place. */
function TypeField({
  view,
  value,
  onChange,
}: {
  view: ListView<{ name: string }>;
  value: string;
  onChange: (next: string) => void;
}) {
  switch (view.kind) {
    case "loading":
      return <Notice>Loading issue Types…</Notice>;
    case "unavailable":
      return (
        <Notice>
          {`Issue Types are unavailable: ${view.error} The issue publishes without a Type.`}
        </Notice>
      );
    case "empty":
      return <Notice>No issue Type is available. The issue publishes without a Type.</Notice>;
    default:
      return (
        <div className="draft-publication-chooser__field">
          <label className="t-ui-sm" htmlFor="publication-root-type">
            Type
          </label>
          <select
            id="publication-root-type"
            className="input input--sm"
            value={value}
            onChange={(e) => onChange(e.target.value)}
          >
            <option value={NONE}>No Type</option>
            {view.items.map((t) => (
              <option key={t.name} value={t.name}>
                {t.name}
              </option>
            ))}
          </select>
        </div>
      );
  }
}

/** NAW-FR-YLKD: what a parent row says about its issue. */
function parentSummary(parent: PublicationParentIssue): string {
  return `${parent.issueType} · ${parent.milestone ? parent.milestone.title : "No milestone"}`;
}

export function PublicationChooser({
  state,
  busy,
  onCancel,
  onConfirm,
}: {
  state: PublicationChooserState;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (choice: PublicationChoiceInput) => void;
}) {
  // NAW-FR-TCQB: the root option is selected when the chooser opens.
  const [selected, setSelected] = useState<string>(ROOT);
  const [rootType, setRootType] = useState(NONE);
  const [rootMilestone, setRootMilestone] = useState(NONE);
  const [subMilestone, setSubMilestone] = useState(NONE);

  const parents = viewOf(state, (m) => m.parents);
  const types = viewOf(state, (m) => m.issueTypes);
  const milestones = viewOf(state, (m) => m.milestones);
  const metadata = state.metadata;
  const policy = metadata?.settings.subIssueMilestonePolicy ?? null;
  const parent =
    selected === ROOT || parents.kind !== "loaded"
      ? null
      : (parents.items.find((p) => String(p.number) === selected) ?? null);
  const repository = metadata
    ? `${metadata.repositoryOwner}/${metadata.repositoryName}`
    : state.remoteName;

  /** NAW-FR-RBTE: the choice as the backend receives it. */
  const choice = (): PublicationChoiceInput => {
    if (parent) {
      return {
        parentIssueNumber: parent.number,
        issueType: null,
        milestoneNumber:
          policy === "author_selected" && subMilestone !== NONE
            ? Number(subMilestone)
            : null,
      };
    }
    return {
      parentIssueNumber: null,
      issueType: rootType === NONE ? null : rootType,
      milestoneNumber: rootMilestone === NONE ? null : Number(rootMilestone),
    };
  };

  return (
    <Modal label="Publish to GitHub" onClose={onCancel} returnFocus={actionTrigger()}>
      <>
        <div className="modal__head">
          <div className="modal__title">Publish to GitHub</div>
        </div>
        <div className="modal__body">
          <p className="t-ui-sm">
            Repository: <span className="draft-publication-picker__name">{repository}</span>
          </p>
          <div
            role="radiogroup"
            aria-label="Publish as"
            className="draft-publication-chooser__options"
          >
            <label className="t-ui-sm draft-publication-chooser__option">
              <input
                type="radio"
                name="publication-parent"
                value={ROOT}
                checked={selected === ROOT}
                onChange={() => setSelected(ROOT)}
              />
              <span className="draft-publication-picker__name">Publish as a root issue</span>
            </label>
            {selected === ROOT && (
              <div className="draft-publication-chooser__detail">
                <TypeField view={types} value={rootType} onChange={setRootType} />
                <MilestoneField
                  view={milestones}
                  value={rootMilestone}
                  onChange={setRootMilestone}
                  id="publication-root-milestone"
                />
              </div>
            )}

            {parents.kind === "loading" && <Notice>Loading parent issues…</Notice>}
            {parents.kind === "unavailable" && (
              <Notice>
                {`Parent issues are unavailable: ${parents.error} Root publication is still available.`}
              </Notice>
            )}
            {parents.kind === "empty" && (
              <Notice>
                {`No open issue of ${
                  metadata?.settings.parentIssueTypes.join(", ") ?? "the configured Types"
                } was found. Root publication is still available.`}
              </Notice>
            )}
            {parents.kind === "loaded" &&
              parents.items.map((p) => (
                <div key={p.number}>
                  <label className="t-ui-sm draft-publication-chooser__option">
                    <input
                      type="radio"
                      name="publication-parent"
                      value={String(p.number)}
                      checked={selected === String(p.number)}
                      onChange={() => setSelected(String(p.number))}
                    />
                    <span>
                      <span className="draft-publication-picker__name">{`#${p.number} ${p.title}`}</span>
                      <span className="t-ui-xs draft-publication-chooser__summary">
                        {parentSummary(p)}
                      </span>
                      <span className="t-meta draft-publication-picker__url">{p.url}</span>
                    </span>
                  </label>
                  {selected === String(p.number) && metadata && (
                    <div className="draft-publication-chooser__detail">
                      <SubIssueDetail
                        metadata={metadata}
                        parent={p}
                        milestones={milestones}
                        milestone={subMilestone}
                        onMilestone={setSubMilestone}
                      />
                    </div>
                  )}
                </div>
              ))}
          </div>
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={
              busy || state.status === "loading" || (selected !== ROOT && parent === null)
            }
            onClick={() => onConfirm(choice())}
          >
            Publish
          </button>
        </div>
      </>
    </Modal>
  );
}

/** NAW-FR-JXDN: the configured Type and milestone policy of a sub-issue. */
function SubIssueDetail({
  metadata,
  parent,
  milestones,
  milestone,
  onMilestone,
}: {
  metadata: PublicationMetadata;
  parent: PublicationParentIssue;
  milestones: ListView<PublicationMilestone>;
  milestone: string;
  onMilestone: (next: string) => void;
}) {
  const type = metadata.subIssueType;
  const policy = metadata.settings.subIssueMilestonePolicy;
  return (
    <>
      {type.resolved ? (
        <p className="t-ui-sm draft-publication-chooser__fixed">
          {`Type: ${type.resolved} (set in Project settings)`}
        </p>
      ) : (
        <Notice>
          {`The configured Type “${type.name}” is unavailable in this repository. The issue publishes without a Type.`}
        </Notice>
      )}
      {policy === "inherit_parent" && (
        <p className="t-ui-sm draft-publication-chooser__fixed">
          {parent.milestone
            ? `Milestone: inherited from the parent, ${parent.milestone.title}`
            : "The parent has no milestone, so the issue publishes without one."}
        </p>
      )}
      {policy === "no_milestone" && (
        <p className="t-ui-sm draft-publication-chooser__fixed">
          Milestone: none (set in Project settings)
        </p>
      )}
      {policy === "author_selected" && (
        <MilestoneField
          view={milestones}
          value={milestone}
          onChange={onMilestone}
          id="publication-sub-milestone"
        />
      )}
    </>
  );
}

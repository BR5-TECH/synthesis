/**
 * The publication chooser — the modal that decides whether a draft is published
 * as a root issue or as a sub-issue, and with which Type and milestone
 * (`NAW-new-artifact.md` NAW-FR-HNVR … NAW-FR-PDXJ, served by
 * `../../specifications/core/GHP-github-publication.md` GHP-FR-MDLD).
 *
 * The parent issues are one filterable combobox under the sub-issue option
 * rather than one row each, so a repository with many parents never grows the
 * chooser past the window (NAW-FR-YLKD, NAW-FR-XTGP).
 *
 * Three lists feed it — the parent issues, the issue Types, and the
 * milestones — and each renders its own loading, empty, unavailable, or loaded
 * state (NAW-FR-WMEP). None of them gates the root option: a list that cannot
 * be read costs the author the choices that needed it and nothing else
 * (NAW-FR-TCQB, GHP-FR-UXOT).
 *
 * The sub-issue option comes first and is the option a loaded parent list
 * selects; the root option follows it and is selected while there is no parent
 * to choose (NAW-FR-TCQB, NAW-FR-ASYN).
 *
 * The lists can change while the chooser is open, when the backend refresh
 * ends (NAW-FR-HRXE). A choice that the new list no longer holds is cleared,
 * and every other choice stays (NAW-FR-VOTL, NAW-FR-LETN).
 */
import { useEffect, useMemo, useState } from "react";
import { Modal } from "./DraftsPanelParts";
import { FilterableSelect, type FilterableOption } from "./FilterableSelect";
import type { PublicationChooserState } from "../hooks/useDraftPublication";
import type {
  MetadataList,
  PublicationChoiceInput,
  PublicationMetadata,
  PublicationMilestone,
  PublicationParentIssue,
} from "../types";

/** The two options of the radio group (NAW-FR-TCQB, NAW-FR-YLKD). */
const ROOT = "root";
const SUB = "sub";
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

/**
 * The ids a list holds, or `null` while it is loading or unavailable, so a
 * choice is kept until a read says what the list holds.
 */
function idsOf<T>(view: ListView<T>, id: (item: T) => string): string[] | null {
  if (view.kind === "empty") return [];
  return view.kind === "loaded" ? view.items.map(id) : null;
}

/**
 * NAW-FR-VOTL / NAW-FR-LETN: clear `value` to `NONE` where the list no longer
 * holds it.
 */
function useClearedWhenMissing(
  value: string | null,
  ids: string[] | null,
  clear: () => void,
): void {
  const missing = value !== null && value !== NONE && ids !== null && !ids.includes(value);
  useEffect(() => {
    if (missing) clear();
  }, [missing, clear]);
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

/** NAW-FR-YLKD: what a parent option says about its issue. */
function parentSummary(parent: PublicationParentIssue): string {
  return `${parent.issueType} · ${parent.milestone ? parent.milestone.title : "No milestone"}`;
}

/** NAW-FR-YLKD: one option of the parent combobox. */
function parentOption(parent: PublicationParentIssue): FilterableOption {
  return {
    id: String(parent.number),
    label: `#${parent.number} ${parent.title}`,
    detail: parentSummary(parent),
  };
}

/**
 * NAW-FR-YELA: the filter matches the issue number and the title, as a
 * case-insensitive substring. The number matches with or without a leading
 * `#`. The Type and the milestone are not matched.
 */
export function matchesParent(
  parents: readonly PublicationParentIssue[],
): (option: FilterableOption, filter: string) => boolean {
  const titles = new Map(parents.map((p) => [String(p.number), p.title.toLowerCase()]));
  return (option, filter) => {
    const needle = filter.trim().toLowerCase();
    if (!needle) return true;
    const number = needle.startsWith("#") ? needle.slice(1) : needle;
    return option.id.includes(number) || (titles.get(option.id) ?? "").includes(needle);
  };
}

/** NAW-FR-PDXJ: the polite status of the parent filter. */
function parentAnnouncement(filter: string, count: number): string {
  const issues = count === 1 ? "1 issue" : `${count} issues`;
  const trimmed = filter.trim();
  return trimmed ? `${issues} match “${trimmed}”` : `${issues} available`;
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
  // NAW-FR-ASYN: the option the author selected, or `null` while they have
  // selected none and the chooser decides.
  const [picked, setPicked] = useState<string | null>(null);
  // NAW-FR-ASYN: the issue chosen in the combobox, kept while the root option
  // is selected.
  const [parentNumber, setParentNumber] = useState(NONE);
  // NAW-FR-ZOAS: the Type the author chose, or `null` for the pre-filled one.
  const [typePicked, setTypePicked] = useState<string | null>(null);
  const [rootMilestone, setRootMilestone] = useState(NONE);
  const [subMilestone, setSubMilestone] = useState(NONE);
  const clearParent = useMemo(() => () => setParentNumber(NONE), []);
  const clearType = useMemo(() => () => setTypePicked(NONE), []);
  const clearRootMilestone = useMemo(() => () => setRootMilestone(NONE), []);
  const clearSubMilestone = useMemo(() => () => setSubMilestone(NONE), []);

  const parents = viewOf(state, (m) => m.parents);
  const types = viewOf(state, (m) => m.issueTypes);
  const milestones = viewOf(state, (m) => m.milestones);
  const metadata = state.metadata;
  // NAW-FR-TCQB / NAW-FR-ASYN: the root option while there is no parent to
  // choose; otherwise the sub-issue option, unless the author chose the root
  // option during the read.
  const selected = parents.kind !== "loaded" ? ROOT : (picked ?? SUB);
  const setSelected = (option: string) => setPicked(option);
  // NAW-FR-ZOAS / GHP-FR-AFEX: the stored root Type in GitHub's spelling, or
  // No Type where none resolves.
  const rootType = typePicked ?? metadata?.rootIssueType?.resolved ?? NONE;
  const setRootType = (name: string) => setTypePicked(name);
  const policy = metadata?.settings.subIssueMilestonePolicy ?? null;
  const parentItems = parents.kind === "loaded" ? parents.items : null;
  // Built once per read, so the combobox does not filter again on every
  // render of the chooser.
  const parentOptions = useMemo(() => (parentItems ?? []).map(parentOption), [parentItems]);
  const parentMatcher = useMemo(() => matchesParent(parentItems ?? []), [parentItems]);
  const parent =
    selected !== SUB || parents.kind !== "loaded"
      ? null
      : (parents.items.find((p) => String(p.number) === parentNumber) ?? null);
  const repository = metadata
    ? `${metadata.repositoryOwner}/${metadata.repositoryName}`
    : state.remoteName;
  // NAW-FR-VOTL: a parent the new list no longer holds is no longer chosen.
  useClearedWhenMissing(parentNumber, idsOf(parents, (p) => String(p.number)), clearParent);
  // NAW-FR-LETN: a Type or milestone the new list no longer holds resets to
  // No Type or No milestone.
  useClearedWhenMissing(typePicked, idsOf(types, (t) => t.name), clearType);
  const milestoneIds = idsOf(milestones, (m) => String(m.number));
  useClearedWhenMissing(rootMilestone, milestoneIds, clearRootMilestone);
  useClearedWhenMissing(subMilestone, milestoneIds, clearSubMilestone);

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
                value={SUB}
                checked={selected === SUB}
                // NAW-FR-ASYN: offered only while there is a parent to choose.
                disabled={parents.kind !== "loaded"}
                onChange={() => setSelected(SUB)}
              />
              <span className="draft-publication-picker__name">Publish as a sub-issue of</span>
            </label>
            <div className="draft-publication-chooser__detail">
              {/* NAW-FR-WMEP: a notice stands in place of the combobox. */}
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
              {parents.kind === "loaded" && (
                <FilterableSelect
                  options={parentOptions}
                  value={parentNumber}
                  // NAW-FR-ASYN: a choice selects the sub-issue option, also a
                  // re-pick of the issue kept while the root option is selected.
                  alwaysNotify
                  onChange={(id) => {
                    setParentNumber(id);
                    setSelected(SUB);
                  }}
                  label="Parent issue"
                  placeholder="Choose a parent issue"
                  emptyText="No matching issue"
                  matches={parentMatcher}
                  announce={parentAnnouncement}
                  fitWindow
                  testId="publication-parent"
                />
              )}
              {selected === SUB && metadata && (
                <SubIssueDetail
                  metadata={metadata}
                  parent={parent}
                  milestones={milestones}
                  milestone={subMilestone}
                  onMilestone={setSubMilestone}
                />
              )}
            </div>

            {/* NAW-FR-TCQB: the root option follows the sub-issue option. */}
            <label className="t-ui-sm draft-publication-chooser__option">
              <input
                type="radio"
                name="publication-parent"
                value={ROOT}
                checked={selected === ROOT}
                onChange={() => setSelected(ROOT)}
                // NAW-FR-ASYN: during the read the root option is already
                // checked, so a click on it fires no change. The click keeps it
                // selected once the parent list loads.
                onClick={() => setSelected(ROOT)}
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
          </div>
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            // NAW-FR-RBTE: the read and the refresh must end, and the
            // sub-issue option needs a chosen parent.
            disabled={
              busy ||
              state.status === "loading" ||
              state.refreshing ||
              (selected === SUB && parent === null)
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
  /** The chosen parent, or `null` while none is chosen. */
  parent: PublicationParentIssue | null;
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
          {parent === null
            ? "Milestone: inherited from the parent issue you choose."
            : parent.milestone
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

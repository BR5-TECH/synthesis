/**
 * MIV-FR-VHDP / MIV-FR-NBHF: the sidebar of the issue view: the fields of the
 * item, each under an uppercase label, in a fixed order, and **Remove** at the
 * end. A field that does not apply to the item or to the source is absent.
 */
import type { ReactNode } from "react";

import { allowedKinds, isContainerKind, type FlowItem } from "../../../state/flowDocument";
import { BoardStatus, KindChip } from "../boardItems";
import { boardStatusWord } from "../boardStatus";
import { kindLabel } from "../kindGlyphs";
import { DraftSection, FootprintSection, ISSUE_LOOK, ReferencesSection } from "../panelSections";
import type { IssueViewProps } from "./IssueView";

/** MIV-FR-PGEC: one plan item that blocks the item. */
export interface BlockerRow {
  id: string;
  name: string;
  kind: FlowItem["kind"];
  /** The `#number` of the item, where it has a tracker tag. */
  number?: number;
}

/** One field of the sidebar under its uppercase label. */
export function SideField({
  label,
  children,
  testId,
}: {
  label: string;
  children: ReactNode;
  testId?: string;
}) {
  return (
    <section className="miv-field" aria-label={label} data-testid={testId}>
      <h3 className="miv-field__label">{label}</h3>
      <div className="miv-field__value">{children}</div>
    </section>
  );
}

export function IssueSidebar(props: IssueViewProps) {
  const { doc, item, facts, inert, handlers } = props;
  const leaf = !isContainerKind(item.kind);
  const tracker = facts.tracker?.facts;
  const isIssue = tracker?.ref.kind === "issue";
  const kinds = allowedKinds(doc, item.id).filter(
    (k) => k === item.kind || (props.kindFilter?.(k) ?? true),
  );
  const external = tracker?.externalBlockers ?? [];
  const hasStatus = boardStatusWord({ tracker, state: facts.state?.state }) !== null;
  const assignees = tracker?.assignees ?? [];

  return (
    <aside className="miv-side" aria-label="Fields" data-testid="miv-side">
      <fieldset className="miv-side__fields" disabled={inert}>
        {hasStatus && (
          <SideField label="Status" testId="miv-field-status">
            <BoardStatus facts={facts} />
          </SideField>
        )}
        {isIssue && (
          <SideField label="Assignees" testId="miv-field-assignees">
            {assignees.length === 0 ? (
              <span className="miv-none">None</span>
            ) : (
              <span className="miv-assignees">
                {assignees.map((login) => (
                  <span key={login} className="miv-assignee">
                    @{login}
                  </span>
                ))}
              </span>
            )}
          </SideField>
        )}
        {props.parts.details}
        <SideField label="Kind" testId="miv-field-kind">
          <select
            id={`flow-kind-${item.id}`}
            className={ISSUE_LOOK.control("flow-select")}
            aria-label="Kind"
            disabled={inert}
            value={item.kind}
            onChange={(e) => handlers.onKind(e.target.value as FlowItem["kind"])}
          >
            {kinds.map((k) => (
              <option key={k} value={k}>
                {kindLabel(k)}
              </option>
            ))}
          </select>
        </SideField>
        {item.kind === "milestone" && (
          <SideField label="Due" testId="miv-field-due">
            <input
              id={`flow-due-${item.id}`}
              className={ISSUE_LOOK.control("flow-input")}
              type="date"
              aria-label="Due"
              disabled={inert}
              value={item.due ?? ""}
              onChange={(e) => handlers.onDue(e.target.value)}
            />
          </SideField>
        )}
        {props.blockers.length + external.length > 0 && (
          <SideField label="Blocked by" testId="miv-field-blocked">
            <ul className="miv-rows">
              {props.blockers.map((b) => (
                <li key={b.id}>
                  <button
                    type="button"
                    className="miv-row"
                    aria-label={`Open ${b.number !== undefined ? `#${b.number} ` : ""}${b.name}`}
                    onClick={() => props.onShowItem(b.id)}
                  >
                    <KindChip kind={b.kind} />
                    {b.number !== undefined && <span className="miv-row__ref">#{b.number}</span>}
                    <span className="miv-row__name">{b.name || "Unnamed"}</span>
                  </button>
                </li>
              ))}
              {external.map((b) => (
                <li key={`external-${b.number}`}>
                  <button
                    type="button"
                    className="miv-row miv-row--link"
                    aria-label={`Open #${b.number} ${b.title} on GitHub`}
                    onClick={() => props.onOpenTracker(b.url)}
                  >
                    <span className="miv-row__ref">#{b.number}</span>
                    <span className="miv-row__name">{b.title}</span>
                  </button>
                </li>
              ))}
            </ul>
          </SideField>
        )}
        {leaf && (
          <>
            <DraftSection
              doc={doc}
              item={item}
              state={facts.state}
              inert={inert}
              creating={props.creating}
              look={ISSUE_LOOK}
              handlers={handlers}
              className="miv-field"
            />
            <FootprintSection
              item={item}
              paths={props.paths}
              treeLoaded={props.treeLoaded}
              inert={inert}
              look={ISSUE_LOOK}
              handlers={handlers}
              className="miv-field"
            />
          </>
        )}
        <ReferencesSection
          item={item}
          references={props.references}
          artifacts={props.artifacts}
          inert={inert}
          handlers={handlers}
          className="miv-field"
        />
        {props.parts.specs}
        <div className="miv-side__foot">
          <button type="button" className={ISSUE_LOOK.danger} disabled={inert} onClick={handlers.onRemove}>
            Remove
          </button>
        </div>
      </fieldset>
    </aside>
  );
}

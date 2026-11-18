/**
 * MIV-FR-XMMS: the main column of the issue view: the title with **Rename**,
 * the error text, the description, and what the host adds below it.
 */
import { useEffect, useRef, useState } from "react";

import { DescriptionSection, ISSUE_LOOK } from "../panelSections";
import type { IssueViewProps } from "./IssueView";
import { IssueMarkdown } from "./IssueMarkdown";

/**
 * MIV-FR-XMMS / MIV-FR-SWZY / MIV-FR-BEZE: the title, renamed in place. Enter
 * and a blur keep the new name, Escape cancels it, and a close of the view
 * keeps a rename in progress.
 */
function TitleField({
  name,
  onRename,
  onEnd,
}: {
  name: string;
  onRename: (name: string) => void;
  onEnd: () => void;
}) {
  const [text, setText] = useState(name);
  const latest = useRef({ text, name, onRename, done: false });
  latest.current = { ...latest.current, text, name, onRename };
  const finish = (keep: boolean) => {
    if (latest.current.done) return;
    latest.current.done = true;
    const value = latest.current.text.trim();
    if (keep && value !== "" && value !== latest.current.name) latest.current.onRename(value);
    onEnd();
  };
  useEffect(() => {
    // StrictMode runs the effect twice: a simulated unmount must not end the field.
    latest.current.done = false;
    return () => {
      const l = latest.current;
      if (l.done) return;
      l.done = true;
      const value = l.text.trim();
      if (value !== "" && value !== l.name) l.onRename(value);
    };
  }, []);
  return (
    <input
      className="miv-title miv-title--edit input"
      aria-label="Item name"
      autoFocus
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(true);
        if (e.key === "Escape") {
          e.stopPropagation();
          finish(false);
        }
      }}
    />
  );
}

export function IssueMain(props: IssueViewProps) {
  const { item, facts, inert } = props;
  const [renaming, setRenaming] = useState(false);
  // MIV-FR-YPWK: when a rename ends, the focus goes back to the view, so Escape
  // still reaches it. Not to Rename: the Enter that ended the rename would
  // press it again.
  const renameButton = useRef<HTMLButtonElement>(null);
  const ended = useRef(false);
  useEffect(() => {
    if (renaming || !ended.current) return;
    ended.current = false;
    renameButton.current?.closest<HTMLElement>(".miv-sheet")?.focus({ preventScroll: true });
  }, [renaming]);
  return (
    <div className="miv-main" data-testid="miv-main">
      <div className="miv-main__inner">
        <div className="miv-title-row">
          {renaming && !inert ? (
            <TitleField
              name={item.name}
              onRename={props.onRenameTo}
              onEnd={() => {
                ended.current = true;
                setRenaming(false);
              }}
            />
          ) : (
            <h2 className="miv-title" data-testid="miv-title">
              {item.name || <span className="flow-placeholder">Unnamed</span>}
            </h2>
          )}
          {!renaming && (
            <button
              ref={renameButton}
              type="button"
              className={ISSUE_LOOK.ghost}
              disabled={inert}
              onClick={() => setRenaming(true)}
            >
              Rename
            </button>
          )}
        </div>
        {facts.error && (
          <div className="flow-panel__error miv-error" role="alert" data-testid="flow-panel-error">
            {facts.error}
          </div>
        )}
        <DescriptionSection
          item={item}
          inert={inert}
          describeOnBlur={props.describeOnBlur}
          onDescribe={props.handlers.onDescribe}
          look={ISSUE_LOOK}
          rows={14}
          fieldClassName="flow-panel__src miv-src input"
          className="miv-section miv-desc"
          onEditingChange={() => undefined}
          renderBody={(markdown) => (
            <IssueMarkdown body={markdown} testId={`flow-item-description-${item.id}`} />
          )}
        />
        {props.parts.below}
      </div>
    </div>
  );
}

/**
 * NAW-FR-41 / DRS-FR-15: the state an inconsistent draft renders. It names the
 * inconsistency, offers no editing surface and no rail, presents no file of the
 * draft as the prompt, and states that the draft is deleted from the Drafts
 * panel — that being the one thing left to do with it.
 */
import { Icon } from "../icons";

export function InconsistentDraft({ name }: { name: string }) {
  return (
    <div className="draft-workspace">
      <div className="draft-workspace__chrome">
        <span className="draft-workspace__name">
          <Icon.Diamond size={12} /> {name}
        </span>
      </div>
      <div className="draft-workspace__blocked" role="alert">
        <h2 className="t-ui-sm">This draft cannot be opened.</h2>
        <p className="t-ui-xs">
          A draft is one Markdown prompt, and this one’s storage holds
          something else. Nothing in it has been changed, and no file of it has
          been chosen as the prompt.
        </p>
        <p className="t-ui-xs">
          Delete it from the Drafts panel, which is the one thing left to do
          with it.
        </p>
      </div>
    </div>
  );
}

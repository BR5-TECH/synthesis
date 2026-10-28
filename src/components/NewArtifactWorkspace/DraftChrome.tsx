/**
 * The chrome row of the New Artifact tab (`NAW-new-artifact.md` NAW-FR-05): the
 * rail's toggle, the draft's name, the tags beside it, and the split controls.
 * Split out of `index.tsx` to keep that file within the size rule.
 */
import { undecidedCount } from "../../types";
import type {
  DraftChangeProposal,
  DraftGraduation,
  GithubIssueLink,
} from "../../types";
import type { UseDraftPublicationResult } from "../../hooks/useDraftPublication";
import type { DiscussionColumnState } from "../../state/discussionColumn";
import { setFocusedHunk } from "../../state/draftDiscussion";
import { openReview, undecidedHunks } from "../../state/draftProposals";
import { stateLabel } from "../../state/graduation";
import { Icon } from "../icons";
import { SplitControls, type SplitRatio } from "../DraftDiscussion";
import { DraftPublicationTag } from "../DraftPublication";
import { GithubShadowTag } from "./GithubShadow";
import type { DraftRename } from "./useDraftRename";

export function DraftChrome({
  draftId,
  name,
  railShown,
  versionCount,
  onToggleRail,
  rename,
  shadow,
  shadowIssue,
  archived,
  graduation,
  onOpenRun,
  publication,
  pendingProposal,
  view,
  onRatioChange,
  onToggleDiscussion,
}: {
  draftId: string;
  name: string;
  railShown: boolean;
  versionCount: number | null;
  onToggleRail: () => void;
  rename: DraftRename;
  shadow: boolean;
  shadowIssue: GithubIssueLink | null;
  archived: boolean;
  graduation: DraftGraduation | null;
  onOpenRun: (runId: string) => void;
  publication: UseDraftPublicationResult;
  pendingProposal: DraftChangeProposal | undefined;
  view: DiscussionColumnState;
  onRatioChange: (ratio: SplitRatio) => void;
  onToggleDiscussion: () => void;
}) {
  return (
    /* NAW-FR-05: the tab's chrome carries the draft's name and the rail's
       toggle, and nothing else. There is no permanently open text field, no
       lifecycle button, and no status control here — every action a draft
       affords is behind the one floating control (NAW-FR-27). */
    <div className="draft-workspace__chrome" data-testid="draft-workspace-chrome">
      {/* NAW-FR-08: the rail's toggle sits at the head of this row and carries
          the number of versions the draft holds, so the count is legible
          without showing the rail. Placed here rather than beside the editing
          surface so that a hidden rail costs the surface no width at all: the
          whole rail goes, and no gutter is left behind to hold this. */}
      <button
        className="draft-rail__toggle"
        aria-expanded={railShown}
        aria-label={railShown ? "Hide History" : "Show History"}
        title={
          versionCount === null
            ? "Version history"
            : `${versionCount} ${
                versionCount === 1 ? "version" : "versions"
              } of this draft`
        }
        onClick={onToggleRail}
      >
        <Icon.History size={13} />
        {versionCount !== null && (
          <span className="draft-rail__count">{versionCount}</span>
        )}
      </button>
      {rename.renaming ? (
        <span className="draft-workspace__rename">
          <input
            className="input input--sm"
            aria-label="Draft name"
            autoFocus
            value={rename.nameText}
            onChange={(e) => rename.edit(e.target.value)}
            // A field already showing a refusal abandons the rename on blur
            // rather than re-issuing it, so the message never outlives the
            // field it belongs to.
            onBlur={() => (rename.nameError ? rename.cancel() : void rename.commit())}
            onKeyDown={(e) => {
              if (e.key === "Enter") void rename.commit();
              if (e.key === "Escape") rename.cancel();
            }}
          />
          {rename.nameError && (
            <p className="t-ui-xs draft-workspace__name-error" role="alert">
              {rename.nameError}
            </p>
          )}
        </span>
      ) : (
        <button
          className="draft-workspace__name"
          // NAW-FR-UEWC: the in-place rename is inert on a shadow draft.
          onClick={() => !shadow && rename.start()}
          aria-disabled={shadow || undefined}
          title={
            shadow
              ? "This draft mirrors a GitHub issue, so it cannot be renamed"
              : "Rename this draft"
          }
        >
          <Icon.Diamond size={12} /> {name}
        </button>
      )}
      {/* NAW-FR-16: an archived draft is marked beside its label, because it
          opens and edits exactly as an active one does and nothing else in
          the tab would say which it is. */}
      {archived && (
        <span className="draft-workspace__archived t-ui-xs">Archived</span>
      )}
      {/* NAW-FR-BJQX: the run's state, as a tag beside the draft's label, and as
          the route to it. A run is where an author acts on a graduation
          (GRU-FR-MYFA), so this says which one holds the prompt and takes them
          there rather than trying to be a second run surface. */}
      {graduation && (
        <button
          className="draft-workspace__graduation t-ui-xs"
          data-testid="draft-graduation-state"
          title="Go to this draft's graduation run"
          onClick={() => onOpenRun(graduation.runId)}
        >
          <Icon.Diamond size={11} /> {stateLabel(graduation.state)}
        </button>
      )}
      {/* NAW-FR-CBUJ: the draft's publication, as a tag beside the draft's
          label on the same terms as the archived marker and the run state. A
          tag that names a record is the route to that issue (NAW-FR-XRLD);
          the records themselves are read in Draft Information. */}
      <DraftPublicationTag publication={publication} />
      {/* NAW-FR-BCHZ: a shadow draft's tag names its repository and issue
          number, and opens the issue. */}
      {shadow && shadowIssue && (
        <GithubShadowTag draftId={draftId} issue={shadowIssue} />
      )}
      {/* NAW-FR-35 / DCR-FR-03: a draft carrying an undecided proposal says
          so, and the marker takes the author to the first change still to be
          decided. It clears the moment the proposal is resolved.

          The live dot is the one thing on this surface that animates, and it
          stops under a reduced-motion preference. */}
      {pendingProposal !== undefined && (
        <button
          className="draft-workspace__proposal t-ui-xs"
          data-testid="draft-pending-proposal"
          title={`Review the ${undecidedCount(pendingProposal.counts)} change${
            undecidedCount(pendingProposal.counts) === 1 ? "" : "s"
          } proposed to ${pendingProposal.path}`}
          onClick={() => {
            const first = undecidedHunks(pendingProposal)[0];
            if (first) setFocusedHunk(draftId, first.id);
            openReview(pendingProposal.id);
          }}
        >
          <span className="dot dot--live" aria-hidden="true" />
          {undecidedCount(pendingProposal.counts)} proposed{" "}
          {undecidedCount(pendingProposal.counts) === 1 ? "change" : "changes"}
        </button>
      )}

      <div className="spacer" />

      <SplitControls
        ratio={view.ratio}
        hidden={view.hidden}
        onRatioChange={onRatioChange}
        onToggle={onToggleDiscussion}
        toggleTestId="draft-discussion-toggle"
      />
    </div>
  );
}

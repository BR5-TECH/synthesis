/**
 * The words that state a saved publication choice
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-NHAY, NAW-FR-QEZG;
 * `DFI-draft-information.md` DFI-FR-BZQN).
 *
 * One function, so the band of a standing attempt and a record of Draft
 * Information say the same thing about the same choice.
 */
import { tlsErrorMessage } from "../tlsError";
import {
  GITHUB_HOST_MISMATCH_MESSAGE,
  type PublicationChoice,
  type PublicationMismatch,
} from "../types";

/**
 * A choice in words: root or sub-issue, then the Type and the milestone where
 * the choice holds them. A record or an attempt that holds no choice reads as a
 * root issue (GHP-FR-CDVT).
 */
export function describePublicationChoice(
  choice: PublicationChoice | undefined,
): string {
  if (!choice || choice.kind === "root") {
    return withMetadata("Root issue", choice);
  }
  const repository =
    choice.parentRepositoryOwner && choice.parentRepositoryName
      ? ` in ${choice.parentRepositoryOwner}/${choice.parentRepositoryName}`
      : "";
  return withMetadata(
    `Sub-issue of #${choice.parentIssueNumber ?? "?"}${repository}`,
    choice,
  );
}

function withMetadata(
  head: string,
  choice: PublicationChoice | undefined,
): string {
  const parts = [head];
  if (choice?.issueType) parts.push(`Type ${choice.issueType}`);
  if (choice?.milestoneNumber !== undefined) {
    parts.push(`Milestone ${choice.milestoneTitle ?? `#${choice.milestoneNumber}`}`);
  }
  return parts.join(" · ");
}

const MISMATCH_LABEL: Record<PublicationMismatch, string> = {
  title: "title",
  body: "body",
  parent: "parent",
  type: "Type",
  milestone: "milestone",
};

/** NAW-FR-EOTB: the differences a recovery choice names. */
export function describeMismatches(mismatches: PublicationMismatch[]): string {
  const names = mismatches.map((m) => MISMATCH_LABEL[m] ?? m);
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/**
 * NAW-FR-NHAY / NAW-FR-HZSW / NAW-FR-KMFT: the displayable text of the typed
 * errors that publishing as a sub-issue and updating an issue add. The code stays in the text, so a log or a
 * report that quotes the band still names it. Every other code renders as it
 * always has.
 */
const PUBLICATION_ERROR_TEXT: Record<string, string> = {
  github_host_mismatch: GITHUB_HOST_MISMATCH_MESSAGE,
  parent_issue_unavailable:
    "The parent issue is missing, closed, or no longer usable. Abandon any standing attempt and publish again with another parent.",
  issue_type_unavailable: "GitHub does not offer the selected issue Type.",
  milestone_unavailable: "The selected milestone is not open any more.",
  invalid_publication_choice:
    "That combination of parent, Type, and milestone is not allowed by Project settings.",
  sub_issue_link_failed:
    "The issue was created, but GitHub did not link it to its parent. Use Retry, then choose Update existing issue.",
  parent_issues_unreadable: "The open issues of this repository could not be read.",
  issue_types_unreadable: "The issue Types of this repository could not be read.",
  milestones_unreadable: "The milestones of this repository could not be read.",
  invalid_publication_settings:
    "The publication settings need at least one parent issue Type and a sub-issue Type.",
  // GHP-FR-MZMW / GHP-FR-SVXP: the failures of Update on GitHub.
  no_publication_record: "This draft has no published issue to update.",
  issue_not_found:
    "GitHub does not find the published issue. It was deleted or moved, or the token cannot read it.",
  issue_moved: "The published issue moved to another repository, so it was not updated.",
  issue_update_forbidden: "The token cannot read or edit the published issue.",
};

/**
 * NAW-FR-KMFT: codes that the band of a publication shows as they are, but that
 * the toast of a failed update states in words.
 */
const UPDATE_ERROR_TEXT: Record<string, string> = {
  issue_update_failed: "GitHub did not accept the edit of the published issue.",
  github_unreachable: "GitHub could not be reached.",
  publication_store_write_failed:
    "The issue was edited, but the publication record could not be written.",
  attempt_in_progress: "A publication attempt for this draft is already in progress.",
};

export function publicationErrorText(raw: string): string {
  // AAP-FR-LRTC: a refused certificate names its host and its cause.
  const tls = tlsErrorMessage(raw);
  if (tls) return tls;
  const text = raw.startsWith("Error: ") ? raw.slice(7) : raw;
  const known = PUBLICATION_ERROR_TEXT[text.trim()];
  return known ? `${known} (${text.trim()})` : raw;
}

/** NAW-FR-KMFT: the displayable text of a failed **Update on GitHub**. */
export function updateErrorText(raw: string): string {
  const text = (raw.startsWith("Error: ") ? raw.slice(7) : raw).trim();
  const known = UPDATE_ERROR_TEXT[text];
  return known ? `${known} (${text})` : publicationErrorText(raw);
}

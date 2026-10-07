/**
 * The shapes the Create a PR window shares with the surfaces that open it
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-JOIG,
 * CPR-FR-VZUZ).
 */

/** CPR-FR-JOIG: what an opener knows, and gives the window to start from. */
export interface PullRequestSource {
  /** The head branch: the branch the pull request proposes. Read-only in the window. */
  head: string;
  /** The base branch the window starts with. The author may change it. */
  base: string;
  /** The title the window starts with. */
  title: string;
}

/** CPR-FR-VZUZ: what the author typed, held while the token picker is open. */
export interface PullRequestInput {
  title: string;
  body: string;
  base: string;
  draft: boolean;
}

/** CPR-FR-VZUZ: how the window comes back after the token picker settled. */
export interface PullRequestResume {
  input: PullRequestInput;
  /** A token was chosen: the window submits once more with `input`. */
  submit: boolean;
  /** A token was not chosen: the window says so inline. */
  notice: string | null;
}

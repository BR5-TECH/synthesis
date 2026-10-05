import { logInfo } from "../../logging";
import { GIT_ERRORS, isTokenSelectionRequired } from "./errors";

/**
 * GIT-FR-10, GTC-FR-10: run an operation that reaches GitHub, and answer a
 * `github_token_selection_required` rejection by opening the token picker.
 *
 * The operation runs again once a token is chosen and is abandoned when the
 * author cancels. A `github_token_missing` rejection is rethrown as it is: it
 * renders inline, because there is nothing to choose between (GHA-FR-19).
 */
export async function withGithubToken<T>(
  operation: () => Promise<T>,
  requestToken: (() => Promise<boolean>) | undefined,
): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    if (!isTokenSelectionRequired(error) || !requestToken) throw error;
    logInfo(["frontend", "remote"], "github token selection requested");
    const chosen = await requestToken();
    if (!chosen) {
      logInfo(["frontend", "remote"], "github token selection cancelled");
      throw GIT_ERRORS.tokenSelectionCancelled;
    }
    return operation();
  }
}

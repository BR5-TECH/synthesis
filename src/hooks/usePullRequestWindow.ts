/**
 * The shell's half of the Create a PR window
 * (`../../specifications/ui/CPR-create-pull-request.md`): which source it is
 * open for, what the author typed while the token picker stood in its place,
 * and the notice a created pull request leaves.
 *
 * The window is a floating overlay of the main window, so opening it closes the
 * others (CPR-FR-IWDK) and every other opener closes it through `dismiss`. That
 * is enforced at the opening site, as it is for every overlay of this shell
 * (SNV-FR-56).
 */
import { useEffect, useRef, useState } from "react";

import type {
  PullRequestInput,
  PullRequestResume,
  PullRequestSource,
} from "../components/CreatePullRequest/types";
import { logInfo } from "../logging";
import type { CreatedPullRequest } from "../types";

interface PullRequestWindowState {
  source: PullRequestSource;
  /** A counter, so two openings never share a key. */
  nonce: number;
  /** CPR-FR-IWDK: the control that opened the window, which gets focus back. */
  opener: HTMLElement | null;
  /** CPR-FR-VZUZ: set when the window is mounted again after the picker. */
  resume: PullRequestResume | null;
  /** CPR-FR-VZUZ: the picker stands in the window's place, holding its input. */
  yielded: boolean;
}

export interface PullRequestWindowDeps {
  /** Close every other overlay of the shell (SNV-FR-56). */
  closeOthers: () => void;
  /** Open the token picker; resolves with whether a token was chosen. */
  requestToken: () => Promise<boolean>;
  /** The project path, so a project change removes the notice (CPR-FR-RDJP). */
  projectPath: string | null;
}

export function usePullRequestWindow(deps: PullRequestWindowDeps) {
  const [window, setWindow] = useState<PullRequestWindowState | null>(null);
  const [notice, setNotice] = useState<CreatedPullRequest | null>(null);
  const windowRef = useRef<PullRequestWindowState | null>(null);
  /** CPR-FR-XMRL: a request runs, so nothing may take the window down. */
  const submittingRef = useRef(false);
  /** True while this hook itself opens the picker, which must not clear the record. */
  const yieldingRef = useRef(false);
  const seq = useRef(0);

  const put = (next: PullRequestWindowState | null) => {
    windowRef.current = next;
    setWindow(next);
  };

  /** CPR-FR-FDVO: open the window for `source`, closing every other overlay. */
  const open = (source: PullRequestSource) => {
    deps.closeOthers();
    submittingRef.current = false;
    logInfo(["frontend", "remote"], "create pull request window opened", {
      head: source.head,
      base: source.base,
    });
    const opener = document.activeElement;
    put({
      source,
      nonce: ++seq.current,
      opener: opener instanceof HTMLElement ? opener : null,
      resume: null,
      yielded: false,
    });
  };

  /** CPR-FR-SSQI: close the window; its input is discarded. */
  const close = () => {
    if (submittingRef.current) return;
    put(null);
  };

  /**
   * SNV-FR-56: another overlay is opening. A request that runs holds the window
   * (CPR-FR-XMRL); the picker this hook opened itself leaves the record in
   * place (CPR-FR-VZUZ).
   */
  const dismiss = () => {
    if (yieldingRef.current || submittingRef.current) return;
    if (windowRef.current) put(null);
  };

  /** CPR-FR-VZUZ: give way to the token picker, then come back. */
  const giveWayToPicker = async (input: PullRequestInput) => {
    const current = windowRef.current;
    if (!current) return;
    put({ ...current, yielded: true });
    yieldingRef.current = true;
    let answer: Promise<boolean>;
    try {
      answer = deps.requestToken();
    } finally {
      // A picker that failed to open must not leave every dismissal ignored.
      yieldingRef.current = false;
    }
    const chosen = await answer;
    // Another overlay took the place of the picker, and cleared the record.
    if (windowRef.current?.nonce !== current.nonce) return;
    put({
      ...current,
      nonce: ++seq.current,
      yielded: false,
      resume: {
        input,
        submit: chosen,
        notice: chosen ? null : "No GitHub token was selected, so no pull request was created.",
      },
    });
  };

  /** CPR-FR-ITWJ: the pull request exists. */
  const created = (pr: CreatedPullRequest) => {
    submittingRef.current = false;
    put(null);
    setNotice(pr);
  };

  // CPR-FR-RDJP: a change of the project removes the notice and the window.
  useEffect(() => {
    setNotice(null);
    submittingRef.current = false;
    put(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deps.projectPath]);

  return {
    pullRequestWindow: window && !window.yielded ? window : null,
    pullRequestNotice: notice,
    dismissPullRequestNotice: () => setNotice(null),
    openPullRequestWindow: open,
    closePullRequestWindow: close,
    dismissPullRequestWindow: dismiss,
    pullRequestSubmittingRef: submittingRef,
    giveWayToPicker,
    pullRequestCreated: created,
  };
}

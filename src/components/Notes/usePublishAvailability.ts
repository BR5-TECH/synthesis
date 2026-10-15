/**
 * NTS-FR-FYUJ: whether **Publish to GitHub** is enabled on a note row, read
 * through `"get note publication availability"` (GHP-FR-CNPX) each time the
 * overflow menu opens. While the read is outstanding the entry is disabled.
 */
import { useCallback, useRef, useState } from "react";

import * as api from "../../api";
import { logInfo } from "../../logging";
import { githubPollingErrorMessage, refusalCode } from "../../state/githubPolling";
import type { PublishAvailability } from "./helpers";

export function usePublishAvailability(): [PublishAvailability, () => Promise<void>] {
  const [availability, setAvailability] = useState<PublishAvailability>({
    status: "loading",
  });
  // A menu opened again before the last read answered takes the newer answer.
  const ticket = useRef(0);

  const read = useCallback(async () => {
    const mine = ++ticket.current;
    setAvailability({ status: "loading" });
    let next: PublishAvailability;
    try {
      const answer = await api.getNotePublicationAvailability();
      next =
        answer.available && answer.remoteName
          ? {
              status: "available",
              remoteName: answer.remoteName,
              repository: `${answer.repositoryOwner}/${answer.repositoryName}`,
            }
          : {
              status: "unavailable",
              reason: answer.reason ?? "Publication to GitHub is not available.",
            };
    } catch (e) {
      logInfo(["frontend", "remote"], "note publication availability could not be read", {
        code: refusalCode(e),
      });
      next = { status: "unavailable", reason: githubPollingErrorMessage(e) };
    }
    if (mine === ticket.current) setAvailability(next);
  }, []);

  return [availability, read];
}

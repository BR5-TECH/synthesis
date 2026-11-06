/**
 * What the Create a PR window reads about the head branch and the base
 * branches (`../../../specifications/ui/CPR-create-pull-request.md`
 * CPR-FR-FZHF, CPR-FR-SQGZ, CPR-FR-VYPG, CPR-FR-PLWK).
 */
import { useEffect, useMemo, useRef, useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import type { PullRequestHeadState } from "../../types";
import { parseRejection } from "../Git/errors";
import { baseBranchNames } from "./baseBranches";
import {
  pullRequestBlocks,
  pullRequestHeadReadMessage,
  pullRequestPushNotice,
} from "./messages";

export type HeadRead =
  | { status: "loading" }
  | { status: "ready"; state: PullRequestHeadState }
  | { status: "failed"; message: string };

export function useHeadState(head: string, base: string, mounted: { current: boolean }) {
  const [branches, setBranches] = useState<string[] | null>(null);
  const [read, setRead] = useState<HeadRead>({ status: "loading" });
  const readSeq = useRef(0);

  // CPR-FR-FZHF: the project's branches, by the names GitHub knows them.
  useEffect(() => {
    let live = true;
    api
      .listBranches()
      .then((all) => {
        if (live) setBranches(baseBranchNames(all, head));
      })
      .catch((error) => {
        // A listing that failed says nothing about the base branch, so it is
        // not marked missing: the window offers the base it was given.
        logWarn(["frontend", "remote"], "pull request branch listing failed", {
          error: parseRejection(error).code,
        });
      });
    return () => {
      live = false;
    };
  }, [head]);

  // CPR-FR-SQGZ: read the head branch's state, and again for another base. A
  // response of an older read is discarded (CPR non-functional).
  const readHead = (forBase: string) => {
    const seq = ++readSeq.current;
    setRead({ status: "loading" });
    api
      .getPullRequestHeadState(head, forBase || null)
      .then((state) => {
        if (seq === readSeq.current && mounted.current) setRead({ status: "ready", state });
      })
      .catch((error) => {
        const message = pullRequestHeadReadMessage(error, head, forBase);
        logWarn(["frontend", "remote"], "pull request head state read failed", {
          head,
          base: forBase,
          error: parseRejection(error).code,
        });
        if (seq === readSeq.current && mounted.current) setRead({ status: "failed", message });
      });
  };

  useEffect(() => {
    readHead(base);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base, head]);

  const blocks = useMemo(
    () => (read.status === "ready" ? pullRequestBlocks(read.state) : []),
    [read],
  );
  const pushNotice = useMemo(
    () => (read.status === "ready" ? pullRequestPushNotice(read.state) : null),
    [read],
  );
  const baseMissing = branches !== null && base !== "" && !branches.includes(base);
  const baseOptions = useMemo(() => {
    const list = branches ?? [];
    return list.includes(base) || base === "" ? list : [base, ...list];
  }, [branches, base]);

  return { branches, read, readHead, blocks, pushNotice, baseMissing, baseOptions };
}

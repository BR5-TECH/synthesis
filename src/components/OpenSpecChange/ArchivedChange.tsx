/**
 * The change workspace of an archived change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-AKVG,
 * OCW-FR-GVZT).
 *
 * The read named the change archived (OSM-FR-KWWM), as a stream merge
 * archives it (per `../../../specifications/core/OAR-openspec-archive.md`).
 * The tab states that, names the archive folder, and offers nothing else: no
 * discussion column, no Apply, no Rework…, no History, no Information, no run
 * strip, and no Retry. It reads no proposal, no request and no version.
 *
 * A mount that opens on an archived tab still reads the change once
 * (OCW-FR-LABL). A change of the same name that is active again in the
 * location reads, and the tab then shows it.
 */
import { useEffect } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { keepChangeView } from "../../state/openspec/changeView";
import type { OpenSpecChangeTabTarget } from "../../types";
import { archivedAsOfError, readErrorCode } from "./failure";
import { useFocusOnOpen } from "./focusOnOpen";

export function ArchivedChange({
  tabId,
  target,
  archivedAs,
  readOnMount,
}: {
  tabId: string;
  target: OpenSpecChangeTabTarget;
  archivedAs: string;
  /**
   * OCW-FR-LABL: false where a read of the same mount answered archived and
   * put the statement in place of the workspace. That read is the read of the
   * open.
   */
  readOnMount: boolean;
}) {
  const { location, name } = target;
  const root = useFocusOnOpen<HTMLElement>(target.openSeq);

  useEffect(() => {
    if (!readOnMount) return;
    let live = true;
    api
      .readOpenspecChange(location, name)
      .then((detail) => {
        if (!live) return;
        logInfo(["frontend"], "an archived openspec change is active again", {
          locationKind: location.kind,
        });
        keepChangeView(tabId, { archivedAs: null, detail });
      })
      .catch((e) => {
        if (!live) return;
        const again = archivedAsOfError(e);
        if (again !== null) {
          keepChangeView(tabId, { archivedAs: again });
          return;
        }
        // Any other failure keeps the statement: the change was archived, and
        // nothing read since says otherwise.
        logWarn(["frontend"], "openspec change could not be read", {
          locationKind: location.kind,
          code: readErrorCode(e),
        });
      });
    return () => {
      live = false;
    };
    // The location and the name identify the tab, so they do not change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId]);

  const where =
    location.kind === "stream"
      ? `stream ${target.streamName ?? location.streamId}`
      : "active worktree";

  return (
    <section ref={root} className="openspec-change" data-testid="openspec-change" tabIndex={-1}>
      <header className="openspec-change__head">
        <div className="openspec-change__facts">
          <h2 className="openspec-change__name">{name}</h2>
          <span className="openspec-change__fact" data-testid="openspec-change-location">
            {where}
          </span>
          <span className="openspec-change__fact">archived</span>
        </div>
      </header>
      <div className="openspec-change__reading openspec-change__reading--alone">
        <div className="openspec-change__state" role="status" data-testid="openspec-change-archived">
          <p>The change “{name}” is archived.</p>
          <p>
            Its archive folder is <code>{archivedAs}</code>, in{" "}
            <code>openspec/changes/archive/</code>.
          </p>
        </div>
      </div>
    </section>
  );
}

/**
 * GSD-FR-QMTF: the graduation start dialog as the shell mounts it, for the two
 * openers that are not a draft's own tab — the Graduate entry of a
 * GitHub-shadow row in the Drafts panel (DRP-FR-YYZU) and the Ready tasks
 * section of the Git panel (GIT-FR-NQTZ, GIT-FR-OLNA).
 *
 * The dialog itself is unchanged for a GitHub-shadow draft (GSD-FR-LXAF). This
 * wrapper only reports the moment it has opened, because a claim is
 * acknowledged then and not before (GPP-FR-BSLI).
 */
import { useEffect } from "react";

import { GraduationStart, type GraduationStartProps } from "./GraduationStart";

export interface ShellGraduationStartProps extends GraduationStartProps {
  /** Called once, after the dialog has mounted. */
  onOpened: () => void;
}

export function ShellGraduationStart({
  onOpened,
  ...dialog
}: ShellGraduationStartProps) {
  useEffect(() => {
    onOpened();
    // Mount-only: the dialog opens once per mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return <GraduationStart {...dialog} />;
}

/**
 * The window's one Export dialog (`EXD-export-dialog.md` EXD-FR-XNLZ), mounted
 * only while open. A new opening remounts it, so each opening starts fresh.
 */
import { useExportDialogRequest } from "../../state/exportDialog";
import { ExportDialog } from "./ExportDialog";

export { ExportRequestCard } from "./ExportRequestCard";

export function ExportDialogHost() {
  const opening = useExportDialogRequest();
  if (opening === null) return null;
  return <ExportDialog key={opening.id} opening={opening} />;
}

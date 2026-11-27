/**
 * The toasts a link of a rich Markdown surface shows
 * (`specifications/ui/NTF-notifications.md` NTF-FR-LSKJ, NTF-FR-WQRN).
 */
import type { LinkRefusal } from "../components/markdownLinks";
import { showToast } from "./toasts";

/** NTF-FR-LSKJ: the key of the toast a refused link shows. */
export const LINK_REFUSED_KEY = "link-refused";
/** NTF-FR-WQRN: the key of the toast a link with no target shows. */
export const LINK_TARGET_MISSING_KEY = "link-target-missing";

/** NTF-FR-LSKJ: a Warn toast for a link that opens nothing. */
export function showLinkRefused(reason: LinkRefusal): void {
  showToast(
    {
      key: LINK_REFUSED_KEY,
      level: "Warn",
      title: "Link not followed",
      body:
        reason === "outside"
          ? "The link goes outside the project."
          : "The app does not open this kind of link.",
      address: null,
    },
    { priority: true },
  );
}

/** NTF-FR-WQRN: a Warn toast for a link whose file or heading does not exist. */
export function showLinkTargetMissing(): void {
  showToast(
    {
      key: LINK_TARGET_MISSING_KEY,
      level: "Warn",
      title: "Link target not found",
      body: "No file or heading matches the link.",
      address: null,
    },
    { priority: true },
  );
}

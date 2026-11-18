/**
 * What a host of the board gives the issue view (`MIV-map-issue-view.md`), and
 * what the view gives back to the parts a host draws.
 */
import type { ReactNode } from "react";

import type { FlowItem } from "../../../state/flowDocument";

/** What the parts of a host can ask of the issue view. */
export interface IssueViewContext {
  /** MIV-FR-PGEC / MIV-FR-ZQAX: select an item, show it, and open its issue view. */
  showItem: (itemId: string) => void;
  /** FLO-FR-ULEM: open a tracker item through `"open tracker link (url)"`. */
  openTracker: (url: string) => void;
}

/** The parts a host adds to the issue view of one item. */
export interface IssueViewParts {
  /** MIV-FR-POMI: the claim, retry, or graduate action in the header. */
  actions?: ReactNode;
  /** MIV-FR-VHDP: the **Labels** and **Milestone** fields of the sidebar. */
  details?: ReactNode;
  /** MIV-FR-VHDP / SDL-FR-KNOP: the **Specs** field at the end of the sidebar. */
  specs?: ReactNode;
  /** MIV-FR-XMMS: the **Sub-issues** section and **Add existing issue**. */
  below?: ReactNode;
}

/** MIV-FR-SVXH .. MIV-FR-ITSA: the host of the issue view of a board. */
export interface IssueViewHost {
  parts?: (item: FlowItem, context: IssueViewContext) => IssueViewParts;
  /** MIV-FR-ITSA: the review sheet shows, so no issue view shows. */
  blocked?: boolean;
  /** MIV-FR-RPAU: the item whose issue view was open when the canvas last unmounted. */
  initialOpen?: string | null;
  /** MIV-FR-RPAU: each change of the open issue view. */
  onOpenChange?: (itemId: string | null) => void;
}

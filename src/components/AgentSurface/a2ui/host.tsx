/**
 * What the application's own parts of a surface read
 * (`ASF-agent-surfaces.md` ASF-FR-NOBK, ASF-FR-NEPE, ASF-FR-MLKG).
 *
 * The official renderer draws inside the application's React tree, so the
 * parts the application supplies — media, the dialog, the cards — read this
 * context: which surface they belong to, whether the panel takes input, what
 * was sent, and how a card sends its action.
 */
import { createContext, useContext } from "react";

import type { SurfaceSubmission } from "../../../types";

/** A card action: its name and the context its kind sends (AUI-FR-XCXV). */
export interface CardSend {
  name: string;
  context: Record<string, unknown>;
  /** The panel's data model at the press, which the renderer adds. */
  dataModel?: Record<string, unknown>;
}

export interface SurfaceHost {
  surfaceId: string;
  /** ASF-FR-XOSG / ASF-FR-YCTE / ASF-FR-JSTI: no control takes input. */
  readOnly: boolean;
  /** ASF-FR-YCTE: the author's one action, once sent. */
  submission?: SurfaceSubmission;
  /** ASF-FR-MLKG: a press of a card's button. */
  sendCard: (sourceId: string, send: CardSend) => void;
}

const HostContext = createContext<SurfaceHost | null>(null);

export const SurfaceHostProvider = HostContext.Provider;

export function useSurfaceHost(): SurfaceHost {
  const host = useContext(HostContext);
  if (!host) throw new Error("an application part of a surface outside its panel");
  return host;
}

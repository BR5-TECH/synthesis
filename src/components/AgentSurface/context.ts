/**
 * What every component of one surface panel reads
 * (`ASF-agent-surfaces.md` ASF-FR-HRQL).
 */
import { createContext, useContext } from "react";

import type { Scope } from "./functions";
import type { SurfaceComponent, SurfaceSubmission } from "../../types";

/** A card action: its name and the context its kind sends (AUI-FR-XCXV). */
export interface CardSend {
  name: string;
  context: Record<string, unknown>;
}

/** What every component of one panel reads. */
export interface SurfaceRenderContext {
  surfaceId: string;
  byId: ReadonlyMap<string, SurfaceComponent>;
  model: Record<string, unknown>;
  write: (pointer: string, value: unknown) => void;
  /** ASF-FR-XOSG / ASF-FR-YCTE / ASF-FR-JSTI: no control takes input. */
  readOnly: boolean;
  /** ASF-FR-QBTR / ASF-FR-MLKG: a press of a Button, or of a card's button. */
  press: (sourceId: string, scope: Scope, card?: CardSend) => void;
  /** ASF-FR-YCTE: the author's one action, once sent. */
  submission?: SurfaceSubmission;
}

const RenderContext = createContext<SurfaceRenderContext | null>(null);

export const SurfaceRenderProvider = RenderContext.Provider;

export function useRender(): SurfaceRenderContext {
  const context = useContext(RenderContext);
  if (!context) throw new Error("a surface component outside its panel");
  return context;
}

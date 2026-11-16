/**
 * The Delivery lens over a Flow file (`MDL-map-delivery-lens.md` MDL-FR-VLQJ,
 * MDL-FR-JRGK, MDL-FR-WMGU, MDL-FR-QAJN, MDL-FR-NTVL): the canvas of
 * `FLO-flow.md` bound to the file, its action control, and Publish to GitHub.
 */
import { useState, type ReactNode, type Ref } from "react";

import * as api from "../../api";
import { errorLogCode } from "../../errorLogCode";
import { logInfo, logWarn } from "../../logging";
import { deliveryErrorMessage } from "../../state/deliveryErrors";
import { showToast } from "../../state/toasts";
import type { DeliverySource } from "../../types/delivery";
import type { ItemDeliveryState, OpenableArtifact } from "../../types";
import type { FlowItem } from "../../state/flowDocument";
import { FlowCanvas, fileBinding, type FlowCanvasHandle, type FlowViewState } from "../Flow";
import { BandMenu, FlowPathLink, type BandMenuEntry } from "./BandMenu";
import { PublishDialog, type PublishState } from "./dialogs";
import type { DeliveryHost } from "./host";

type FileSource = Extract<DeliverySource, { kind: "file" }>;

/** DSR-FR-ULON: the URLs of the items a failed publish created. */
function createdUrlsOf(e: unknown): string[] {
  if (e && typeof e === "object" && "createdUrls" in e) {
    const urls = (e as { createdUrls: unknown }).createdUrls;
    if (Array.isArray(urls)) return urls.filter((u): u is string => typeof u === "string");
  }
  return [];
}

export interface FileLensProps {
  source: FileSource;
  host: DeliveryHost;
  canvasRef: Ref<FlowCanvasHandle>;
  /** The mounted canvas, for Reset layout and the write before a publish. */
  getCanvas: () => FlowCanvasHandle | null;
  initialView?: FlowViewState;
  onViewChange: (state: FlowViewState) => void;
  panelSpecs: (itemId: string) => ReactNode;
  onOpenArtifact: (item: OpenableArtifact) => void;
  onStates: (states: ReadonlyMap<string, ItemDeliveryState> | null) => void;
  /** SNV-FR-56: close every other floating overlay before a dialog opens. */
  onOverlayOpening?: () => void;
  /** MDL-FR-HULA: the **Discuss** entry of the menu of an item or a milestone. */
  onDiscuss?: (item: FlowItem) => void;
  /** MDL-FR-JRGK: a publish made GitHub the source; read it again. */
  onSourceChanged: () => void;
}

export function FileLens(props: FileLensProps) {
  const { source, host } = props;
  const [publish, setPublish] = useState<PublishState | null>(null);
  const configuration = host.claims?.view?.configuration ?? null;
  // MDL-FR-JRGK: Publish to GitHub needs a GitHub configuration that is valid.
  const canPublish = configuration?.state === "valid";

  const runPublish = async () => {
    setPublish({ phase: "running" });
    const handle = props.getCanvas();
    // MDL-FR-JRGK / FLO-FR-27: the pending changes are written first.
    const written = handle ? await handle.flush() : { ok: true };
    if (!written.ok) {
      logWarn(["frontend"], "delivery publish stopped: the flow was not written", {});
      setPublish({
        phase: "failed",
        error: "The Flow file was not written, so nothing was published.",
        createdUrls: [],
      });
      return;
    }
    try {
      const result = await api.publishDeliveryToGithub();
      setPublish(null);
      logInfo(["frontend", "remote"], "delivery published to github", {
        milestones: result.milestones,
        issues: result.issues,
        relations: result.relations,
      });
      showToast(
        {
          key: "delivery-publish",
          level: "Info",
          title: "Plan published to GitHub",
          body: `${result.milestones} milestones, ${result.issues} issues, and ${result.relations} relations are on GitHub. GitHub is now the source of the delivery.`,
          address: null,
        },
        { priority: true },
      );
      props.onSourceChanged();
    } catch (e) {
      const createdUrls = createdUrlsOf(e);
      logWarn(["frontend", "remote"], "delivery publish refused", {
        code: errorLogCode(e),
        created: createdUrls.length,
      });
      setPublish({ phase: "failed", error: deliveryErrorMessage(e), createdUrls });
    }
  };

  const entries: BandMenuEntry[] = [
    ...(canPublish
      ? [
          {
            label: "Publish to GitHub",
            onSelect: () => {
              props.onOverlayOpening?.();
              setPublish({ phase: "confirm" });
            },
          },
        ]
      : []),
    {
      label: "Reset layout",
      onSelect: () => {
        logInfo(["frontend"], "delivery layout reset", { source: "file" });
        props.getCanvas()?.resetLayout();
      },
    },
  ];

  return (
    <div className="dlens-plan">
      <div className="dlens-canvas">
        <FlowCanvas
          ref={props.canvasRef}
          binding={fileBinding(host.flows, source.flowId)}
          // MDL-FR-VLQJ / MDL-FR-XJBV / DBR-FR-ERHW: the lens lays the plan out as a board.
          board
          // MDL-FR-WMGU: while a publish runs, no edit affordance acts.
          inert={publish?.phase === "running"}
          initialView={props.initialView}
          onViewChange={props.onViewChange}
          onDiscuss={props.onDiscuss}
          onDeliveryStates={props.onStates}
          onOpenArtifact={props.onOpenArtifact}
          onCreateDraft={host.onCreateDraft}
          onOpenDraft={host.onOpenDraft}
          onOpenDraftById={host.onOpenDraftById}
          bandExtras={
            <>
              <FlowPathLink
                path={props.source.path}
                {...(host.onRevealInProject
                  ? { onReveal: () => host.onRevealInProject?.(props.source.flowId) }
                  : {})}
              />
              <BandMenu entries={entries} />
            </>
          }
          // MDL layout notes: a notice line sits below the band.
          belowBand={
            publish?.phase === "running" && (
              <div className="smap-notice dlens-notice" role="status" data-testid="delivery-publish-progress">
                <span className="dlens-spinner" aria-hidden="true" />
                Publishing the plan to GitHub. The plan cannot be changed until the publish ends.
              </div>
            )
          }
          panelExtras={(item) => props.panelSpecs(item.id)}
        />
      </div>
      {publish && (
        <PublishDialog
          state={publish}
          projectTitle={configuration?.projectTitle ?? null}
          onConfirm={() => void runPublish()}
          onCancel={() => setPublish(null)}
          onOpenLink={(url) => {
            api.openTrackerLink(url).catch((e) => {
              logWarn(["frontend"], "created tracker item could not be opened", { code: errorLogCode(e) });
            });
          }}
        />
      )}
    </div>
  );
}

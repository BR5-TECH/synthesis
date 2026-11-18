/**
 * The Delivery lens over a GitHub plan (`MDL-map-delivery-lens.md`
 * MDL-FR-XJBV .. MDL-FR-PUKL, MDL-FR-QIXG, MDL-FR-QAJN, MDL-FR-NTVL): the
 * canvas of `FLO-flow.md` with a plan binding, the notices, the band, and the
 * dialogs of a GitHub plan.
 */
import { useEffect, useMemo, useState, useSyncExternalStore, type Ref } from "react";

import * as api from "../../api";
import { errorLogCode } from "../../errorLogCode";
import { logInfo, logWarn } from "../../logging";
import { deliveryErrorMessage } from "../../state/deliveryErrors";
import { isContainerKind, type FlowDocument, type FlowItem } from "../../state/flowDocument";
import type { GithubPlanSession } from "../../state/delivery/githubPlan";
import type { DeliveryLensData } from "../../state/delivery/lensData";
import { showToast } from "../../state/toasts";
import { githubLayoutScope, type DeliverySettings, type DeliverySource } from "../../types/delivery";
import type { ItemDeliveryState, OpenableArtifact } from "../../types";
import {
  FlowCanvas,
  type FlowCanvasHandle,
  type FlowPlanBinding,
  type FlowViewState,
  type IssueViewContext,
  type IssueViewHost,
} from "../Flow";
import { GithubIssueDetails, SubIssues, useGithubIssueDetail, type PlanIssue } from "./IssueDetailFields";
import { BandMenu, GithubBandFacts } from "./BandMenu";
import {
  ExportDialog,
  exportPathFor,
  MapTypesDialog,
  RemoveDialog,
  type RemoveMode,
  type TypeChoice,
} from "./dialogs";
import type { DeliveryHost } from "./host";
import { useProposalBoard, withoutGhosts } from "./useProposalBoard";
import { AddExistingIssue, ClaimActions } from "./ItemPanel";
import { StaleNotice, StructureNotice, UnmappedNotice } from "./Notices";

type GithubSource = Extract<DeliverySource, { kind: "github" }>;

type Dialog =
  | { kind: "remove"; itemId: string; name: string; itemKind: FlowDocument["items"][number]["kind"]; count: number }
  | { kind: "types"; busy: boolean; error: string | null }
  | { kind: "export"; busy: boolean; error: string | null };

/** MDL-FR-XUJY: the settings with the author's choices of Map Types written in. */
export function withTypeChoices(
  settings: DeliverySettings,
  choices: ReadonlyMap<string | null, Exclude<TypeChoice, null>>,
): DeliverySettings {
  const readEntries = [...settings.readEntries];
  let untyped = settings.untyped;
  for (const [type, kind] of choices) {
    if (type === null) {
      untyped = kind;
      continue;
    }
    const at = readEntries.findIndex((e) => e.type === type);
    if (at >= 0) readEntries[at] = { type, kind };
    else readEntries.push({ type, kind });
  }
  return { ...settings, readEntries, untyped };
}

export interface GithubLensProps {
  source: GithubSource;
  session: GithubPlanSession;
  data: DeliveryLensData;
  host: DeliveryHost;
  canvasRef: Ref<FlowCanvasHandle>;
  /** The mounted canvas, for Reset layout and the write before a publish. */
  getCanvas: () => FlowCanvasHandle | null;
  initialView?: FlowViewState;
  onViewChange: (state: FlowViewState) => void;
  panelSpecs: (itemId: string) => React.ReactNode;
  /** MIV-FR-ITSA / MIV-FR-RPAU: what the lens keeps of the issue view. */
  issueView: IssueViewHost;
  onOpenArtifact: (item: OpenableArtifact) => void;
  /** SNV-FR-56: close every other floating overlay before a dialog opens. */
  onOverlayOpening?: () => void;
  /** MDL-FR-HULA: the **Discuss** entry of the menu of an item or a milestone. */
  onDiscuss?: (item: FlowItem) => void;
  /** MDL-FR-QIXG / MDL-FR-XUJY: the source changed; read it again. */
  onSourceChanged: () => void;
}

export function GithubLens(props: GithubLensProps) {
  const { source, session, data, host } = props;
  useSyncExternalStore(session.subscribe, session.getVersion);
  const { answer, shown, loading, loadError, pending, itemErrors } = session.snapshot();
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [addingTop, setAddingTop] = useState(false);

  // MDL-FR-LZIE: the first render loads with `refresh` false, then true.
  useEffect(() => {
    session.start();
  }, [session]);

  const repository = `${source.repository.owner}/${source.repository.name}`;
  const tracker = useMemo(() => new Map((answer?.tracker ?? []).map((t) => [t.itemId, t])), [answer]);
  const settings = host.claims?.view?.settings;
  const statusNames = useMemo(
    () => ({ ready: settings?.readyStatus ?? null, inProgress: settings?.inProgressStatus ?? null }),
    [settings?.readyStatus, settings?.inProgressStatus],
  );
  const states = useMemo(
    () => new Map<string, ItemDeliveryState>((answer?.states ?? []).map((s) => [s.itemId, s])),
    [answer],
  );
  // MDL-FR-KKYH: the plan name is the Project's title.
  const named = useMemo(
    () => (shown ? { ...shown, name: source.project.title } : null),
    [shown, source.project.title],
  );
  // DBR-FR-AGIN .. DBR-FR-PMGS: the marks and the ghost cards of the plan proposals.
  const { document, proposal } = useProposalBoard(named, answer?.tracker);

  // MIV-FR-EDKQ: the issue view of an issue reads its detail.
  const [openItem, setOpenItem] = useState<string | null>(props.issueView.initialOpen ?? null);
  const hostOpenChange = props.issueView.onOpenChange;
  const openFacts = openItem === null ? undefined : tracker.get(openItem);
  const detail = useGithubIssueDetail(
    openFacts && openFacts.ref.kind === "issue"
      ? { itemId: openFacts.itemId, number: openFacts.ref.number, updatedAt: openFacts.updatedAt }
      : null,
  );
  const issueByNumber = useMemo(() => {
    const out = new Map<number, string>();
    for (const t of tracker.values()) if (t.ref.kind === "issue") out.set(t.ref.number, t.itemId);
    return out;
  }, [tracker]);
  const planIssue = (number: number): PlanIssue | undefined => {
    const itemId = issueByNumber.get(number);
    const item = itemId === undefined ? undefined : document?.items.find((i) => i.id === itemId);
    const facts = itemId === undefined ? undefined : tracker.get(itemId);
    return item && facts ? { item, tracker: facts, state: states.get(item.id) } : undefined;
  };
  const issueParts = (item: FlowItem, context: IssueViewContext) => {
    const facts = tracker.get(item.id);
    const isIssue = facts?.ref.kind === "issue";
    const shown = detail?.itemId === item.id ? detail : null;
    return {
      // MIV-FR-POMI / MDL-FR-QEXS: the claim and graduate actions in the header.
      actions: (
        <ClaimActions
          item={item}
          tracker={facts}
          state={states.get(item.id)}
          claims={host.claims}
          inert={false}
        />
      ),
      // MIV-FR-NBHF: Labels and Milestone apply to an issue.
      details: isIssue ? <GithubIssueDetails detail={shown} /> : undefined,
      specs: props.panelSpecs(item.id),
      below: (
        <>
          {isIssue && facts && (
            <SubIssues
              detail={shown}
              issueUrl={facts.ref.url}
              repository={repository}
              planIssue={planIssue}
              context={context}
            />
          )}
          {isContainerKind(item.kind) && (
            <AddExistingIssue
              key={item.id}
              containerName={item.name}
              inert={false}
              onAdd={async (number) => {
                const done = session.addExistingIssue(number, item.id);
                const answer = done ? await done : null;
                return answer && !answer.ok ? answer.error : null;
              }}
            />
          )}
        </>
      ),
    };
  };

  const open = (next: Dialog) => {
    props.onOverlayOpening?.();
    setDialog(next);
  };

  const binding: FlowPlanBinding = {
    kind: "plan",
    scope: githubLayoutScope(source.project.nodeId),
    document,
    states: answer?.states ?? null,
    overlaps: answer?.overlaps ?? null,
    tracker: answer?.tracker ?? [],
    repository,
    pending,
    itemErrors,
    proposal,
    layouts: data.layouts,
    // DBR-FR-WKNI: a ghost card is not part of the plan, so an edit never carries it.
    onEdit: (change) => session.change(withoutGhosts(change, proposal.ghosts)),
    // MDL-FR-XQGD: a removal opens the remove dialog.
    onRemoveRequest: (request) =>
      open({ kind: "remove", itemId: request.itemId, name: request.name, itemKind: request.kind, count: request.count }),
  };

  const refresh = async () => {
    setRefreshing(true);
    const ok = await session.load(true);
    setRefreshing(false);
    if (!ok) {
      showToast(
        {
          key: "github-delivery-refresh",
          level: "Error",
          title: "Plan not refreshed",
          body: "GitHub could not be read. The plan shows the last read.",
          address: null,
        },
        { priority: true },
      );
    }
  };

  const remove = (mode: RemoveMode) => {
    if (dialog?.kind !== "remove") return;
    const { itemId } = dialog;
    setDialog(null);
    logInfo(["frontend", "remote"], "delivery lens removes a github plan item", { mode });
    void session.remove(itemId, mode);
  };

  const mapTypes = async (choices: ReadonlyMap<string | null, Exclude<TypeChoice, null>>) => {
    setDialog({ kind: "types", busy: true, error: null });
    try {
      const settings = await api.getDeliverySettings();
      await api.setDeliverySettings(withTypeChoices(settings, choices));
    } catch (e) {
      logWarn(["frontend"], "delivery types were not mapped", { code: errorLogCode(e) });
      setDialog({ kind: "types", busy: false, error: deliveryErrorMessage(e) });
      return;
    }
    setDialog(null);
    logInfo(["frontend"], "delivery types mapped", { types: choices.size });
    showToast(
      { key: "delivery-map-types", level: "Info", title: "Types mapped", body: "The plan was read again with the new kinds.", address: null },
      { priority: true },
    );
    void session.load(false);
  };

  const exportTo = async (path: string) => {
    setDialog({ kind: "export", busy: true, error: null });
    try {
      const result = await api.exportDeliveryToFile(path);
      setDialog(null);
      logInfo(["frontend"], "delivery exported to a flow file", {});
      showToast(
        {
          key: "delivery-export",
          level: "Info",
          title: "Plan exported",
          body: `The delivery is now the Flow file ${result.path}.`,
          address: null,
        },
        { priority: true },
      );
      props.onSourceChanged();
    } catch (e) {
      logWarn(["frontend"], "delivery export refused", { code: errorLogCode(e) });
      setDialog({ kind: "export", busy: false, error: deliveryErrorMessage(e) });
    }
  };

  // MDL layout notes: the notice lines sit below the band, one line each.
  const notices = (
    <>
      {answer?.stale && (
        <StaleNotice answer={answer} retrying={refreshing} onRetry={() => void refresh()} />
      )}
      {answer && answer.notices.unmappedTypes.length > 0 && (
        <UnmappedNotice
          types={answer.notices.unmappedTypes}
          onMapTypes={() => open({ kind: "types", busy: false, error: null })}
        />
      )}
      {answer && answer.notices.misplaced.length + answer.notices.omittedRelations.length > 0 && (
        <StructureNotice notices={answer.notices} tracker={tracker} />
      )}
    </>
  );

  let body;
  if (!document) {
    body =
      loadError && !loading ? (
        // MDL-FR-NRJM: a load that refused with no plan to show.
        <div className="smap-state" role="alert">
          <p className="smap-state__line">{loadError}</p>
          <div className="smap-state__actions">
            <button type="button" className="btn btn--sm" onClick={() => void session.retry()}>
              Retry
            </button>
          </div>
        </div>
      ) : (
        <div className="smap-state" role="status">
          <p className="smap-state__line">Loading the delivery plan…</p>
        </div>
      );
  } else {
    body = (
      <FlowCanvas
        ref={props.canvasRef}
        binding={binding}
        // MDL-FR-VLQJ / MDL-FR-XJBV / DBR-FR-ERHW: the lens lays the plan out as a board.
        board
        initialView={props.initialView}
        onViewChange={props.onViewChange}
        onDiscuss={props.onDiscuss}
        onOpenArtifact={props.onOpenArtifact}
        onCreateDraft={host.onCreateDraft}
        onOpenDraft={host.onOpenDraft}
        onOpenDraftById={host.onOpenDraftById}
        belowBand={notices}
        bandExtras={
          <>
            <GithubBandFacts
              repository={repository}
              projectUrl={source.project.url}
              readAt={answer?.readAt ?? null}
              refreshing={refreshing}
              onRefresh={() => void refresh()}
            />
            <BandMenu
              entries={[
                {
                  label: "Export to file",
                  onSelect: () => open({ kind: "export", busy: false, error: null }),
                },
                {
                  label: "Reset layout",
                  onSelect: () => {
                    logInfo(["frontend"], "delivery layout reset", { source: "github" });
                    props.getCanvas()?.resetLayout();
                  },
                },
              ]}
            />
          </>
        }
        // DBR-FR-TCPE: the status dot reads the Ready and In Progress status names.
        statusNames={statusNames}
        // MDL-FR-XKCH / MIV-FR-VHDP: the issue view of an item of the plan.
        issueView={{
          ...props.issueView,
          onOpenChange: (itemId) => {
            setOpenItem(itemId);
            hostOpenChange?.(itemId);
          },
          parts: issueParts,
        }}
        addMenuExtras={(close) => (
          <button
            type="button"
            role="menuitem"
            className="draft-actions__item"
            onClick={() => {
              close();
              setAddingTop(true);
            }}
          >
            <span className="flow-addmenu__glyph" aria-hidden="true">
              #
            </span>
            Existing issue
          </button>
        )}
      />
    );
  }

  return (
    <div className="dlens-plan">
      <div className="dlens-canvas">
        {body}
        {addingTop && document && (
          <div className="dlens-add-top" data-testid="delivery-add-top">
            <AddExistingIssue
              containerName={null}
              inert={false}
              autoFocus
              onAdd={async (number) => {
                const done = session.addExistingIssue(number, null);
                const answer = done ? await done : null;
                return answer && !answer.ok ? answer.error : null;
              }}
              onDone={() => setAddingTop(false)}
            />
            <button
              type="button"
              className="flow-btn flow-btn--quiet flow-btn--xs dlens-add-top__close"
              onClick={() => setAddingTop(false)}
            >
              Cancel
            </button>
          </div>
        )}
      </div>

      {dialog?.kind === "remove" && (
        <RemoveDialog
          name={dialog.name}
          kind={dialog.itemKind}
          count={dialog.count}
          onChoose={remove}
          onCancel={() => setDialog(null)}
        />
      )}
      {dialog?.kind === "types" && answer && (
        <MapTypesDialog
          types={answer.notices.unmappedTypes}
          busy={dialog.busy}
          error={dialog.error}
          onConfirm={(choices) => void mapTypes(choices)}
          onCancel={() => setDialog(null)}
        />
      )}
      {dialog?.kind === "export" && (
        <ExportDialog
          initialPath={exportPathFor(source.project.title)}
          busy={dialog.busy}
          error={dialog.error}
          onConfirm={(path) => void exportTo(path)}
          onCancel={() => setDialog(null)}
        />
      )}
    </div>
  );
}

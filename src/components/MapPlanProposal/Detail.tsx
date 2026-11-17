/**
 * MPV-FR-WQWG: the detail of the selected row: its identity, its title, its
 * field changes, its body diff, and its footer.
 */
import type { MapPlanProposal, MapPlanRow } from "../../types";
import { rowTitle, type RowStatus } from "../../state/mapPlan/derive";
import { BodyDiffView } from "./BodyDiff";
import { Fields } from "./Fields";
import type { MapPlanHost } from "./host";
import { OpBadge, RowKind, RowRef, StatusWord } from "./parts";
import { RowFooter, type FooterActions } from "./RowFooter";

/** MPV-FR-NMYT: the reference of a changed row selects its item on the canvas. */
function DetailRef({ row, host }: { row: MapPlanRow; host: MapPlanHost | null }) {
  if (row.kind === "modify_issue" && host?.itemOnCanvas(row.fields.number)) {
    const number = row.fields.number;
    return (
      <button
        type="button"
        className="pp-ref pp-ref--link"
        title={`Select #${number} on the canvas`}
        onClick={() => host.selectItem(number)}
        data-testid="pp-detail-ref"
      >
        #{number}
      </button>
    );
  }
  return <RowRef row={row} />;
}

export function Detail({
  proposal,
  row,
  status,
  refusal,
  reprepareRefusal,
  reprepareBusy,
  proposer,
  host,
  actions,
}: {
  proposal: MapPlanProposal;
  row: MapPlanRow;
  status: RowStatus;
  refusal: string | null;
  reprepareRefusal: string | null;
  reprepareBusy: boolean;
  proposer: string | null;
  host: MapPlanHost | null;
  actions: FooterActions;
}) {
  return (
    <div className="pp-detail" data-testid="pp-detail" data-row={row.key}>
      <div className="pp-detail__scroll">
        <div>
          <div className="pp-detail__id">
            <RowKind row={row} host={host} />
            <DetailRef row={row} host={host} />
            <OpBadge row={row} />
            <StatusWord status={status} />
          </div>
          <h3 className="pp-detail__title">{rowTitle(row)}</h3>
        </div>
        <Fields proposal={proposal} row={row} />
        <BodyDiffView row={row} />
      </div>
      <div className="pp-detail__foot">
        <RowFooter
          proposal={proposal}
          row={row}
          status={status}
          refusal={refusal}
          reprepareRefusal={reprepareRefusal}
          reprepareBusy={reprepareBusy}
          proposer={proposer}
          host={host}
          actions={actions}
        />
      </div>
    </div>
  );
}

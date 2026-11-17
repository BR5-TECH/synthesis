/**
 * MPV-FR-ZFDO .. MPV-FR-PVXW: the body of a row typeset as the issue reads,
 * with a gutter mark on each added or removed block, the changed words of an
 * edited block marked inline, and folds of unchanged blocks.
 */
import { useMemo, useState } from "react";

import { bodyCountLabel, diffBody, type BodyBlock, type Piece } from "../../diff/bodyDiff";
import type { MapPlanRow } from "../../types";
import { Icon } from "../icons";

function PieceView({ piece }: { piece: Piece }) {
  const text =
    piece.style === "strong" ? (
      <strong>{piece.text}</strong>
    ) : piece.style === "em" ? (
      <em>{piece.text}</em>
    ) : piece.style === "code" ? (
      <code>{piece.text}</code>
    ) : (
      piece.text
    );
  if (piece.mark === "ins") return <ins>{text}</ins>;
  if (piece.mark === "del") return <del>{text}</del>;
  return <>{text}</>;
}

const GUTTER: Readonly<Record<BodyBlock["mode"], string>> = { add: "+", del: "−", edit: "", ctx: "" };
const SPOKEN: Readonly<Record<BodyBlock["mode"], string>> = {
  add: "added",
  del: "removed",
  edit: "edited",
  ctx: "",
};

function BlockView({ block }: { block: BodyBlock }) {
  const m = block.mode === "edit" ? "ctx" : block.mode;
  return (
    <div className="pp-b" data-m={m} data-mode={block.mode}>
      <span className="pp-b__g" aria-hidden="true">
        {GUTTER[block.mode]}
      </span>
      <div
        className={`pp-b__c pp-${block.kind}`}
        style={block.kind === "li" && block.depth > 0 ? { marginLeft: block.depth * 16 } : undefined}
      >
        {SPOKEN[block.mode] && <span className="pp-sr-only">{SPOKEN[block.mode]}: </span>}
        {block.pieces.map((piece, i) => (
          <PieceView key={i} piece={piece} />
        ))}
      </div>
    </div>
  );
}

function Fold({ blocks, label }: { blocks: BodyBlock[]; label?: string }) {
  const [open, setOpen] = useState(false);
  if (open) {
    return (
      <>
        {blocks.map((block, i) => (
          <BlockView key={i} block={block} />
        ))}
      </>
    );
  }
  return (
    <button type="button" className="pp-fold" onClick={() => setOpen(true)} data-testid="pp-fold">
      <Icon.Unfold size={12} aria-hidden="true" />
      {label ?? `${blocks.length} unchanged blocks`}
    </button>
  );
}

/** The old and the new body of a row, or null when the row shows no body. */
export function bodiesOf(row: MapPlanRow): { before: string | null; after: string | null } | null {
  if (row.kind === "modify_issue") {
    const change = row.fields.changes.find((c) => c.field === "body");
    if (!change) return null;
    const text = (v: unknown) => (typeof v === "string" ? v : null);
    return { before: text(change.before), after: text(change.after) };
  }
  if (row.kind === "create_issue") {
    return row.fields.body && row.fields.body.trim() !== "" ? { before: null, after: row.fields.body } : null;
  }
  return row.fields.body.trim() !== "" ? { before: null, after: row.fields.body } : null;
}

export function BodyDiffView({ row }: { row: MapPlanRow }) {
  const bodies = bodiesOf(row);
  const diff = useMemo(() => (bodies ? diffBody(bodies.before, bodies.after) : null), [bodies?.before, bodies?.after]);
  if (!bodies || !diff) return null;
  const count = bodies.before === null ? "new description" : bodyCountLabel(diff);
  return (
    <section className="pp-doc" aria-label="Body" data-testid="pp-doc">
      <div className="pp-doc__label">
        <span>Body</span>
        {count && <span>{count}</span>}
      </div>
      {diff.items.map((item, i) =>
        item.type === "fold" ? <Fold key={i} blocks={item.blocks} /> : <BlockView key={i} block={item.block} />,
      )}
    </section>
  );
}

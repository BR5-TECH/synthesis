/**
 * The three-letter chip a question card and a change row carry
 * (`../../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-MCUP).
 *
 * It names the **kind of the row** and never the type of the file: `QST` on a
 * question, `CHG` on a change. What the row is about is the name beside the
 * chip, so two change rows never differ in chip for a reason the reader has no
 * way to see.
 *
 * The geometry is the Library's artifact chip (`.chip-type`); only the tone
 * differs, and both tones are the application's own — `--accent` on
 * `--accent-soft` for a question, `--ok` on `--ok-soft` for a change — so the
 * chip follows the theme with everything around it (DDS-FR-RJEV).
 */
export type StreamChipKind = "question" | "change";

const LABELS: Record<StreamChipKind, string> = {
  question: "QST",
  change: "CHG",
};

export function TypeChip({ kind }: { kind: StreamChipKind }) {
  return (
    <span className="dds-stream-chip" data-kind={kind}>
      {LABELS[kind]}
    </span>
  );
}

/**
 * Reading a submitted exchange back as one question card
 * (`../../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-KYWR, DQA-FR-ZPGM).
 *
 * The draft's discussion column draws a question and the answer beside it as
 * **one card**: the question text, its recorded options, the chosen option
 * marked in place, and the note in the card's footer. Nothing in the transcript
 * states the answer a second time.
 *
 * ## Why the card is parsed rather than read from a record
 *
 * DQA-FR-TSJD makes the pairing presentation and nothing else, and ADQ-FR-NUEB
 * forbids a marker in either body. So there is no structured answer to read: the
 * two comments hold exactly the prose the backend composed
 * (`../../../../src-tauri/src/comments/question_submit.rs`), and this module
 * reads that prose back. The question body is the recorded text, a blank line,
 * and the options as an ordered list; the answer body is one fixed opening line
 * and an optional note paragraph.
 *
 * A body this module cannot read yields `null`, and the entry then renders as
 * the two ordinary comments it is — the grouping is what is lost, and nothing
 * else (DQA-FR-TSJD).
 */

/** ADQ-FR-RECR: the line an answer opens with where the author chose an option. */
const SELECTED_PREFIX = "**Selected option:**";
/** ADQ-FR-RECR: the line an answer opens with where the author wrote their own. */
const OWN_PREFIX = "**Own answer:**";
/** ADQ-FR-MRSK: the paragraph a note is composed as, beneath the chosen option. */
const NOTE_PREFIX = "**Note:**";

/**
 * The characters the composer backslash-escapes, and this reader puts back.
 *
 * The same set `../../../diff/markdown.ts` unescapes and
 * `question_submit.rs::escape_markdown` writes — the three must agree, or an
 * option carrying an asterisk reads with a stray backslash in the card.
 */
const ESCAPABLE = "\\`*_[]";

/** One option of a question card, as the card draws it. */
export interface CardOption {
  /** 1-based, the number the recorded list carries. */
  number: number;
  /** The recorded value, with the composer's escaping taken back off. */
  value: string;
  /** DQA-FR-KYWR: the option the author chose, marked in place. */
  chosen: boolean;
}

/** A submitted exchange, as one card. */
export interface QuestionCardModel {
  /** ADQ-FR-MRSK: the recorded question text, exactly as it was recorded. */
  text: string;
  options: CardOption[];
  /**
   * DQA-FR-KYWR / DQA-FR-ZPGM: what stands in the card's footer, or `null` where
   * the card carries no footer at all.
   *
   * `note` is the note beside a chosen option; `own` is the author's own words,
   * which mark no option.
   */
  footer: { kind: "note" | "own"; text: string } | null;
}

/**
 * Put back the characters the composer escaped for Markdown.
 *
 * A backslash before anything outside {@link ESCAPABLE} is text the author
 * wrote, and stays — `C:\demo` is a path rather than an escape.
 */
export function unescapeMarkdown(value: string): string {
  let out = "";
  for (let at = 0; at < value.length; at += 1) {
    const ch = value[at];
    const next = value[at + 1];
    if (ch === "\\" && next !== undefined && ESCAPABLE.includes(next)) {
      out += next;
      at += 1;
      continue;
    }
    out += ch;
  }
  return out;
}

/**
 * ADQ-FR-MRSK: the question text and its options, split back out of the body.
 *
 * The options are taken from the **end** of the body: the longest run of
 * trailing lines that numbers 1, 2, 3 … without a gap. A question whose own text
 * ends in a numbered list would otherwise lose that list to the option group,
 * and reading backwards is what makes the recorded options the ones the composer
 * actually appended.
 */
function splitQuestion(body: string): { text: string; values: string[] } | null {
  const lines = body.split("\n");

  let end = lines.length;
  while (end > 0 && lines[end - 1].trim() === "") end -= 1;

  // Every numbered line at the foot of the body, collected backwards.
  const numbered: { number: number; value: string }[] = [];
  let head = end;
  while (head > 0) {
    const match = /^(\d+)\.[ \t]+(.*)$/.exec(lines[head - 1]);
    if (!match) break;
    numbered.unshift({ number: Number(match[1]), value: match[2] });
    head -= 1;
  }
  if (numbered.length === 0) return null;

  // The options are the last run that begins at 1 and counts up by one. A list
  // in the question's own text therefore stays in the text, because the run the
  // composer appended is the one closest to the foot.
  const from = numbered.map((entry) => entry.number).lastIndexOf(1);
  if (from < 0) return null;
  const options = numbered.slice(from);
  if (options.some((entry, at) => entry.number !== at + 1)) return null;

  const text = lines.slice(0, head + from).join("\n").trim();
  if (text === "") return null;
  return { text, values: options.map((entry) => entry.value) };
}

/** ADQ-FR-RECR: the answer body, split into its chosen value and its note. */
function splitAnswer(
  body: string,
): { kind: "selected" | "own"; value: string; note: string | null } | null {
  const trimmed = body.trimStart();
  if (trimmed.startsWith(OWN_PREFIX)) {
    return {
      kind: "own",
      value: trimmed.slice(OWN_PREFIX.length).trim(),
      note: null,
    };
  }
  if (!trimmed.startsWith(SELECTED_PREFIX)) return null;
  const rest = trimmed.slice(SELECTED_PREFIX.length);
  const at = rest.indexOf(NOTE_PREFIX);
  if (at < 0) return { kind: "selected", value: rest.trim(), note: null };
  return {
    kind: "selected",
    value: rest.slice(0, at).trim(),
    note: rest.slice(at + NOTE_PREFIX.length).trim() || null,
  };
}

/**
 * DQA-FR-KYWR: the card one submitted exchange draws, or `null` where either
 * body is not one this module can read.
 *
 * The chosen option is matched on the **escaped** text of both sides, which is
 * what the one composer wrote them as, so a value carrying an asterisk matches
 * itself rather than its unescaped twin.
 */
export function questionCardOf(
  questionBody: string,
  answerBody: string | null,
): QuestionCardModel | null {
  const question = splitQuestion(questionBody);
  if (!question) return null;

  const answer = answerBody === null ? null : splitAnswer(answerBody);
  // DQA-FR-ZPGM: the author's own words mark no option.
  const chosen =
    answer !== null && answer.kind === "selected" ? answer.value : null;

  const options = question.values.map((value, index) => ({
    number: index + 1,
    value: unescapeMarkdown(value),
    chosen: chosen !== null && value === chosen,
  }));

  return {
    text: question.text,
    options,
    footer: footerOf(answer),
  };
}

/** DQA-FR-KYWR / DQA-FR-ZPGM: what the card's footer holds, where it holds one. */
function footerOf(
  answer: ReturnType<typeof splitAnswer>,
): QuestionCardModel["footer"] {
  if (answer === null) return null;
  if (answer.kind === "own") {
    return { kind: "own", text: unescapeMarkdown(answer.value) };
  }
  if (answer.note === null) return null;
  return { kind: "note", text: unescapeMarkdown(answer.note) };
}

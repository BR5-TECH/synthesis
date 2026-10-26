/**
 * EDT-FR-FKUH, EDT-FR-QJJP: the shape of the bundled syntax hints of the
 * diagram editor overlay.
 */

/** One syntax snippet and its short explanation (EDT-FR-NVGA). */
export interface Hint {
  /** Code text, shown in the Source code role. It can hold line breaks. */
  code: string;
  /** A short English explanation. */
  text: string;
}

/** A titled group of hints, for example "Nodes" or "Styling". */
export interface HintGroup {
  title: string;
  hints: readonly Hint[];
}

/** The hints of one chart type (EDT-FR-QJJP, EDT-FR-DUSC). */
export interface HintSet {
  /** A stable key of the set. */
  key: string;
  /** The name of the chart type that the hints area shows (EDT-FR-YNQF). */
  name: string;
  /**
   * The ids that the bundled Mermaid version gives to the chart types this
   * set covers, for example `["flowchart", "flowchart-v2", "flowchart-elk"]`.
   */
  mermaidIds: readonly string[];
  /**
   * EDT-FR-NWZX: matches the first line of the source that is not empty and
   * is not a comment, when the source has this chart type.
   */
  detect: RegExp;
  /** The declaration line of the chart type. The general hints list it. */
  declaration: Hint;
  /** The element forms and the other hints of the chart type. */
  groups: readonly HintGroup[];
  /** A short complete example that the bundled Mermaid version parses. */
  example: string;
}

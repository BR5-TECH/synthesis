/**
 * EDT-FR-ERTS: the general hints, which hold the forms that every chart type
 * shares.
 */
import type { HintGroup } from "./types";

export const GENERAL_GROUPS: readonly HintGroup[] = [
  {
    title: "Comments and frontmatter",
    hints: [
      { code: "%% A comment", text: "A line that starts with %% is a comment." },
      {
        code: "---\ntitle: My diagram\n---",
        text: "A frontmatter block at the start sets the title and the configuration.",
      },
      {
        code: "---\nconfig:\n  theme: neutral\n---",
        text: "The config key of the frontmatter sets options, such as the theme.",
      },
    ],
  },
  {
    title: "Declaration",
    hints: [
      {
        code: "flowchart LR",
        text: "The first line that is not a comment declares the chart type.",
      },
    ],
  },
];

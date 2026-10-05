/**
 * CIP-FR-RDCY: the three areas of the frontend suite. `vitest.config.ts` makes
 * one Vitest project of each area, and CI runs each area as one check
 * (CIP-FR-VRPM). Every test file is in exactly one area: `components` is what
 * the first area does not hold under `src/components/`, and `app and state` is
 * what is outside `src/components/`.
 */

const TEST_FILE = "*.test.{ts,tsx}";

/**
 * The components of the editor and of the views around a document. A name
 * holds the files `Name*.test.ts(x)` and the folders `Name*` under
 * `src/components/`.
 */
const EDITOR_AND_DOCUMENT_COMPONENTS = [
  "Editor",
  "DiffView",
  "NewArtifactWorkspace",
  "Flow",
  "Notes",
  "notes",
  "DraftDiscussion",
  "discussion",
  "DiscussionQuestions",
  "Comment",
  "comment",
  "Document",
  "PdfViewer",
  "markdown",
  "findHighlight",
];

const editorAndDocuments = EDITOR_AND_DOCUMENT_COMPONENTS.flatMap((name) => [
  `src/components/${name}${TEST_FILE}`,
  `src/components/${name}*/**/${TEST_FILE}`,
]);

export interface TestArea {
  /** The Vitest project name, and the name in the CI check. */
  name: string;
  include: string[];
  exclude: string[];
}

export const TEST_AREAS: TestArea[] = [
  { name: "editor and documents", include: editorAndDocuments, exclude: [] },
  {
    name: "components",
    include: [`src/components/**/${TEST_FILE}`],
    exclude: editorAndDocuments,
  },
  {
    name: "app and state",
    include: [`src/**/${TEST_FILE}`],
    exclude: ["src/components/**"],
  },
];

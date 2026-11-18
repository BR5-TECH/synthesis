/**
 * MIV-FR-RNVP / MIV-FR-DDAF: the description of an issue as rich text: the
 * read-only rich view of DTV-FR-SSQI, with each fenced code block drawn by
 * `CodeFence` (highlighted, Gherkin, or a Mermaid diagram).
 */
import { renderCodeFence } from "../../CodeFence";
import { DocumentCodeContext, DocumentMarkdown } from "../../DocumentViewer/DocumentMarkdown";

export function IssueMarkdown({ body, testId }: { body: string; testId?: string }) {
  return (
    <DocumentCodeContext.Provider value={renderCodeFence}>
      <div className="doc miv-body" data-testid={testId}>
        <DocumentMarkdown text={body} />
      </div>
    </DocumentCodeContext.Provider>
  );
}

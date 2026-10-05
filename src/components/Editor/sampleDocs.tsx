import type { ReactNode } from "react";
import type { ArtifactType } from "../../types";

interface SampleDoc {
  title: string;
  type: ArtifactType;
  chip: string;
  body: ReactNode;
}

export const SAMPLE_DOCS: Record<string, SampleDoc> = {
  "design-review": {
    title: "design-review",
    type: "skill",
    chip: "SKL",
    body: (
      <div className="doc">
        <h1>Design review skill</h1>
        <p>
          Used when the agent needs to review a design artifact (Figma export,
          screenshot, or UI spec) and surface issues against the project's{" "}
          <strong>visual fundamentals</strong>.
        </p>
        <h2>When to invoke</h2>
        <ul>
          <li>
            A spec calls for a <em>"design review"</em> step.
          </li>
          <li>
            A new visual artifact lands in <code>specifications/ui/</code>.
          </li>
          <li>An author flags a screen as ready for review.</li>
        </ul>
        <h2>Steps</h2>
        <ol>
          <li>Open the artifact in the Editor and read the surrounding spec.</li>
          <li>
            Cross-reference the visual against the system tokens in{" "}
            <code>colors_and_type.css</code>.
          </li>
          <li>
            Produce a markdown report grouped by <strong>severity</strong>.
          </li>
        </ol>
        <blockquote>
          Reviewers are not allowed to invent new tokens. If a visual demands
          one, file it as an issue against the design system.
        </blockquote>
        <h3>Output format</h3>
        <pre>
          <code>{`## <screen-name>
- [blocker] <description>
- [major]   <description>
- [minor]   <description>`}</code>
        </pre>
      </div>
    ),
  },
  "tone-guide.draft": {
    title: "tone-guide.draft",
    type: "prompt",
    chip: "PRO",
    body: (
      <div className="doc">
        <h1>Tone guide (draft)</h1>
        <p>
          Working notes — not yet promoted to <strong>active</strong>. Loaded by
          playbooks under <em>customer-facing</em> roles.
        </p>
        <h2>House style</h2>
        <ul>
          <li>
            Second person: <em>"you"</em>, not <em>"the user"</em>.
          </li>
          <li>Active verbs in commit messages, imperative mood in skills.</li>
          <li>No emoji. No exclamation marks.</li>
        </ul>
      </div>
    ),
  },
};

import { useEffect, useState } from "react";
import { SAMPLE_DOCS } from "./sampleDocs";
import { EditorToolbar } from "./toolbar";
import type { EditorProps } from "./props";

/** The pre-backend mock editor used for surfaces without a backend artifact id. */
export function MockEditor({ artifactName }: EditorProps) {
  const doc =
    (artifactName && SAMPLE_DOCS[artifactName]) || SAMPLE_DOCS["design-review"];
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    setDirty(false);
  }, [artifactName]);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <EditorToolbar dirty={dirty} />
      <div
        style={{ flex: 1, overflow: "auto" }}
        onKeyDown={() => setDirty(true)}
      >
        <div className="editor">{doc.body}</div>
      </div>
    </div>
  );
}

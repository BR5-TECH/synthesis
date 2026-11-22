/**
 * The attachment render path's own guards (`CMT-comments.md` CMT-FR-48/49).
 *
 * An attachment record reaches the UI from a JSONL log that is committed to a
 * repository, concatenated by a union merge, and editable by hand — so the
 * values below are not hypothetical inputs, they are what an attacker controls
 * if they can land a line in the log. The backend refuses them at write time
 * (CMS-FR-43, CMS-FR-47); these are the render path refusing them again.
 */
import { describe, expect, it } from "vitest";
import { fileToInput, guessMediaType, inlineDataUrl, safeUrl } from "./CommentAttachments";
import {
  ATTACH_ACCEPT,
  DOCX_MEDIA_TYPE,
  base64ByteLength,
  formatByteSize,
  isDocumentMediaType,
} from "./commentDocuments";

describe("safeUrl: what may reach an href or a src", () => {
  it("admits http and https and nothing else", () => {
    expect(safeUrl("https://example.test/a.png")).toBe("https://example.test/a.png");
    expect(safeUrl("http://example.test/a.png")).toBe("http://example.test/a.png");
  });

  it("refuses every scheme that would execute or read something local", () => {
    for (const hostile of [
      "javascript:alert(1)",
      "JavaScript:alert(1)",
      "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
      "vbscript:msgbox(1)",
      "file:///etc/passwd",
      "blob:https://example.test/abc",
      "  javascript:alert(1)",
      "not a url at all",
      "",
    ]) {
      expect(safeUrl(hostile)).toBeNull();
    }
  });
});

describe("inlineDataUrl: what may be inlined as bytes", () => {
  it("inlines an image, stripping any media-type parameters", () => {
    expect(inlineDataUrl("image/png", "aGk=")).toBe("data:image/png;base64,aGk=");
    expect(inlineDataUrl("image/jpeg; charset=binary", "aGk=")).toBe(
      "data:image/jpeg;base64,aGk=",
    );
  });

  it("inlines nothing that could be a page or a script", () => {
    // `text/html` above all: a data:text/html URL in an href is a page that
    // runs script, so it must never be built even for a line claiming it.
    for (const hostile of [
      "text/html",
      "TEXT/HTML",
      "image/png,text/html",
      "application/pdf",
      "text/plain",
      "application/javascript",
      // An SVG document can carry script, and a thumbnail is wrapped in an
      // anchor to its own source — so bytes this application inlines are never
      // an SVG, however the line labels them.
      "image/svg+xml",
      "IMAGE/SVG+XML",
      "",
    ]) {
      expect(inlineDataUrl(hostile, "aGk=")).toBeNull();
    }
  });
});

describe("guessMediaType: a link's type, from its path alone", () => {
  it("recognises the common image extensions and the two document ones", () => {
    expect(guessMediaType("shot.PNG")).toBe("image/png");
    expect(guessMediaType("https://e.test/a/b.jpeg?v=2#x")).toBe("image/jpeg");
    expect(guessMediaType("notes.pdf")).toBe("application/pdf");
    expect(guessMediaType("out.log")).toBe("text/plain");
  });

  it("falls back to a type the backend refuses rather than inventing one", () => {
    // Guessing `image/png` for an unknown extension would smuggle whatever the
    // address actually serves past the accepted set (CMS-FR-47).
    expect(guessMediaType("archive.zip")).toBe("application/octet-stream");
    expect(guessMediaType("no-extension")).toBe("application/octet-stream");
  });
});

describe("DOCX in the Attach picker", () => {
  it("CVP-FR-YBTS: the picker offers images and exactly the accepted document and text files", () => {
    expect(ATTACH_ACCEPT.split(",").sort()).toEqual(
      [".docx", ".log", ".markdown", ".md", ".pdf", ".txt", "image/*"].sort(),
    );
  });

  it("CVP-FR-YBTS: a .docx name gives the DOCX type, and .doc does not", () => {
    expect(guessMediaType("spec.DOCX")).toBe(DOCX_MEDIA_TYPE);
    expect(guessMediaType("old.doc")).toBe("application/octet-stream");
  });

  it("CVP-FR-YBTS: a .docx the system names as empty or generic bytes is sent as DOCX", async () => {
    for (const type of ["", "application/octet-stream"]) {
      const input = await fileToInput(new File(["zip"], "spec.docx", { type }));
      expect(input).toMatchObject({ kind: "inline", mediaType: DOCX_MEDIA_TYPE, filename: "spec.docx" });
    }
    const named = await fileToInput(new File(["%PDF"], "a.pdf", { type: "application/pdf" }));
    expect(named).toMatchObject({ mediaType: "application/pdf" });
  });

  it("CVP-FR-YBTS: a Markdown file is sent as text/markdown, whatever the system names it", async () => {
    expect(guessMediaType("notes.MD")).toBe("text/markdown");
    expect(guessMediaType("notes.markdown")).toBe("text/markdown");
    for (const name of ["notes.md", "notes.markdown"]) {
      for (const type of ["", "application/octet-stream"]) {
        const input = await fileToInput(new File(["# hi"], name, { type }));
        expect(input).toMatchObject({ mediaType: "text/markdown", filename: name });
      }
    }
    const named = await fileToInput(new File(["# hi"], "notes.md", { type: "text/markdown" }));
    expect(named).toMatchObject({ mediaType: "text/markdown" });
  });
});

describe("the size a document chip shows", () => {
  it("CMT-FR-HQDV: PDF and DOCX are documents, whatever the case; other types are not", () => {
    expect(isDocumentMediaType("application/pdf")).toBe(true);
    expect(isDocumentMediaType(`${DOCX_MEDIA_TYPE.toUpperCase()}; x=y`)).toBe(true);
    expect(isDocumentMediaType("text/plain")).toBe(false);
    expect(isDocumentMediaType("image/png")).toBe(false);
  });

  it("CMT-FR-HQDV: sizes read in B, KB, and MB", () => {
    expect(formatByteSize(512)).toBe("512 B");
    expect(formatByteSize(2048)).toBe("2 KB");
    expect(formatByteSize(3 * 1024 * 1024)).toBe("3.0 MB");
  });

  it("CMT-FR-HQDV: the size of a pending file is the decoded length of its data", () => {
    expect(base64ByteLength(btoa("abc"))).toBe(3);
    expect(base64ByteLength(btoa("abcd"))).toBe(4);
    expect(base64ByteLength(btoa("abcde"))).toBe(5);
    expect(base64ByteLength("")).toBe(0);
  });
});

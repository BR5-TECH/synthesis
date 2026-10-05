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
import { guessMediaType, inlineDataUrl, safeUrl } from "./CommentAttachments";

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

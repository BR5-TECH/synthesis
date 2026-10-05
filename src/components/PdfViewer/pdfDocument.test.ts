// How the PDF viewer loads PDF.js (PDV-FR-CEIV, PDV-FR-GPLH): the worker is a
// bundled asset of the application, nothing is fetched from a remote host, and
// PDF.js gets bytes only.
import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

const library = vi.hoisted(() => ({
  GlobalWorkerOptions: { workerSrc: "" },
  getDocument: vi.fn(),
}));

vi.mock("pdfjs-dist/legacy/build/pdf.mjs", () => library);

beforeEach(() => {
  library.GlobalWorkerOptions.workerSrc = "";
  library.getDocument.mockReset();
  vi.resetModules();
});

describe("PDV-FR-CEIV: the worker is bundled", () => {
  it("PDV-FR-CEIV: the worker source is the application's own asset, resolved by the bundler, and not a remote URL", async () => {
    const { loadPdfjs } = await import("./pdfDocument");
    const loaded = await loadPdfjs();
    const src = loaded.GlobalWorkerOptions.workerSrc;
    expect(src).toMatch(/pdf\.worker\.min.*\.mjs/);
    expect(src).not.toMatch(/^[a-z][a-z0-9+.-]*:\/\//i);
    expect(src).not.toMatch(/^\/\//);
    expect(src.startsWith("/")).toBe(true);
  });

  it("PDV-FR-CEIV: the library loads once, however many viewers ask", async () => {
    const { loadPdfjs } = await import("./pdfDocument");
    const first = loadPdfjs();
    const second = loadPdfjs();
    expect(await first).toBe(await second);
  });

  it("PDV-FR-CEIV: no source of the viewer names a remote host, a CDN, or a network call", () => {
    const dir = resolve(process.cwd(), "src/components/PdfViewer");
    const files = readdirSync(dir).filter(
      (name) => /\.(ts|tsx)$/.test(name) && !/\.test\.|Fixtures/.test(name),
    );
    expect(files.length).toBeGreaterThan(5);
    for (const name of files) {
      const code = readFileSync(resolve(dir, name), "utf8")
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/^\s*\/\/.*$/gm, "");
      expect(code, name).not.toMatch(/https?:\/\//i);
      expect(code, name).not.toMatch(/\b(fetch|XMLHttpRequest|WebSocket|sendBeacon)\b/);
      expect(code, name).not.toMatch(/cdn|unpkg|jsdelivr/i);
    }
  });
});

describe("PDV-FR-GPLH: bytes only", () => {
  it("PDV-FR-GPLH: decodes base64 to the exact bytes", async () => {
    const { decodeBase64 } = await import("./pdfDocument");
    const bytes = new Uint8Array([0, 1, 2, 250, 251, 255, 37, 80, 68, 70]);
    let binary = "";
    for (const b of bytes) binary += String.fromCharCode(b);
    expect([...decodeBase64(btoa(binary))]).toEqual([...bytes]);
    expect(decodeBase64("").length).toBe(0);
  });

  it("PDV-FR-GPLH, PDV-FR-CEIV: opens a document from data alone, with no URL, no range, and no stream", async () => {
    const { startOpen } = await import("./pdfDocument");
    const data = new Uint8Array([1, 2, 3]);
    startOpen(library as never, data);
    expect(library.getDocument).toHaveBeenCalledTimes(1);
    const params = library.getDocument.mock.calls[0][0];
    expect(params.data).toBe(data);
    expect(params).not.toHaveProperty("url");
    expect(params.disableRange).toBe(true);
    expect(params.disableStream).toBe(true);
    expect(params.disableAutoFetch).toBe(true);
  });

  it("PDV-FR-YLWC: recognises the error PDF.js raises for a password", async () => {
    const { isPasswordError } = await import("./pdfDocument");
    expect(isPasswordError({ name: "PasswordException" })).toBe(true);
    expect(isPasswordError(new Error("x"))).toBe(false);
    expect(isPasswordError(null)).toBe(false);
  });
});

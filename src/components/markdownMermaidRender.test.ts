import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { logDebug } from "../logging";
import {
  holdNextRender,
  mermaidApi,
  overrideNextParse,
  overrideNextRender,
  resetMermaidMock,
} from "../test/mermaidMock";
import {
  DIAGRAM_ERROR,
  cachedDiagram,
  currentTheme,
  diagramElement,
  forgetTransientFailures,
  loadMermaid,
  releaseDiagrams,
  renderDiagram,
  resetMermaidForTest,
  showDiagrams,
  subscribeTheme,
} from "./markdownMermaidRender";

const SRC = "graph TD\nA-->B";

/**
 * The shape of a Mermaid flowchart with an HTML label, as Mermaid writes it:
 * HTML serialisation, so `<br>` has no close and U+00A0 is `&nbsp;`.
 */
const htmlLabelSvg = (id: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" id="${id}" viewBox="0 0 10 10"><g>` +
  '<foreignObject width="10" height="10"><div xmlns="http://www.w3.org/1999/xhtml">' +
  '<span class="nodeLabel"><p>Frontend<br>React&nbsp;19</p></span></div>' +
  "</foreignObject></g></svg>";

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
  vi.mocked(logDebug).mockClear();
  delete document.documentElement.dataset.theme;
});

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

describe("rendering configuration", () => {
  it("EDT-FR-JBZU, EDT-FR-JIBV: every render runs at the strict security level in the light theme", async () => {
    const result = await renderDiagram(SRC, "light");
    expect(result?.state).toBe("rendered");
    expect(mermaidApi.initialize).toHaveBeenCalledWith(
      expect.objectContaining({
        startOnLoad: false,
        securityLevel: "strict",
        suppressErrorRendering: true,
        theme: "default",
      }),
    );
  });

  it("EDT-FR-JIBV: the dark theme renders with the dark Mermaid theme and configures once per theme", async () => {
    await renderDiagram("graph LR\nA-->B", "dark");
    await renderDiagram("graph LR\nB-->C", "dark");
    expect(mermaidApi.initialize).toHaveBeenCalledTimes(1);
    expect(mermaidApi.initialize).toHaveBeenLastCalledWith(
      expect.objectContaining({ theme: "dark", securityLevel: "strict" }),
    );
    await renderDiagram("graph LR\nA-->B", "light");
    expect(mermaidApi.initialize).toHaveBeenCalledTimes(2);
  });

  it("EDT-FR-JIBV: the cache holds one result per theme", async () => {
    await renderDiagram(SRC, "light");
    expect(cachedDiagram(SRC, "dark")).toBeUndefined();
    await renderDiagram(SRC, "dark");
    expect(mermaidApi.render).toHaveBeenCalledTimes(2);
    await renderDiagram(SRC, "light");
    expect(mermaidApi.render).toHaveBeenCalledTimes(2);
  });

  it("EDT-FR-MOYS: each render gets a new id", async () => {
    await renderDiagram("graph TD\nA", "light");
    await renderDiagram("graph TD\nB", "light");
    expect(mermaidApi.render.mock.calls.map((call) => call[0])).toEqual([
      "synthesis-mermaid-1",
      "synthesis-mermaid-2",
    ]);
  });
});

describe("the render queue", () => {
  it("EDT-FR-MOYS: renders run one at a time, in the order they were asked for", async () => {
    const held = holdNextRender();
    const first = renderDiagram("graph TD\nfirst", "light");
    const second = renderDiagram("graph TD\nsecond", "light");
    await vi.waitFor(() => expect(mermaidApi.render).toHaveBeenCalledTimes(1));
    held.release();
    await Promise.all([first, second]);
    expect(mermaidApi.render.mock.calls.map((call) => call[1])).toEqual([
      "graph TD\nfirst",
      "graph TD\nsecond",
    ]);
  });

  it("EDT-FR-MOYS: two requests for one source and theme share one render", async () => {
    const [a, b] = await Promise.all([
      renderDiagram(SRC, "light"),
      renderDiagram(SRC, "light"),
    ]);
    expect(mermaidApi.render).toHaveBeenCalledTimes(1);
    expect(a).toBe(b);
  });

  it("EDT-FR-MOYS: a source nobody shows any more when its turn comes is skipped and not remembered", async () => {
    let shown = true;
    const held = holdNextRender();
    const blocker = renderDiagram("graph TD\nblocker", "light");
    const skipped = renderDiagram(SRC, "light", () => shown);
    shown = false;
    held.release();
    await blocker;
    expect(await skipped).toBeNull();
    expect(mermaidApi.render).toHaveBeenCalledTimes(1);
    expect(cachedDiagram(SRC, "light")).toBeUndefined();
  });

  it("EDT-FR-MOYS: a render runs when any one of its callers still shows the source", async () => {
    const held = holdNextRender();
    const blocker = renderDiagram("graph TD\nblocker", "light");
    const gone = renderDiagram(SRC, "light", () => false);
    const kept = renderDiagram(SRC, "light", () => true);
    held.release();
    await blocker;
    expect((await gone)?.state).toBe("rendered");
    expect((await kept)?.state).toBe("rendered");
  });

  it("EDT-FR-XTEB: a failed render does not stop the renders after it", async () => {
    const failed = renderDiagram("INVALID", "light");
    const next = renderDiagram(SRC, "light");
    expect(await failed).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.syntax,
      detail: "Parse error on line 1: INVALID",
    });
    expect((await next)?.state).toBe("rendered");
  });

  it("EDT-FR-XTEB: a throwing interest check gives a skip, not a rejection", async () => {
    const result = await renderDiagram(SRC, "light", () => {
      throw new Error("boom");
    });
    expect(result).toBeNull();
  });

  it("EDT-FR-XTEB: a configuration that throws gives an error result, not a rejection", async () => {
    mermaidApi.initialize.mockImplementationOnce(() => {
      throw new Error("bad config");
    });
    expect(await renderDiagram(SRC, "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.draw,
      detail: "bad config",
    });
    // The next render configures again rather than trusting the failed one.
    expect((await renderDiagram("graph TD\nX", "light"))?.state).toBe("rendered");
    expect(mermaidApi.initialize).toHaveBeenCalledTimes(2);
  });
});

describe("render errors", () => {
  it("EDT-FR-XKMP: a parse failure names the syntax step and gives the reason of Mermaid", async () => {
    overrideNextParse(async () => {
      throw new Error("Parse error on line 2:\nA-->\n----^\nExpecting 'NODE_STRING', got 'EOF'");
    });
    expect(await renderDiagram(SRC, "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.syntax,
      detail: "Parse error on line 2:\nA-->\n----^\nExpecting 'NODE_STRING', got 'EOF'",
    });
    // A source that does not parse is never drawn.
    expect(mermaidApi.render).not.toHaveBeenCalled();
  });

  it("EDT-FR-XKMP: a draw failure after a valid parse names the draw step", async () => {
    overrideNextRender(async () => {
      throw new Error("Cannot read properties of undefined (reading 'x')");
    });
    expect(await renderDiagram(SRC, "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.draw,
      detail: "Cannot read properties of undefined (reading 'x')",
    });
  });

  it("EDT-FR-XKMP: an empty reason is left out, and a thrown string is the reason", async () => {
    overrideNextRender(async () => {
      throw new Error("   ");
    });
    expect(await renderDiagram("graph TD\nA", "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.draw,
    });
    overrideNextParse(async () => {
      throw "  plain failure  ";
    });
    expect(await renderDiagram("graph TD\nB", "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.syntax,
      detail: "plain failure",
    });
  });

  it("EDT-FR-XTEB, EDT-FR-XKMP: an output with no SVG is a read error, not an empty diagram", async () => {
    overrideNextRender(async () => ({ svg: "<div>no drawing</div>" }));
    expect(await renderDiagram(SRC, "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.read,
      detail: "The output of Mermaid has no <svg> element.",
    });
  });

  it("EDT-FR-XTEB, EDT-FR-XKMP: a truncated output is a read error", async () => {
    overrideNextRender(async () => ({ svg: "<svg" }));
    expect(await renderDiagram(SRC, "light")).toEqual({
      state: "error",
      summary: DIAGRAM_ERROR.read,
      detail: "The output of Mermaid has no <svg> element.",
    });
  });

  it("EDT-FR-LXFD: an SVG with HTML line breaks and entities in its labels renders", async () => {
    overrideNextRender(async (id) => ({ svg: htmlLabelSvg(id) }));
    const result = await renderDiagram(SRC, "light");
    expect(result?.state).toBe("rendered");
  });

  it("EDT-FR-XTEB: the temporary container a failed render leaves is removed", async () => {
    overrideNextRender(async (id) => {
      const leftover = document.createElement("div");
      leftover.id = `d${id}`;
      document.body.appendChild(leftover);
      throw new Error("failed");
    });
    await renderDiagram(SRC, "light");
    expect(document.getElementById("dsynthesis-mermaid-1")).toBeNull();
  });

  it("EDT-FR-JBZU: a failed render logs the shape of the source, never its text or the message", async () => {
    await renderDiagram("INVALID secret words", "light");
    expect(logDebug).toHaveBeenCalledTimes(1);
    const payload = JSON.stringify(vi.mocked(logDebug).mock.calls[0]);
    expect(payload).not.toContain("secret");
    expect(payload).not.toContain("Parse error");
    expect(vi.mocked(logDebug).mock.calls[0][2]).toEqual({
      step: "syntax",
      length: 20,
      theme: "light",
    });
  });
});

describe("the result cache", () => {
  it("EDT-FR-MOYS: the cache keeps the 64 latest results and a read counts as a use", async () => {
    await renderDiagram("graph TD\nkeep", "light");
    for (let i = 0; i < 63; i++) await renderDiagram(`graph TD\nn${i}`, "light");
    // A read makes "keep" the latest, so the next result evicts "n0" instead.
    expect(cachedDiagram("graph TD\nkeep", "light")).toBeDefined();
    await renderDiagram("graph TD\nlast", "light");
    expect(cachedDiagram("graph TD\nkeep", "light")).toBeDefined();
    expect(cachedDiagram("graph TD\nn0", "light")).toBeUndefined();
    expect(cachedDiagram("graph TD\nlast", "light")).toBeDefined();
  });
});

describe("diagrams on screen", () => {
  it("EDT-FR-MOYS: a diagram a surface shows is never evicted, however many there are", async () => {
    const owner = {};
    const sources = Array.from({ length: 70 }, (_, i) => `graph TD\nshown${i}`);
    showDiagrams(owner, sources.map((source) => [source, "light"] as const));
    for (const source of sources) await renderDiagram(source, "light");
    for (const source of sources) expect(cachedDiagram(source, "light")).toBeDefined();

    releaseDiagrams(owner);
    await renderDiagram("graph TD\nafter", "light");
    expect(cachedDiagram(sources[0], "light")).toBeUndefined();
    expect(cachedDiagram("graph TD\nafter", "light")).toBeDefined();
  });
});

describe("loading the library", () => {
  it("EDT-FR-JIBV: two loads share one import", () => {
    expect(loadMermaid()).toBe(loadMermaid());
  });

  it("EDT-FR-XTEB: forgetting failed loads keeps every other result", async () => {
    await renderDiagram(SRC, "light");
    await renderDiagram("INVALID", "light");
    forgetTransientFailures();
    expect(cachedDiagram(SRC, "light")?.state).toBe("rendered");
    expect(cachedDiagram("INVALID", "light")?.state).toBe("error");
  });

  it("EDT-FR-JIBV: the renderer imports the bundled library and names no network host", () => {
    const files = [
      "src/components/markdownMermaidRender.ts",
      "src/components/markdownMermaid.ts",
      "src/components/DiffView/MermaidStatic.tsx",
    ];
    for (const file of files) {
      const text = readFileSync(resolve(process.cwd(), file), "utf8");
      expect(text).not.toMatch(/https?:\/\//);
      expect(text).not.toMatch(/\bfetch\(/);
    }
    const render = readFileSync(
      resolve(process.cwd(), "src/components/markdownMermaidRender.ts"),
      "utf8",
    );
    expect(render).toContain('import("mermaid")');
    const pkg = JSON.parse(readFileSync(resolve(process.cwd(), "package.json"), "utf8"));
    expect(pkg.dependencies.mermaid).toBeDefined();
  });
});

describe("the SVG element", () => {
  const svg = (body: string, attrs = "") =>
    `<svg xmlns="http://www.w3.org/2000/svg" id="m-1" ${attrs}>${body}</svg>`;

  it("EDT-FR-JBZU: a valid SVG becomes a new element of this document on each call", () => {
    const a = diagramElement(svg("<g/>"), "m-1");
    const b = diagramElement(svg("<g/>"), "m-1");
    expect(a?.namespaceURI).toBe("http://www.w3.org/2000/svg");
    expect(a?.ownerDocument).toBe(document);
    expect(a).not.toBe(b);
  });

  it("EDT-FR-JBZU: markup that does not start with an SVG gives nothing", () => {
    expect(diagramElement('<div xmlns="http://www.w3.org/1999/xhtml"/>', "m-1")).toBeNull();
    expect(diagramElement("<p>text</p><svg></svg>", "m-1")).toBeNull();
    expect(diagramElement("", "m-1")).toBeNull();
    expect(diagramElement("<math><svg></svg></math>", "m-1")).toBeNull();
  });

  it("EDT-FR-LXFD: an HTML label with a line break and a non-breaking space keeps both", () => {
    const el = diagramElement(htmlLabelSvg("m-1"), "m-1")!;
    expect(el).not.toBeNull();
    expect(el.namespaceURI).toBe("http://www.w3.org/2000/svg");
    const label = el.querySelector("foreignObject p")!;
    expect(label.namespaceURI).toBe("http://www.w3.org/1999/xhtml");
    expect(label.querySelectorAll("br")).toHaveLength(1);
    expect(label.textContent).toBe("FrontendReact\u00a019");
    // The HTML parser keeps the case of SVG names.
    expect(el.getAttribute("viewBox")).toBe("0 0 10 10");
    expect(el.querySelector("foreignObject")?.localName).toBe("foreignObject");
  });

  it("EDT-FR-XTEB: an SVG with no closing tags is closed by the parser and shows what it holds", () => {
    // The HTML parser closes open elements at the end of the input, as a
    // browser does for any page. Mermaid never writes such output.
    const el = diagramElement('<svg xmlns="http://www.w3.org/2000/svg" id="m-1"><g><text>t', "m-1");
    expect(el?.querySelector("g text")?.textContent).toBe("t");
  });

  it("EDT-FR-JBZU: elements that change the document, a form, and a template are removed from a label", () => {
    const el = diagramElement(
      svg(
        '<foreignObject><div xmlns="http://www.w3.org/1999/xhtml">' +
          '<base href="https://evil.invalid/"><meta http-equiv="refresh" content="0;url=https://evil.invalid">' +
          '<form action="https://evil.invalid"><input></form>' +
          '<button formaction="javascript:evil()">b</button>' +
          "<template><img src=x onerror=evil()><script>evil()</script></template>label</div></foreignObject>",
      ),
      "m-1",
    )!;
    const names = Array.from(el.getElementsByTagName("*")).map((e) => e.localName);
    for (const name of ["base", "meta", "form", "template", "script"]) {
      expect(names).not.toContain(name);
    }
    expect(el.querySelector("button")?.getAttribute("formaction")).toBeNull();
    expect(el.textContent).toContain("label");
  });

  it("EDT-FR-JBZU: an import spelled with a CSS escape and an SVG data URL are removed", () => {
    const el = diagramElement(
      svg(
        "<style>@\\69mport url(evil.css);</style><style>.ok{fill:red}</style>" +
          '<g style="@\\69mport url(evil.css)"/>' +
          '<use href="data:image/svg+xml,&lt;svg onload=evil()&gt;"/>' +
          '<image href="data:image/png;base64,AAAA"/>',
      ),
      "m-1",
    )!;
    const styles = Array.from(el.getElementsByTagName("style")).map((s) => s.textContent);
    expect(styles).toEqual([".ok{fill:red}"]);
    expect(el.querySelector("g")?.getAttribute("style")).toBeNull();
    expect(el.querySelector("use")?.getAttribute("href")).toBeNull();
    expect(el.querySelector("image")?.getAttribute("href")).toBe("data:image/png;base64,AAAA");
  });

  it("EDT-FR-JBZU: an HTML element in SVG content ends the SVG, and nothing after the SVG is kept", () => {
    const el = diagramElement(
      svg('<g><img src="x" onerror="evil()"><text>lost</text></g>') + "<p>after</p>",
      "m-1",
    )!;
    expect(el.getElementsByTagName("img")).toHaveLength(0);
    expect(el.textContent).not.toContain("lost");
    expect(el.textContent).not.toContain("after");
  });

  it("EDT-FR-JBZU: a script URL in an upper-case xlink:href is removed", () => {
    const el = diagramElement(svg('<a XLINK:HREF="javascript:evil()"><text>t</text></a>'), "m-1")!;
    const a = el.querySelector("a")!;
    expect(a.attributes).toHaveLength(0);
  });

  it("EDT-FR-JBZU: markup quoted in an attribute stays text, because the SVG is never parsed twice", () => {
    const el = diagramElement(
      svg(
        '<foreignObject><div xmlns="http://www.w3.org/1999/xhtml">' +
          '<noscript><p title="</noscript><img src=x onerror=evil()>">n</p></noscript></div></foreignObject>',
      ),
      "m-1",
    )!;
    expect(el.getElementsByTagName("img")).toHaveLength(0);
    expect(el.querySelector("noscript p")?.getAttribute("title")).toBe(
      "</noscript><img src=x onerror=evil()>",
    );
  });

  it("EDT-FR-JBZU: an event attribute and a script inside an HTML label are removed", () => {
    const el = diagramElement(
      svg(
        '<foreignObject><div xmlns="http://www.w3.org/1999/xhtml">' +
          '<img src="x" onerror="evil()"><br><script>evil()</script>label</div></foreignObject>',
      ),
      "m-1",
    )!;
    expect(el.querySelector("img")?.getAttribute("onerror")).toBeNull();
    expect(el.getElementsByTagName("script")).toHaveLength(0);
    expect(el.textContent).toContain("label");
  });

  it("EDT-FR-JBZU: scripts, event attributes and script URLs are removed", () => {
    const el = diagramElement(
      svg(
        '<script>window.__x = 1</script><g onclick="evil()"><a href="javascript:evil()"><text>t</text></a></g>',
        'onload="evil()"',
      ),
      "m-1",
    )!;
    expect(el.getElementsByTagName("script")).toHaveLength(0);
    expect(el.getAttribute("onload")).toBeNull();
    expect(el.querySelector("g")?.getAttribute("onclick")).toBeNull();
    expect(el.querySelector("a")?.getAttribute("href")).toBeNull();
  });

  it("EDT-FR-JBZU: a script URL hidden by control characters, animation of links, and style imports are removed", () => {
    const el = diagramElement(
      svg(
        '<a href="java\tscript:evil()"><text>t</text></a>' +
          '<animate attributeName="href" to="javascript:evil()"/><set attributeName="href"/>' +
          "<style>@import url(evil.css);</style><style>.ok{fill:red}</style>" +
          '<g style="background:url(javascript:evil())"/><image href="data:image/png;base64,AAAA"/>',
      ),
      "m-1",
    )!;
    expect(el.querySelector("a")?.getAttribute("href")).toBeNull();
    expect(el.getElementsByTagName("animate")).toHaveLength(0);
    expect(el.getElementsByTagName("set")).toHaveLength(0);
    const styles = Array.from(el.getElementsByTagName("style")).map((s) => s.textContent);
    expect(styles).toEqual([".ok{fill:red}"]);
    expect(el.querySelector("g")?.getAttribute("style")).toBeNull();
    expect(el.querySelector("image")?.getAttribute("href")).toBe("data:image/png;base64,AAAA");
  });

  it("EDT-FR-JBZU: a prefixed script and a document frame inside a label are removed", () => {
    const el = diagramElement(
      '<svg xmlns="http://www.w3.org/2000/svg" xmlns:svg="http://www.w3.org/2000/svg" id="m-1">' +
        "<svg:script>window.__x = 1</svg:script>" +
        '<foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><span>label</span>' +
        '<iframe srcdoc="&lt;script&gt;evil()&lt;/script&gt;"></iframe>' +
        '<object data="evil.swf"></object><embed src="evil.swf"/></div></foreignObject></svg>',
      "m-1",
    )!;
    const names = Array.from(el.getElementsByTagName("*")).map((e) => e.localName);
    expect(names).not.toContain("script");
    expect(names).not.toContain("iframe");
    expect(names).not.toContain("object");
    expect(names).not.toContain("embed");
    expect(el.textContent).toContain("label");
  });

  it("EDT-FR-MOYS: each copy of one diagram gets its own ids", () => {
    const markup = svg('<marker id="m-1_arrow"/><path marker-end="url(#m-1_arrow)"/>');
    const a = diagramElement(markup, "m-1")!;
    const b = diagramElement(markup, "m-1")!;
    expect(a.id).not.toBe(b.id);
    expect(a.querySelector("marker")?.id).not.toBe(b.querySelector("marker")?.id);
    const ref = a.querySelector("path")?.getAttribute("marker-end");
    expect(ref).toBe(`url(#${a.querySelector("marker")?.id})`);
  });
});

describe("the theme", () => {
  it("EDT-FR-JIBV: the theme is read from the document, light by default", () => {
    expect(currentTheme()).toBe("light");
    document.documentElement.dataset.theme = "dark";
    expect(currentTheme()).toBe("dark");
  });

  it("EDT-FR-JIBV: a listener hears each change of theme once, and nothing after it stops", async () => {
    const heard: string[] = [];
    const stop = subscribeTheme((theme) => heard.push(theme));
    document.documentElement.dataset.theme = "dark";
    await vi.waitFor(() => expect(heard).toEqual(["dark"]));
    document.documentElement.dataset.theme = "dark";
    document.documentElement.setAttribute("lang", "en");
    document.documentElement.dataset.theme = "light";
    await vi.waitFor(() => expect(heard).toEqual(["dark", "light"]));
    stop();
    document.documentElement.dataset.theme = "dark";
    await Promise.resolve();
    expect(heard).toEqual(["dark", "light"]);
    document.documentElement.removeAttribute("lang");
  });
});

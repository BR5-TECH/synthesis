import { describe, expect, it } from "vitest";

import { STREAM_ERRORS } from "../../types";
import { refusalText } from "./refusals";

describe("a merge refused for a missing Docker socket (WSS-FR-NPXC, WKS-FR-BPGM)", () => {
  it("WSS-FR-NPXC, GSD-FR-BXKR: the row says it in words, names the path, and says nothing was written", () => {
    const text = refusalText(`${STREAM_ERRORS.dockerSocketUnavailable}: /run/x/docker.sock`);
    expect(text).toContain("/run/x/docker.sock");
    expect(text).toContain("Start Docker or restore the socket");
    expect(text).toContain("Project settings");
    expect(text).toContain("Nothing was written.");
    expect(text).not.toContain(STREAM_ERRORS.dockerSocketUnavailable);
  });

  it("WSS-FR-NPXC: a refusal that carries no path still reads as a sentence", () => {
    const text = refusalText(STREAM_ERRORS.dockerSocketUnavailable);
    expect(text).toContain("configured path");
  });
});

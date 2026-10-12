import { describe, expect, it, vi } from "vitest";

import { parseAddress } from "./notificationAddress";
import {
  DOCKER_SOCKET_TOAST_KEY,
  dockerSocketRaise,
  onDockerSocketRefusal,
  reportDockerSocketRefusal,
} from "./dockerSocketRefusal";

describe("the Docker socket refusal raise (GSD-FR-BXKR, NTF-FR-24)", () => {
  it("GSD-FR-BXKR, NTF-FR-24: the raise is Error, names the path, and addresses the Docker section of Project settings", () => {
    const raise = dockerSocketRaise("/run/x/docker.sock", "/dev/acme", "/dev/acme");
    expect(raise.level).toBe("Error");
    expect(raise.key).toBe(DOCKER_SOCKET_TOAST_KEY);
    expect(raise.body).toContain("/run/x/docker.sock");
    expect(parseAddress(raise.address)).toEqual({
      projectKey: "/dev/acme",
      worktree: "/dev/acme",
      target: { kind: "settings", scope: "project", section: "docker" },
    });
  });

  it("GSD-FR-BXKR: a refusal that carries no path still gives a sentence", () => {
    expect(dockerSocketRaise(null, "/dev/acme", "/dev/acme").body).toContain(
      "restore the socket",
    );
  });

  it("GSD-FR-BXKR: a report reaches every listener until it stops listening", () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = onDockerSocketRefusal(first);
    const stopSecond = onDockerSocketRefusal(second);
    reportDockerSocketRefusal("/a.sock");
    stopFirst();
    reportDockerSocketRefusal("/b.sock");
    stopSecond();
    reportDockerSocketRefusal("/c.sock");

    expect(first.mock.calls).toEqual([["/a.sock"]]);
    expect(second.mock.calls).toEqual([["/a.sock"], ["/b.sock"]]);
  });
});

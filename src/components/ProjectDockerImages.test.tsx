import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The two build channels, driven by hand: a test plays the backend's own events
 * rather than waiting on a Docker daemon that is not there.
 */
const listeners = new Map<string, (payload: unknown) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, (payload) => handler({ payload }));
    return Promise.resolve(() => listeners.delete(name));
  },
  emit: () => Promise.resolve(),
}));

import { BUILD_LOG_LIMIT, ProjectDockerImages } from "./ProjectDockerImages";
import type {
  AgenticCliVendor,
  ImageBuildFinished,
  ImageBuildProgress,
  ProjectVendorImageStatus,
} from "../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const emitProgress = (payload: ImageBuildProgress) =>
  listeners.get("project-image-build-progress")?.(payload);
const emitFinished = (payload: ImageBuildFinished) =>
  listeners.get("project-image-build-finished")?.(payload);

function unset(vendor: AgenticCliVendor): ProjectVendorImageStatus {
  return {
    vendor,
    configuration: "unset",
    imageName: "",
    tag: null,
    imageReference: null,
    dockerfile: null,
    dockerfileState: "absent",
    dockerfileProblem: null,
    graduationState:
      vendor === "opencode" ? "execution_unsupported" : "image_name_missing",
  };
}

function configured(
  vendor: AgenticCliVendor,
  patch: Partial<ProjectVendorImageStatus> = {},
): ProjectVendorImageStatus {
  return {
    ...unset(vendor),
    configuration: "configured",
    imageName: "acme/agent",
    imageReference: "acme/agent",
    graduationState: vendor === "opencode" ? "execution_unsupported" : "usable",
    ...patch,
  };
}

interface Scripted {
  statuses?: ProjectVendorImageStatus[];
  verified?: boolean;
  resolved?: string | null;
  save?: (vendor: string, entry: unknown) => ProjectVendorImageStatus;
  build?: () => { operationId: string };
  /** PSS-FR-29: the build the project already has in flight, if any. */
  inFlight?: { vendor: AgenticCliVendor; operationId: string } | null;
}

function backend(options: Scripted = {}) {
  const statuses =
    options.statuses ?? [unset("claude_code"), unset("codex"), unset("opencode")];
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    switch (cmd) {
      case "load_project_docker_images":
        return statuses;
      case "load_docker_backend":
        return {
          mode: "bollard",
          endpoint: "automatic",
          cliPath: null,
          state: options.verified === false ? "unverified" : "verified",
          serverVersion: "27.1.1",
          verifiedAt: "2026-01-01T00:00:00Z",
        };
      case "get_project_agentic_integration":
        return {
          vendor: options.resolved === undefined ? "claude_code" : options.resolved,
          resolution: "inherited",
          override: null,
        };
      case "load_project_tree":
        return {
          id: "root",
          name: "project",
          path: "",
          nodeKind: "folder",
          children: [
            {
              id: "d",
              name: "docker",
              path: "docker",
              nodeKind: "folder",
              children: [
                {
                  id: "f",
                  name: "agent.Dockerfile",
                  path: "docker/agent.Dockerfile",
                  nodeKind: "file",
                },
              ],
            },
          ],
        };
      case "save_project_vendor_image": {
        const { vendor, entry } = args as { vendor: string; entry: unknown };
        if (!options.save) throw new Error("image_name_empty");
        return options.save(vendor, entry);
      }
      case "load_project_image_build_in_flight":
        return options.inFlight ?? null;
      case "build_project_vendor_image":
        if (!options.build) throw new Error("docker_backend_unverified");
        return options.build();
      case "cancel_project_vendor_image_build":
        return undefined;
      default:
        return undefined;
    }
  });
}

const tab = (name: string) => screen.getByRole("tab", { name });
const buildButton = () =>
  screen.getByRole("button", { name: /^(Build|Retry)$/ });

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
});
afterEach(cleanup);

describe("Project settings → Docker (SET-FR-21..SET-FR-27)", () => {
  it("SET-FR-03, SET-FR-21, SET-FR-25: three tabs, Claude Code selected, empty fields, Build disabled", async () => {
    backend({});
    render(<ProjectDockerImages />);

    await waitFor(() =>
      expect(calls("load_project_docker_images").length).toBeGreaterThan(0),
    );
    for (const name of ["Claude Code", "Codex", "OpenCode"]) {
      expect(tab(name)).toBeInTheDocument();
    }
    expect(tab("Claude Code")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByLabelText("Image name")).toHaveValue("");
    expect(screen.getByLabelText("Tag (optional)")).toHaveValue("");
    expect(screen.getByLabelText("Dockerfile (optional)")).toHaveValue("");
    expect(buildButton()).toBeDisabled();
  });

  it("SET-FR-22, SET-FR-24, SET-FR-08: fields persist at once, and a refused path renders inline", async () => {
    const user = userEvent.setup();
    const saved: unknown[] = [];
    backend({
      save: (vendor, entry) => {
        saved.push({ vendor, entry });
        const value = entry as { dockerfile?: string | null };
        if (value.dockerfile?.startsWith("/")) {
          throw new Error("dockerfile_absolute");
        }
        return configured("claude_code", {
          imageName: "acme/agent",
          tag: "2.1",
          imageReference: "acme/agent:2.1",
        });
      },
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(tab("Claude Code")).toBeInTheDocument());

    await user.type(screen.getByLabelText("Image name"), "acme/agent");
    await user.tab();
    await waitFor(() =>
      expect(calls("save_project_vendor_image")).toHaveLength(1),
    );
    // SET-FR-24: no dirty state — the write happened on its own, with no Save
    // control anywhere in the section.
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();

    // SET-FR-22: an absolute path is refused and named.
    await user.type(
      screen.getByLabelText("Dockerfile (optional)"),
      "/etc/Dockerfile",
    );
    await user.tab();
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /That path is absolute/,
      ),
    );
  });

  it("SET-FR-23: an absent Dockerfile, an invalid one, and OpenCode read differently", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        unset("claude_code"),
        // Configured, no Dockerfile: not a problem.
        configured("codex", { imageReference: "acme/codex" }),
        // Configured with a Dockerfile that is gone.
        configured("opencode", {
          dockerfile: "docker/gone.Dockerfile",
          dockerfileState: "invalid",
          dockerfileProblem: "not_a_regular_file",
        }),
      ],
      resolved: null,
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(tab("Codex")).toBeInTheDocument());

    await user.click(tab("Codex"));
    expect(screen.getByRole("tabpanel")).toHaveTextContent(
      /No Dockerfile is set, so nothing is built here/,
    );
    // An absent Dockerfile is never rendered as a problem.
    expect(screen.queryByRole("alert")).toBeNull();

    await user.click(tab("OpenCode"));
    const panel = screen.getByRole("tabpanel");
    // SET-FR-23: OpenCode states that execution support is missing rather than
    // that its configuration is wrong…
    expect(panel).toHaveTextContent(/cannot run OpenCode yet/);
    expect(panel).not.toHaveTextContent(/still needs an image name/);
    // …and the invalid Dockerfile is still named, which is the other half of
    // the distinction.
    expect(screen.getByRole("alert")).toHaveTextContent(
      /does not name a file in this project/,
    );
  });

  it("SET-FR-24: the tab graduation needs is marked, and says what it still needs", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [unset("claude_code"), unset("codex"), unset("opencode")],
      resolved: "codex",
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(tab("Codex")).toBeInTheDocument());

    // Not this tab.
    await waitFor(() =>
      expect(screen.getByRole("tabpanel")).not.toHaveTextContent(
        /graduates through this agent/,
      ),
    );

    await user.click(tab("Codex"));
    await waitFor(() =>
      expect(screen.getByRole("tabpanel")).toHaveTextContent(
        /graduation cannot start until an image name is filled in here/,
      ),
    );

    await user.click(tab("OpenCode"));
    expect(screen.getByRole("tabpanel")).not.toHaveTextContent(
      /graduates through this agent/,
    );
  });

  it("SET-FR-25, SET-FR-26: one click builds, every tab's Build is disabled while it runs", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        configured("claude_code", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        configured("codex", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("opencode"),
      ],
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(buildButton()).toBeEnabled());

    await user.click(buildButton());
    await waitFor(() =>
      expect(calls("build_project_vendor_image")).toHaveLength(1),
    );

    // SET-FR-26: indeterminate until a total is reported, then determinate,
    // with the phase and message each update carries rendered.
    emitProgress({
      vendor: "claude_code",
      operationId: "op-1",
      phase: "building",
      message: "Sending build context",
    });
    await waitFor(() =>
      expect(screen.getByLabelText("Build progress")).not.toHaveAttribute(
        "value",
      ),
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "building: Sending build context",
    );

    emitProgress({
      vendor: "claude_code",
      operationId: "op-1",
      phase: "building",
      message: "Step 2/3 : RUN true",
      completed: 2,
      total: 3,
    });
    await waitFor(() =>
      expect(screen.getByLabelText("Build progress")).toHaveAttribute(
        "value",
        "2",
      ),
    );

    // SET-FR-25: while it runs, every vendor tab's Build is disabled —
    // whichever tab started it.
    await user.click(tab("Codex"));
    expect(buildButton()).toBeDisabled();
    await user.click(tab("Claude Code"));
    expect(buildButton()).toBeDisabled();

    emitFinished({
      vendor: "claude_code",
      operationId: "op-1",
      outcome: "succeeded",
      imageReference: "acme/agent",
    });
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("Built acme/agent."),
    );
    expect(buildButton()).toBeEnabled();
  });

  it("SET-FR-27, SET-FR-25, SET-FR-24: a failure keeps the configuration, names the error, and offers Retry", async () => {
    const user = userEvent.setup();
    let started = 0;
    backend({
      statuses: [
        configured("claude_code", {
          tag: "2.1",
          imageReference: "acme/agent:2.1",
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("codex"),
        unset("opencode"),
      ],
      build: () => {
        started += 1;
        return { operationId: `op-${started}` };
      },
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(buildButton()).toBeEnabled());

    await user.click(buildButton());
    emitFinished({
      vendor: "claude_code",
      operationId: "op-1",
      outcome: "failed",
      diagnostic: "no such file or directory",
    });

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "no such file or directory",
      ),
    );
    // SET-FR-27: nothing was cleared, reverted, or rewritten.
    expect(screen.getByLabelText("Image name")).toHaveValue("acme/agent");
    expect(screen.getByLabelText("Tag (optional)")).toHaveValue("2.1");
    expect(screen.getByLabelText("Dockerfile (optional)")).toHaveValue(
      "docker/agent.Dockerfile",
    );
    // Nothing retried on its own.
    expect(calls("build_project_vendor_image")).toHaveLength(1);

    const retry = screen.getByRole("button", { name: "Retry" });
    expect(retry).toBeEnabled();
    await user.click(retry);
    await waitFor(() =>
      expect(calls("build_project_vendor_image")).toHaveLength(2),
    );
  });

  it("SET-FR-25: Build is disabled and says so while the backend is unverified", async () => {
    backend({
      verified: false,
      statuses: [
        configured("claude_code", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("codex"),
        unset("opencode"),
      ],
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(buildButton()).toBeDisabled());
    expect(screen.getByRole("tabpanel")).toHaveTextContent(
      /Verify the Docker backend in Global settings first/,
    );
  });

  it("SET-FR-25, SET-FR-26 / SET-FR-27, SET-FR-24: a build started earlier is still rendered against its tab on re-entry", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        configured("claude_code", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        configured("codex", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("opencode"),
      ],
      // The project is already building Claude Code's image, started by a
      // window that has since been left (PSS-FR-29).
      inFlight: { vendor: "claude_code", operationId: "op-earlier" },
      build: () => ({ operationId: "op-new" }),
    });
    render(<ProjectDockerImages />);

    await waitFor(() =>
      expect(calls("load_project_image_build_in_flight").length).toBeGreaterThan(
        0,
      ),
    );
    // SET-FR-26: rendered against its own tab.
    await waitFor(() =>
      expect(screen.getByLabelText("Build progress")).toBeInTheDocument(),
    );
    // SET-FR-25: and every tab's Build is disabled while it runs.
    expect(buildButton()).toBeDisabled();
    await user.click(tab("Codex"));
    expect(buildButton()).toBeDisabled();
    expect(screen.queryByLabelText("Build progress")).toBeNull();

    // The build it already knew about is the one the events then finish.
    emitFinished({
      vendor: "claude_code",
      operationId: "op-earlier",
      outcome: "succeeded",
      imageReference: "acme/agent",
    });
    await waitFor(() => expect(buildButton()).toBeEnabled());
  });

  it("SET-FR-26: build output accumulates in a scrollable log and outlives the result", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        configured("claude_code", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("codex"),
        unset("opencode"),
      ],
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(buildButton()).toBeEnabled());

    await user.click(buildButton());
    for (const message of ["Step 1/3 : FROM node", "Step 2/3 : RUN true"]) {
      emitProgress({
        vendor: "claude_code",
        operationId: "op-1",
        phase: "building",
        message,
      });
    }

    // Every update is kept, in arrival order — not only the last one.
    const log = await screen.findByRole("log", { name: "Build output" });
    await waitFor(() =>
      expect(log).toHaveTextContent(/Step 1\/3 : FROM node/),
    );
    expect(log).toHaveTextContent(/Step 2\/3 : RUN true/);
    expect(
      log.textContent!.indexOf("Step 1/3"),
    ).toBeLessThan(log.textContent!.indexOf("Step 2/3"));
    // The log itself never announces: a build that speaks every layer it pulls
    // is unusable with a screen reader. The region around it does.
    expect(log).toHaveAttribute("aria-live", "off");

    emitFinished({
      vendor: "claude_code",
      operationId: "op-1",
      outcome: "failed",
      diagnostic: "no such file or directory",
    });

    // SET-FR-27: the output that explains the failure is still readable beside
    // the failure itself.
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "no such file or directory",
      ),
    );
    expect(screen.getByRole("log")).toHaveTextContent(/Step 2\/3 : RUN true/);
  });

  it("SET-FR-26: the log is capped, keeping the newest lines", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        configured("claude_code", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("codex"),
        unset("opencode"),
      ],
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(buildButton()).toBeEnabled());
    await user.click(buildButton());

    for (let step = 0; step < BUILD_LOG_LIMIT + 20; step += 1) {
      emitProgress({
        vendor: "claude_code",
        operationId: "op-1",
        phase: "building",
        message: `line ${step}`,
      });
    }

    const log = await screen.findByRole("log", { name: "Build output" });
    await waitFor(() =>
      expect(log).toHaveTextContent(`line ${BUILD_LOG_LIMIT + 19}`),
    );
    // The end of a build is where its failure is written, so the start is what
    // is dropped: the first twenty lines are gone and the log begins at the
    // twenty-first.
    expect(log.children).toHaveLength(BUILD_LOG_LIMIT);
    expect(log.children[0]).toHaveTextContent("building: line 20");
    expect(log.children[BUILD_LOG_LIMIT - 1]).toHaveTextContent(
      `building: line ${BUILD_LOG_LIMIT + 19}`,
    );
  });

  it("SET-FR-24: the backend line states the backend and opens where it is set up", async () => {
    const user = userEvent.setup();
    backend({ verified: false });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(tab("Claude Code")).toBeInTheDocument());

    await waitFor(() =>
      expect(screen.getByText("Docker backend not verified")).toBeInTheDocument(),
    );
    // The pointer is followable rather than only readable, and it names the
    // section it opens.
    await user.click(
      screen.getByRole("button", { name: "Global settings → Docker" }),
    );
    await waitFor(() => expect(calls("open_settings_window")).toHaveLength(1));
    expect(calls("open_settings_window")[0][1]).toEqual({
      kind: "global",
      section: "docker",
    });
  });

  it("SET-FR-24: a verified backend reads as verified", async () => {
    backend({});
    render(<ProjectDockerImages />);
    await waitFor(() =>
      expect(screen.getByText("Docker backend verified")).toBeInTheDocument(),
    );
  });

  it("PSS-FR-28: a running build can be cancelled, and reports cancelled", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        configured("claude_code", {
          dockerfile: "docker/agent.Dockerfile",
          dockerfileState: "valid",
        }),
        unset("codex"),
        unset("opencode"),
      ],
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages />);
    await waitFor(() => expect(buildButton()).toBeEnabled());

    await user.click(buildButton());
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument(),
    );

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(calls("cancel_project_vendor_image_build")).toHaveLength(1),
    );
    expect(calls("cancel_project_vendor_image_build")[0][1]).toEqual({
      operationId: "op-1",
    });

    // The terminal result is the backend's, and it leaves the configuration
    // exactly as it was.
    emitFinished({
      vendor: "claude_code",
      operationId: "op-1",
      outcome: "cancelled",
    });
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        /The build was cancelled\. Nothing here changed\./,
      ),
    );
    expect(screen.getByLabelText("Image name")).toHaveValue("acme/agent");
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(buildButton()).toBeEnabled();
  });
});

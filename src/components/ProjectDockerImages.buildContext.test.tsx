import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { implementAllAgenticVendorsForTest } from "./agenticImplemented";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
  emit: () => Promise.resolve(),
}));
const openDialogMock = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openDialogMock(...args),
}));

import { ProjectDockerImages } from "./ProjectDockerImages";
import { resetLogBufferForTest } from "../logging";
import type {
  AgenticCliVendor,
  ProjectVendorImage,
  ProjectVendorImageStatus,
} from "../types";

const ROOT = "/work/app";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function status(
  vendor: AgenticCliVendor,
  patch: Partial<ProjectVendorImageStatus> = {},
): ProjectVendorImageStatus {
  return {
    vendor,
    configuration: "configured",
    imageName: "acme/agent",
    tag: null,
    imageReference: "acme/agent",
    dockerfile: "docker/agent.Dockerfile",
    dockerfileState: "valid",
    dockerfileProblem: null,
    buildContext: null,
    buildContextState: "absent",
    graduationState: "usable",
    ...patch,
  };
}

interface Scripted {
  statuses?: ProjectVendorImageStatus[];
  /** Answers a save, or rejects it with the typed refusal. */
  save?: (entry: ProjectVendorImage, vendor: AgenticCliVendor) => ProjectVendorImageStatus;
  build?: () => { operationId: string };
}

function backend(options: Scripted = {}) {
  const statuses = options.statuses ?? [
    status("claude_code"),
    status("codex"),
    status("opencode"),
  ];
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    switch (cmd) {
      case "load_project_docker_images":
        return Promise.resolve(statuses);
      case "load_docker_backend":
        return Promise.resolve({
          mode: "bollard",
          endpoint: "automatic",
          cliPath: null,
          state: "verified",
          serverVersion: "27.1.1",
          verifiedAt: "2026-01-01T00:00:00Z",
        });
      case "get_project_agentic_integration":
        return Promise.resolve({ vendor: "claude_code", resolution: "inherited", override: null });
      case "load_project_tree":
        return Promise.resolve(null);
      case "load_project_image_build_in_flight":
        return Promise.resolve(null);
      case "load_docker_in_docker_settings":
        return Promise.resolve({
          enabled: false,
          socketPath: "",
          hostGateway: false,
          defaultSocketPath: "/var/run/docker.sock",
          support: "supported",
        });
      case "append_log_records":
        return Promise.resolve();
      case "save_project_vendor_image": {
        const { entry, vendor } = args as { entry: ProjectVendorImage; vendor: AgenticCliVendor };
        if (!options.save) return Promise.reject(new Error("image_name_empty"));
        try {
          return Promise.resolve(options.save(entry, vendor));
        } catch (e) {
          return Promise.reject(e);
        }
      }
      case "build_project_vendor_image":
        return options.build
          ? Promise.resolve(options.build())
          : Promise.reject(new Error("build_context_not_a_directory"));
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
}

const contextInput = () => screen.getByLabelText("Build context (optional)");
const savedEntries = () =>
  calls("save_project_vendor_image").map((c) => (c[1] as { entry: ProjectVendorImage }).entry);

beforeEach(() => {
  invokeMock.mockReset();
  openDialogMock.mockReset();
  resetLogBufferForTest();
});
afterEach(cleanup);

describe("Project settings → Docker → Build context (SET-FR-KDRP, SET-FR-FPCX, SET-FR-HYTR)", () => {
  it("SET-FR-21, SET-FR-KDRP: the empty field shows the project-root placeholder", async () => {
    backend();
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());
    expect(contextInput()).toHaveValue("");
    expect(contextInput()).toHaveAttribute("placeholder", "Project root (default)");
  });

  it("SET-FR-HYTR: each picker button sits inside its own field, has a name, and follows the field in the tab order", async () => {
    const user = userEvent.setup();
    backend();
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    const dockerfileField = screen.getByLabelText("Dockerfile (optional)");
    const dockerfileFrame = dockerfileField.closest(".joined-field") as HTMLElement;
    const fileButton = within(dockerfileFrame).getByRole("button", { name: "Choose a Dockerfile" });
    const contextFrame = contextInput().closest(".joined-field") as HTMLElement;
    const folderButton = within(contextFrame).getByRole("button", {
      name: "Choose a build context folder",
    });
    expect(dockerfileFrame).not.toBe(contextFrame);

    dockerfileField.focus();
    await user.tab();
    expect(fileButton).toHaveFocus();
    await user.tab();
    expect(contextInput()).toHaveFocus();
    await user.tab();
    expect(folderButton).toHaveFocus();
  });

  it("SET-FR-KDRP: a typed context saves at once with the entry, and clearing it saves none", async () => {
    const user = userEvent.setup();
    backend({
      save: (entry) =>
        status("claude_code", {
          buildContext: entry.buildContext ?? null,
          buildContextState: entry.buildContext ? "valid" : "absent",
        }),
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.type(contextInput(), "../certs");
    await user.tab();
    await waitFor(() => expect(savedEntries()).toHaveLength(1));
    expect(savedEntries()[0]).toMatchObject({
      imageName: "acme/agent",
      dockerfile: "docker/agent.Dockerfile",
      buildContext: "../certs",
    });
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();

    await user.clear(contextInput());
    await user.tab();
    await waitFor(() => expect(savedEntries()).toHaveLength(2));
    expect(savedEntries()[1].buildContext).toBeNull();
  });

  it("SET-FR-FPCX, SET-FR-HYTR: the folder picker opens in the project folder, and a folder inside it is stored project-relative", async () => {
    const user = userEvent.setup();
    backend({
      save: (entry) =>
        status("claude_code", { buildContext: entry.buildContext ?? null, buildContextState: "valid" }),
    });
    openDialogMock.mockResolvedValue(`${ROOT}/docker/ctx`);
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Choose a build context folder" }));
    await waitFor(() => expect(savedEntries()).toHaveLength(1));
    expect(openDialogMock).toHaveBeenCalledWith(
      expect.objectContaining({ directory: true, defaultPath: ROOT }),
    );
    expect(savedEntries()[0].buildContext).toBe("docker/ctx");
    await waitFor(() => expect(contextInput()).toHaveValue("docker/ctx"));
  });

  it("SET-FR-FPCX: a folder outside the project is kept as selected, and the picker reopens in the current context folder", async () => {
    const user = userEvent.setup();
    backend({
      statuses: [
        status("claude_code", { buildContext: "docker/ctx", buildContextState: "valid" }),
        status("codex"),
        status("opencode"),
      ],
      save: (entry) =>
        status("claude_code", { buildContext: entry.buildContext ?? null, buildContextState: "valid" }),
    });
    openDialogMock.mockResolvedValue("/etc/certs");
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toHaveValue("docker/ctx"));

    await user.click(screen.getByRole("button", { name: "Choose a build context folder" }));
    await waitFor(() => expect(savedEntries()).toHaveLength(1));
    expect(openDialogMock).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: `${ROOT}/docker/ctx` }),
    );
    expect(savedEntries()[0].buildContext).toBe("/etc/certs");
    await waitFor(() => expect(contextInput()).toHaveValue("/etc/certs"));
  });

  it("SET-FR-HYTR: a cancelled picker changes nothing and saves nothing", async () => {
    const user = userEvent.setup();
    backend({ save: () => status("claude_code") });
    openDialogMock.mockResolvedValue(null);
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Choose a build context folder" }));
    await user.click(screen.getByRole("button", { name: "Choose a Dockerfile" }));
    await waitFor(() => expect(openDialogMock).toHaveBeenCalledTimes(2));
    await act(async () => {
      await Promise.resolve();
    });
    expect(savedEntries()).toHaveLength(0);
    expect(contextInput()).toHaveValue("");
    expect(screen.getByLabelText("Dockerfile (optional)")).toHaveValue("docker/agent.Dockerfile");
  });

  it("SET-FR-FPCX: a context that is not an existing folder is refused inline and the stored entry stays", async () => {
    const user = userEvent.setup();
    backend({
      save: () => {
        throw new Error("build_context_not_a_directory");
      },
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.type(contextInput(), "missing");
    await user.tab();
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "That build context does not name an existing folder.",
      ),
    );
    expect(contextInput()).toHaveValue("missing");
  });

  it("SET-FR-FPCX: a configured context that no longer names a folder is named in the tab", async () => {
    backend({
      statuses: [
        status("claude_code", { buildContext: "gone", buildContextState: "invalid" }),
        status("codex"),
        status("opencode"),
      ],
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toHaveValue("gone"));
    expect(screen.getByRole("alert")).toHaveTextContent(
      "That build context does not name an existing folder.",
    );
  });

  it("SET-FR-22, SET-FR-HYTR: the file picker opens in the project folder and fills a project-relative path", async () => {
    const user = userEvent.setup();
    backend({
      save: (entry) => status("claude_code", { dockerfile: entry.dockerfile ?? null }),
    });
    openDialogMock.mockResolvedValue(`${ROOT}/build/Dockerfile.agent`);
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Choose a Dockerfile" }));
    await waitFor(() => expect(savedEntries()).toHaveLength(1));
    expect(openDialogMock).toHaveBeenCalledWith(
      expect.objectContaining({ directory: false, defaultPath: ROOT }),
    );
    expect(savedEntries()[0].dockerfile).toBe("build/Dockerfile.agent");
  });

  it("SET-FR-22: a Dockerfile chosen outside the project is refused inline and the field stays", async () => {
    const user = userEvent.setup();
    backend({ save: () => status("claude_code") });
    openDialogMock.mockResolvedValue("/elsewhere/Dockerfile");
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Choose a Dockerfile" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(/That path leaves the project/),
    );
    expect(savedEntries()).toHaveLength(0);
    expect(screen.getByLabelText("Dockerfile (optional)")).toHaveValue("docker/agent.Dockerfile");
  });

  it("SET-FR-25: a build refused for its context says so in the tab", async () => {
    const user = userEvent.setup();
    backend();
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Build" })).toBeEnabled());

    expect(screen.queryByRole("alert")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() => expect(calls("build_project_vendor_image")).toHaveLength(1));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "That build context does not name an existing folder.",
      ),
    );
  });
  it("SET-FR-HYTR: each picker button opens its native picker from the keyboard", async () => {
    const user = userEvent.setup();
    backend();
    openDialogMock.mockResolvedValue(null);
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    screen.getByRole("button", { name: "Choose a Dockerfile" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(openDialogMock).toHaveBeenCalledTimes(1));
    expect(openDialogMock).toHaveBeenLastCalledWith(expect.objectContaining({ directory: false }));

    screen.getByRole("button", { name: "Choose a build context folder" }).focus();
    await user.keyboard(" ");
    await waitFor(() => expect(openDialogMock).toHaveBeenCalledTimes(2));
    expect(openDialogMock).toHaveBeenLastCalledWith(expect.objectContaining({ directory: true }));
  });

  it("SET-FR-FPCX: a folder chosen in the picker goes through the same check and a refusal keeps the entry", async () => {
    const user = userEvent.setup();
    backend({
      save: () => {
        throw new Error("build_context_not_a_directory");
      },
    });
    openDialogMock.mockResolvedValue(`${ROOT}/gone`);
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Choose a build context folder" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "That build context does not name an existing folder.",
      ),
    );
    expect(savedEntries()).toHaveLength(1);
    expect(contextInput()).toHaveValue("gone");
  });

  it("SET-FR-25: Build waits for the save of a field that is still being edited and uses its value", async () => {
    const user = userEvent.setup();
    backend({
      save: (entry) =>
        status("claude_code", {
          buildContext: entry.buildContext ?? null,
          buildContextState: "valid",
        }),
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.type(contextInput(), "certs");
    await user.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() => expect(calls("build_project_vendor_image")).toHaveLength(1));
    expect(savedEntries()[savedEntries().length - 1]?.buildContext).toBe("certs");
    const order = invokeMock.mock.calls.map((c) => c[0] as string);
    expect(order.indexOf("save_project_vendor_image")).toBeLessThan(
      order.indexOf("build_project_vendor_image"),
    );
  });

  it("SET-FR-25, SET-FR-FPCX: a refused save of the field starts no build and keeps its message", async () => {
    const user = userEvent.setup();
    backend({
      save: () => {
        throw new Error("build_context_not_a_directory");
      },
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.type(contextInput(), "missing");
    await user.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "That build context does not name an existing folder.",
      ),
    );
    expect(calls("build_project_vendor_image")).toHaveLength(0);
  });

  it("SET-FR-25: a refused save on one vendor's tab does not hold back the Build of another vendor", async () => {
    // AII-FR-OTQY / SET-FR-GVED: this test uses a vendor that the settings UI
    // does not implement yet, so it opens the vendor gate.
    implementAllAgenticVendorsForTest();
    const user = userEvent.setup();
    backend({
      save: (entry, vendor) => {
        if (vendor === "claude_code") throw new Error("build_context_not_a_directory");
        return status(vendor, { buildContext: entry.buildContext ?? null });
      },
      build: () => ({ operationId: "op-codex" }),
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.type(contextInput(), "missing");
    await user.click(screen.getByRole("tab", { name: "Codex" }));
    await waitFor(() => expect(savedEntries()).toHaveLength(1));

    await waitFor(() => expect(screen.getByRole("button", { name: "Build" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() => expect(calls("build_project_vendor_image")).toHaveLength(1));
    expect(calls("build_project_vendor_image")[0][1]).toMatchObject({ vendor: "codex" });
  });

  it("SET-FR-25, SET-FR-FPCX: a second Build after a refused save is held back again and its message stays in the tab", async () => {
    const user = userEvent.setup();
    backend({
      save: () => {
        throw new Error("build_context_not_a_directory");
      },
      build: () => ({ operationId: "op-1" }),
    });
    render(<ProjectDockerImages contentRoot={ROOT} />);
    await waitFor(() => expect(contextInput()).toBeInTheDocument());

    await user.type(contextInput(), "missing");
    await user.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Build" }));
    await waitFor(() => expect(savedEntries().length).toBeGreaterThanOrEqual(2));
    expect(calls("build_project_vendor_image")).toHaveLength(0);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "That build context does not name an existing folder.",
    );
  });
});

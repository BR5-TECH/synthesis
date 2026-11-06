import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { DockerInDockerState } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  IMAGE_REQUIREMENTS,
  ProjectDockerInDocker,
  dockerInDockerRefusalMessage,
} from "./ProjectDockerInDocker";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const state = (patch: Partial<DockerInDockerState> = {}): DockerInDockerState => ({
  enabled: false,
  socketPath: "",
  hostGateway: false,
  underMicrosandbox: "vm_daemon",
  defaultSocketPath: "/var/run/docker.sock",
  support: "supported",
  ...patch,
});

/** What the fake backend holds; a save writes it, a refusal leaves it. */
let stored: DockerInDockerState;
let refuseSave: string | null;
let isolationMode: "none" | "microsandbox";

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  refuseSave = null;
  isolationMode = "none";
  stored = state();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_docker_in_docker_settings") return stored;
    if (cmd === "load_isolation_settings") {
      return {
        settings: {
          mode: isolationMode,
          cpus: null,
          memoryGib: null,
          diskGib: null,
          blockLocalNetwork: true,
          allowedLocalHosts: [],
        },
        host: { microsandboxSupported: true, unsupportedReason: null, logicalCpus: 8, totalMemoryGib: 16, diskGib: 460 },
        effectiveCpus: 4,
        effectiveMemoryGib: 8,
        effectiveDiskGib: 64,
      };
    }
    if (cmd === "save_docker_in_docker_settings") {
      if (refuseSave) throw new Error(refuseSave);
      const { settings } = args as { settings: Partial<DockerInDockerState> };
      stored = { ...stored, ...settings };
      return undefined;
    }
    // The background log batch is answered; anything else is refused, as the
    // test rules ask.
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

const checkbox = () => screen.findByRole("checkbox", { name: "Docker-in-Docker" });

describe("Project settings → Docker → Docker-in-Docker (SET-FR-NXDV, SET-FR-JMHE, SET-FR-PQLA)", () => {
  it("SET-FR-PMHB: under Microsandbox with the checkbox off, no selector shows, and the checkbox stays editable", async () => {
    isolationMode = "microsandbox";
    render(<ProjectDockerInDocker />);
    const box = await checkbox();
    await waitFor(() => expect(calls("load_isolation_settings")).toHaveLength(1));
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(box).toBeEnabled();
    await userEvent.click(box);
    await waitFor(() => expect(calls("save_docker_in_docker_settings")).toHaveLength(1));
  });

  it("SET-FR-PMHB, SET-FR-JMHE: under Microsandbox with the checkbox on, the selector shows the daemon choice by default with both trade-offs, and no host socket controls", async () => {
    isolationMode = "microsandbox";
    stored = state({ enabled: true });
    render(<ProjectDockerInDocker />);
    const daemon = await screen.findByRole("radio", { name: /Docker daemon in the VM/ });
    const host = screen.getByRole("radio", { name: /No isolation: use the host Docker socket/ });
    expect(daemon).toBeChecked();
    expect(host).not.toBeChecked();
    expect(daemon).toHaveAccessibleDescription(/keeps the microVM isolation.*image must hold dockerd/is);
    expect(host).toHaveAccessibleDescription(/gives up the isolation.*host Docker socket/is);
    expect(screen.queryByLabelText("Docker socket path")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Host gateway" })).not.toBeInTheDocument();
    expect(screen.queryByText(/ignored/)).not.toBeInTheDocument();
  });

  it("SET-FR-PMHB, SET-FR-JMHE: selecting the host socket saves the whole record at once and shows the socket path and the Host gateway", async () => {
    const user = userEvent.setup();
    isolationMode = "microsandbox";
    stored = state({ enabled: true, socketPath: "/a/d.sock", hostGateway: true });
    render(<ProjectDockerInDocker />);
    await user.click(
      await screen.findByRole("radio", { name: /No isolation: use the host Docker socket/ }),
    );

    await waitFor(() => expect(stored.underMicrosandbox).toBe("host_socket"));
    expect(calls("save_docker_in_docker_settings")[0][1]).toEqual({
      settings: {
        enabled: true,
        socketPath: "/a/d.sock",
        hostGateway: true,
        underMicrosandbox: "host_socket",
      },
    });
    expect(await screen.findByLabelText("Docker socket path")).toHaveValue("/a/d.sock");
    expect(screen.getByRole("switch", { name: "Host gateway" })).toBeChecked();

    await user.click(screen.getByRole("radio", { name: /Docker daemon in the VM/ }));
    await waitFor(() => expect(stored.underMicrosandbox).toBe("vm_daemon"));
    expect(screen.queryByLabelText("Docker socket path")).not.toBeInTheDocument();
  });

  it("SET-FR-PMHB: a refused choice keeps the previous choice and renders the refusal", async () => {
    const user = userEvent.setup();
    isolationMode = "microsandbox";
    stored = state({ enabled: true });
    refuseSave = "invalid value for underMicrosandbox";
    render(<ProjectDockerInDocker />);
    await user.click(
      await screen.findByRole("radio", { name: /No isolation: use the host Docker socket/ }),
    );

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Docker daemon in the VM/ })).toBeChecked();
    expect(stored.underMicrosandbox).toBe("vm_daemon");
  });

  it("SET-FR-PMHB, SET-FR-JMHE: under None no selector shows, and the host socket controls show whatever choice is stored", async () => {
    stored = state({ enabled: true, underMicrosandbox: "vm_daemon" });
    render(<ProjectDockerInDocker />);
    expect(await screen.findByLabelText("Docker socket path")).toBeInTheDocument();
    await waitFor(() => expect(calls("load_isolation_settings")).toHaveLength(1));
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByText(/Isolation is Microsandbox/)).not.toBeInTheDocument();
  });

  it("SET-FR-JMHE, SET-FR-PMHB: no host socket control shows before the isolation mode lands, and the daemon choice hides them once it does", async () => {
    isolationMode = "microsandbox";
    stored = state({ enabled: true });
    let land: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      land = resolve;
    });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_isolation_settings") await held;
      return base(cmd, args);
    });
    render(<ProjectDockerInDocker />);
    await waitFor(() => expect(calls("load_docker_in_docker_settings")).toHaveLength(1));
    expect(screen.getByText(/Reading the Docker-in-Docker settings/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Docker socket path")).not.toBeInTheDocument();

    land();
    expect(await screen.findByRole("radio", { name: /Docker daemon in the VM/ })).toBeChecked();
    expect(screen.queryByLabelText("Docker socket path")).not.toBeInTheDocument();
  });

  it("SET-FR-PMHB: a backend that reports no choice reads as the daemon choice", async () => {
    isolationMode = "microsandbox";
    const { underMicrosandbox: _dropped, ...older } = state({ enabled: true });
    stored = older as DockerInDockerState;
    render(<ProjectDockerInDocker />);
    expect(await screen.findByRole("radio", { name: /Docker daemon in the VM/ })).toBeChecked();
  });

  it("SET-FR-PMHB, SET-FR-JMHE: an isolation mode that cannot be read shows no selector and the host socket controls", async () => {
    stored = state({ enabled: true });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_isolation_settings") throw new Error("store unreadable");
      return base(cmd, args);
    });
    render(<ProjectDockerInDocker />);
    expect(await screen.findByLabelText("Docker socket path")).toBeInTheDocument();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
  });

  it("SET-FR-PMHB, SET-FR-PQLA: on Windows under Microsandbox no selector shows", async () => {
    isolationMode = "microsandbox";
    stored = state({ enabled: true, support: "unsupported" });
    render(<ProjectDockerInDocker />);
    expect(await checkbox()).toBeDisabled();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
  });

  it("PSS-FR-VRHC, SET-FR-PMHB: a refusal of an unknown choice is stated in words", () => {
    expect(
      dockerInDockerRefusalMessage(new Error("invalid args `settings`: unknown variant `privileged`")),
    ).toBe("That choice for Microsandbox is not known. Nothing was saved.");
  });

  it("SET-FR-NXDV: the checkbox is off for a project that stored nothing, and the path field and toggle are not shown", async () => {
    render(<ProjectDockerInDocker />);
    expect(await checkbox()).not.toBeChecked();
    expect(screen.queryByLabelText("Docker socket path")).not.toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Host gateway" })).not.toBeInTheDocument();
    // The statement of what the setting does, and that it stays on this machine.
    expect(screen.getByText(/not committed with the project/)).toBeInTheDocument();
  });

  it("SET-FR-NXDV, SET-FR-JMHE: selecting the checkbox saves at once and shows the path field with the platform default as its placeholder and the toggle off", async () => {
    const user = userEvent.setup();
    render(<ProjectDockerInDocker />);
    await user.click(await checkbox());

    const field = await screen.findByLabelText("Docker socket path");
    expect(field).toHaveAttribute("placeholder", "/var/run/docker.sock");
    expect(field).toHaveValue("");
    expect(screen.getByRole("switch", { name: "Host gateway" })).not.toBeChecked();
    expect(calls("save_docker_in_docker_settings")).toHaveLength(1);
    expect(calls("save_docker_in_docker_settings")[0][1]).toEqual({
      settings: { enabled: true, socketPath: "", hostGateway: false, underMicrosandbox: "vm_daemon" },
    });
  });

  it("SET-FR-JMHE: the path commits on Enter and on blur, and an unchanged path saves nothing", async () => {
    const user = userEvent.setup();
    stored = state({ enabled: true });
    render(<ProjectDockerInDocker />);
    const field = await screen.findByLabelText("Docker socket path");

    await user.click(field);
    await user.tab();
    expect(calls("save_docker_in_docker_settings")).toHaveLength(0);

    await user.type(field, "/run/user/1000/docker.sock{Enter}");
    await waitFor(() => expect(stored.socketPath).toBe("/run/user/1000/docker.sock"));
    expect(calls("save_docker_in_docker_settings")).toHaveLength(1);

    await user.clear(field);
    await user.type(field, "/other/docker.sock");
    await user.tab();
    await waitFor(() => expect(stored.socketPath).toBe("/other/docker.sock"));
  });

  it("SET-FR-JMHE: the Host gateway toggle saves at once and carries the other three values through", async () => {
    const user = userEvent.setup();
    stored = state({ enabled: true, socketPath: "/a/d.sock" });
    render(<ProjectDockerInDocker />);
    await user.click(await screen.findByRole("switch", { name: "Host gateway" }));

    await waitFor(() => expect(stored.hostGateway).toBe(true));
    expect(calls("save_docker_in_docker_settings")[0][1]).toEqual({
      settings: {
        enabled: true,
        socketPath: "/a/d.sock",
        hostGateway: true,
        underMicrosandbox: "vm_daemon",
      },
    });
    expect(screen.getByText(/TESTCONTAINERS_HOST_OVERRIDE/)).toBeInTheDocument();
  });

  it("SET-FR-NXDV: a refused write keeps the previous value in the control and renders the refusal in words", async () => {
    const user = userEvent.setup();
    stored = state({ enabled: true, socketPath: "/a/d.sock" });
    refuseSave = "docker_socket_path_invalid";
    render(<ProjectDockerInDocker />);
    const field = await screen.findByLabelText("Docker socket path");
    await user.clear(field);
    await user.type(field, "relative.sock");
    await user.tab();

    expect(await screen.findByRole("alert")).toHaveTextContent(/full path/);
    expect(screen.getByLabelText("Docker socket path")).toHaveValue("/a/d.sock");
    expect(stored.socketPath).toBe("/a/d.sock");
  });

  it("SET-FR-NXDV, SET-FR-JMHE: two changes made before the first write returns are two writes of one record, and the later carries the earlier", async () => {
    const user = userEvent.setup();
    stored = state({ enabled: true, socketPath: "/a/d.sock" });
    // The first write is held until the test lets it go, so the second change
    // is made while it is out. No timer decides the order.
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    let writes = 0;
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "save_docker_in_docker_settings" && writes++ === 0) await held;
      return base(cmd, args);
    });
    render(<ProjectDockerInDocker />);
    const field = await screen.findByLabelText("Docker socket path");
    await user.clear(field);
    await user.type(field, "/new/d.sock");
    await user.tab();
    await user.click(screen.getByRole("switch", { name: "Host gateway" }));
    release();

    await waitFor(() => expect(calls("save_docker_in_docker_settings")).toHaveLength(2));
    await waitFor(() =>
      expect(stored).toMatchObject({ socketPath: "/new/d.sock", hostGateway: true }),
    );
    expect(calls("save_docker_in_docker_settings")[1][1]).toEqual({
      settings: {
        enabled: true,
        socketPath: "/new/d.sock",
        hostGateway: true,
        underMicrosandbox: "vm_daemon",
      },
    });
    expect(screen.getByLabelText("Docker socket path")).toHaveValue("/new/d.sock");
  });

  it("SET-FR-JMHE: a path typed with spaces around it is saved and shown trimmed", async () => {
    const user = userEvent.setup();
    stored = state({ enabled: true });
    render(<ProjectDockerInDocker />);
    const field = await screen.findByLabelText("Docker socket path");
    await user.type(field, "  /x/d.sock  ");
    await user.tab();

    await waitFor(() => expect(stored.socketPath).toBe("/x/d.sock"));
    expect(screen.getByLabelText("Docker socket path")).toHaveValue("/x/d.sock");
    await user.click(field);
    await user.tab();
    expect(calls("save_docker_in_docker_settings")).toHaveLength(1);
  });

  it("SET-FR-PQLA: on Windows the checkbox is disabled and states the reason", async () => {
    stored = state({ support: "unsupported", defaultSocketPath: "" });
    render(<ProjectDockerInDocker />);
    const box = await checkbox();
    expect(box).toBeDisabled();
    expect(screen.getByText(/named pipe cannot be mounted/)).toBeInTheDocument();
  });

  it("SET-FR-PQLA: on macOS the checkbox works and the section says access is best effort", async () => {
    const user = userEvent.setup();
    stored = state({ support: "best_effort" });
    render(<ProjectDockerInDocker />);
    const box = await checkbox();
    expect(box).toBeEnabled();
    expect(screen.getByText(/access is best effort/)).toBeInTheDocument();
    expect(screen.getByText(/group that the\s+socket has in a container/)).toBeInTheDocument();
    expect(
      screen.getByText(/an image with no stat may give the agent no access to\s+the daemon/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/a run stops/)).not.toBeInTheDocument();
    await user.click(box);
    await waitFor(() => expect(stored.enabled).toBe(true));
  });

  it("SET-FR-PQLA: on Linux the section states no limit", async () => {
    render(<ProjectDockerInDocker />);
    await checkbox();
    expect(screen.queryByText(/best effort/)).not.toBeInTheDocument();
    expect(screen.queryByText(/named pipe/)).not.toBeInTheDocument();
  });

  it("SET-FR-HVDC: under None with the checkbox on, the section says the image must hold the Docker CLI and shows its Dockerfile line", async () => {
    stored = state({ enabled: true });
    render(<ProjectDockerInDocker />);
    await checkbox();
    expect(screen.getByText(/must hold the Docker CLI \(docker\) and stat/)).toBeInTheDocument();
    expect(
      screen.getByText("COPY --from=docker:29-cli /usr/local/bin/docker /usr/local/bin/docker"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/must hold dockerd/)).not.toBeInTheDocument();
  });

  it("SET-FR-HVDC: under Microsandbox with the daemon in the VM, the section says the image must hold dockerd, iptables and the Docker CLI", async () => {
    isolationMode = "microsandbox";
    stored = state({ enabled: true, underMicrosandbox: "vm_daemon" });
    render(<ProjectDockerInDocker />);
    expect(await screen.findByText(/must hold dockerd, iptables and the Docker CLI/)).toBeInTheDocument();
    expect(
      screen.getByText(/apt-get install -y --no-install-recommends docker.io docker-cli iptables/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/docker:29-cli/)).not.toBeInTheDocument();
  });

  it("SET-FR-HVDC: under Microsandbox with the host socket, the section shows the Docker CLI line, and changing the choice changes the line", async () => {
    const user = userEvent.setup();
    isolationMode = "microsandbox";
    stored = state({ enabled: true, underMicrosandbox: "host_socket" });
    render(<ProjectDockerInDocker />);
    // The isolation mode lands first, so the line below is the one of the
    // stored choice and not of the state before the mode is known.
    expect(
      await screen.findByRole("radio", { name: /No isolation: use the host Docker socket/ }),
    ).toBeChecked();
    expect(screen.getByText(/docker:29-cli/)).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /Docker daemon in the VM/ }));
    expect(await screen.findByText(/must hold dockerd, iptables/)).toBeInTheDocument();
    expect(screen.queryByText(/docker:29-cli/)).not.toBeInTheDocument();
    await waitFor(() => expect(stored.underMicrosandbox).toBe("vm_daemon"));
  });

  it("SET-FR-HVDC: selecting the checkbox shows the image requirement, and clearing it hides it", async () => {
    const user = userEvent.setup();
    render(<ProjectDockerInDocker />);
    const box = await checkbox();
    expect(screen.queryByText(/project's image must hold/)).not.toBeInTheDocument();
    await user.click(box);
    expect(await screen.findByText(/must hold the Docker CLI/)).toBeInTheDocument();
    await user.click(box);
    await waitFor(() => expect(screen.queryByText(/project's image must hold/)).not.toBeInTheDocument());
    await waitFor(() => expect(stored.enabled).toBe(false));
  });

  it("SET-FR-HVDC, EAC-FR-MRTJ, EAC-FR-UDQN: each example line is the line the acceptance test proves in its Dockerfile", () => {
    const dockerfile = (name: string) =>
      readFileSync(resolve(__dirname, "../../tests/dind-uat", `Dockerfile.${name}`), "utf8").split("\n");
    for (const name of ["none-host-socket", "microsandbox-host-socket"]) {
      expect(dockerfile(name)).toContain(IMAGE_REQUIREMENTS.host_socket[1]);
    }
    const daemonLine = IMAGE_REQUIREMENTS.vm_daemon[1];
    expect(dockerfile("microsandbox-vm-daemon").some((line) => line.startsWith(daemonLine))).toBe(true);
  });

  it("SET-FR-HVDC: with the checkbox off, and on Windows, the section states no image requirement", async () => {
    render(<ProjectDockerInDocker />);
    await checkbox();
    expect(screen.queryByText(/project's image must hold/)).not.toBeInTheDocument();
    cleanup();
    stored = state({ enabled: true, support: "unsupported", defaultSocketPath: "" });
    render(<ProjectDockerInDocker />);
    await checkbox();
    expect(screen.queryByText(/project's image must hold/)).not.toBeInTheDocument();
  });

  it("SET-FR-NXDV: settings that cannot be read are stated and nothing is shown to change", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return undefined;
      throw new Error("no_project_open");
    });
    render(<ProjectDockerInDocker />);
    expect(await screen.findByRole("alert")).toHaveTextContent(/could not be read/);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });
});

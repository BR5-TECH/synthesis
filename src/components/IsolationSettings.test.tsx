import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import type { IsolationHost, IsolationSettings as Settings, IsolationState } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { allowedHostProblem, IsolationSettings, unsupportedReasonMessage } from "./IsolationSettings";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const lastSaved = (): Settings => {
  const saves = calls("save_isolation_settings");
  return (saves[saves.length - 1][1] as { settings: Settings }).settings;
};

const supportedHost: IsolationHost = {
  microsandboxSupported: true,
  unsupportedReason: null,
  logicalCpus: 12,
  totalMemoryGib: 32,
};

/** What the fake backend holds; a save writes it, a refusal leaves it. */
let stored: Settings;
let host: IsolationHost;
let refuseSave: string | null;

function stateOf(settings: Settings): IsolationState {
  return {
    settings,
    host,
    effectiveCpus: Math.min(settings.cpus ?? 4, host.logicalCpus),
    effectiveMemoryGib: Math.min(settings.memoryGib ?? 8, host.totalMemoryGib),
  };
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  refuseSave = null;
  host = supportedHost;
  stored = {
    mode: "none",
    cpus: null,
    memoryGib: null,
    blockLocalNetwork: true,
    allowedLocalHosts: [],
  };
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_isolation_settings") return stateOf(stored);
    if (cmd === "save_isolation_settings") {
      if (refuseSave) throw new Error(refuseSave);
      stored = (args as { settings: Settings }).settings;
      return stateOf(stored);
    }
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

const microsandbox = () => screen.findByRole("radio", { name: "Microsandbox" });

/** From now on the backend answers no save until the test releases it. */
function holdSaves(): (() => void)[] {
  const pending: (() => void)[] = [];
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "save_isolation_settings") {
      await new Promise<void>((release) => pending.push(release));
      stored = (args as { settings: Settings }).settings;
      return stateOf(stored);
    }
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
  return pending;
}

describe("Global settings → Isolation (GLS-FR-YAEA through GLS-FR-GHYV)", () => {
  it("GLS-FR-YAEA: the mode offers None and Microsandbox, read on mount, None by default, and states its scope", async () => {
    render(<IsolationSettings />);
    expect(await microsandbox()).not.toBeChecked();
    expect(screen.getByRole("radio", { name: "None" })).toBeChecked();
    expect(screen.getByText(/stream, direct, and merge runs, and to no other agent/)).toBeInTheDocument();
    expect(calls("load_isolation_settings")).toHaveLength(1);
    // No Microsandbox controls while None is selected.
    expect(screen.queryByLabelText(/CPU cores/)).not.toBeInTheDocument();
  });

  it("GLS-FR-GHYV: choosing Microsandbox applies at once through the save, with no dirty state", async () => {
    render(<IsolationSettings />);
    await userEvent.click(await microsandbox());
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(1));
    expect(lastSaved().mode).toBe("microsandbox");
    expect(await screen.findByLabelText(/CPU cores/)).toBeInTheDocument();
  });

  it("GLS-FR-USNH: the sliders are bounded by the machine, default to min(4) and min(8 GB), and render a stored value above a maximum at it", async () => {
    stored = { ...stored, mode: "microsandbox" };
    render(<IsolationSettings />);
    const cpus = await screen.findByLabelText(/CPU cores/);
    const ram = screen.getByLabelText(/RAM/);
    expect(cpus).toHaveAttribute("min", "1");
    expect(cpus).toHaveAttribute("max", "12");
    expect(cpus).toHaveValue("4");
    expect(ram).toHaveAttribute("min", "1");
    expect(ram).toHaveAttribute("max", "32");
    expect(ram).toHaveValue("8");
    cleanup();

    host = { ...supportedHost, logicalCpus: 2, totalMemoryGib: 4 };
    stored = { ...stored, cpus: 16, memoryGib: 64 };
    render(<IsolationSettings />);
    expect(await screen.findByLabelText(/CPU cores/)).toHaveValue("2");
    expect(screen.getByLabelText(/RAM/)).toHaveValue("4");
  });

  it("GLS-FR-USNH, GLS-FR-GHYV: a slider writes once, when it is let go", async () => {
    stored = { ...stored, mode: "microsandbox" };
    render(<IsolationSettings />);
    const cpus = await screen.findByLabelText(/CPU cores/);
    fireEvent.change(cpus, { target: { value: "6" } });
    fireEvent.change(cpus, { target: { value: "7" } });
    expect(calls("save_isolation_settings")).toHaveLength(0);
    fireEvent.pointerUp(cpus);
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(1));
    expect(lastSaved().cpus).toBe(7);
    const ram = screen.getByLabelText(/RAM/);
    fireEvent.change(ram, { target: { value: "16" } });
    fireEvent.blur(ram);
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(2));
    expect(lastSaved().memoryGib).toBe(16);
  });

  it("GLS-FR-GHYV: two changes made before the first save returns are both kept", async () => {
    stored = { ...stored, mode: "microsandbox", allowedLocalHosts: ["10.0.0.5"] };
    render(<IsolationSettings />);
    const cpus = await screen.findByLabelText(/CPU cores/);
    const pending = holdSaves();

    fireEvent.change(cpus, { target: { value: "6" } });
    fireEvent.pointerUp(cpus);
    await waitFor(() => expect(pending).toHaveLength(1));
    const ram = screen.getByLabelText(/RAM/);
    fireEvent.change(ram, { target: { value: "16" } });
    fireEvent.blur(ram);
    const field = screen.getByRole("textbox", { name: "Allowed local host" });
    fireEvent.change(field, { target: { value: "gateway.lan" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    pending[0]();
    await waitFor(() => expect(pending).toHaveLength(2));
    pending[1]();
    await waitFor(() => expect(pending).toHaveLength(3));
    pending[2]();
    await waitFor(() => expect(screen.getByText("gateway.lan")).toBeInTheDocument());

    expect(calls("save_isolation_settings")).toHaveLength(3);
    expect(lastSaved()).toMatchObject({
      cpus: 6,
      memoryGib: 16,
      allowedLocalHosts: ["10.0.0.5", "gateway.lan"],
    });
  });

  it("GLS-FR-GHYV, GLS-FR-QFWZ: an entry added again before its first write returns is listed once", async () => {
    stored = { ...stored, mode: "microsandbox" };
    render(<IsolationSettings />);
    const field = await screen.findByRole("textbox", { name: "Allowed local host" });
    const pending = holdSaves();
    for (let i = 0; i < 2; i += 1) {
      fireEvent.change(field, { target: { value: "gateway.lan" } });
      fireEvent.click(screen.getByRole("button", { name: "Add" }));
    }
    await waitFor(() => expect(pending).toHaveLength(1));
    pending[0]();
    await waitFor(() => expect(pending).toHaveLength(2));
    pending[1]();
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(2));
    await waitFor(() => expect(screen.getAllByText("gateway.lan")).toHaveLength(1));
    expect(lastSaved().allowedLocalHosts).toEqual(["gateway.lan"]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("GLS-FR-GHYV: a slider moved back to its first value while a write is in progress writes that value", async () => {
    stored = { ...stored, mode: "microsandbox", cpus: 2 };
    render(<IsolationSettings />);
    const cpus = await screen.findByLabelText(/CPU cores/);
    const pending = holdSaves();
    fireEvent.change(cpus, { target: { value: "4" } });
    fireEvent.pointerUp(cpus);
    await waitFor(() => expect(pending).toHaveLength(1));
    // A drag back to 2 passes through 3 on its way.
    fireEvent.change(cpus, { target: { value: "3" } });
    fireEvent.change(cpus, { target: { value: "2" } });
    fireEvent.pointerUp(cpus);
    pending[0]();
    await waitFor(() => expect(pending).toHaveLength(2));
    pending[1]();
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(2));
    await waitFor(() => expect(screen.getByLabelText(/CPU cores/)).toHaveValue("2"));
    expect(lastSaved().cpus).toBe(2);
  });

  it("GLS-FR-LKXZ: the section states outbound access is allowed and the block switch defaults on", async () => {
    stored = { ...stored, mode: "microsandbox" };
    render(<IsolationSettings />);
    const block = await screen.findByRole("switch", { name: "Block local network access" });
    expect(block).toBeChecked();
    expect(screen.getByText(/Outbound internet access is always allowed/)).toBeInTheDocument();
    expect(screen.getByText(/loopback, private ranges, link-local/)).toBeInTheDocument();
    await userEvent.click(block);
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(1));
    expect(lastSaved().blockLocalNetwork).toBe(false);
  });

  it("GLS-FR-QFWZ: a valid entry is added and removed; an invalid or repeated one is refused inline", async () => {
    stored = { ...stored, mode: "microsandbox", allowedLocalHosts: ["10.0.0.5"] };
    render(<IsolationSettings />);
    const field = await screen.findByRole("textbox", { name: "Allowed local host" });

    await userEvent.type(field, "not a host!");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/not an IP address/);
    expect(calls("save_isolation_settings")).toHaveLength(0);

    await userEvent.clear(field);
    await userEvent.type(field, "10.0.0.5");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/already holds/);
    expect(calls("save_isolation_settings")).toHaveLength(0);

    await userEvent.clear(field);
    await userEvent.type(field, "192.168.1.0/24{Enter}");
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(1));
    expect(lastSaved().allowedLocalHosts).toEqual(["10.0.0.5", "192.168.1.0/24"]);

    await userEvent.click(await screen.findByRole("button", { name: "Remove 10.0.0.5" }));
    await waitFor(() => expect(calls("save_isolation_settings")).toHaveLength(2));
    expect(lastSaved().allowedLocalHosts).toEqual(["192.168.1.0/24"]);
  });

  it("GLS-FR-QFWZ: while the switch is off the list is disabled and keeps its entries", async () => {
    stored = { ...stored, mode: "microsandbox", blockLocalNetwork: false, allowedLocalHosts: ["gateway.lan"] };
    render(<IsolationSettings />);
    expect(await screen.findByRole("textbox", { name: "Allowed local host" })).toBeDisabled();
    expect(screen.getByText("gateway.lan")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove gateway.lan" })).toBeDisabled();
  });

  it("GLS-FR-QFWZ: the inline check accepts addresses, ranges, and host names", () => {
    for (const good of ["10.0.0.5", "fd00::1", "::1", "10.0.0.0/8", "fd00::/8", "localhost", "gateway.lan"]) {
      expect(allowedHostProblem(good, [])).toBeNull();
    }
    for (const bad of ["", "10.0.0.0/33", "fd00::/129", "-bad", "a b", "http://x", "300.1.1.1/24"]) {
      expect(allowedHostProblem(bad, [])).not.toBeNull();
    }
    expect(allowedHostProblem("Gateway.LAN", ["gateway.lan"])).toMatch(/already holds/);
  });

  it("GLS-FR-CWJX: the section warns that a local Custom Gateway is blocked until its host is listed", async () => {
    stored = { ...stored, mode: "microsandbox" };
    render(<IsolationSettings />);
    const warning = await screen.findByTestId("isolation-gateway-warning");
    expect(warning).toHaveTextContent(/Custom Gateway whose address is localhost or a local network/);
    expect(warning).toHaveTextContent(/does not add the gateway host for you/);
  });

  it("GLS-FR-WAGX: the section states that Docker-in-Docker is ignored under Microsandbox", async () => {
    render(<IsolationSettings />);
    await microsandbox();
    expect(screen.getByText(/Docker-in-Docker setting of a project is ignored/)).toBeInTheDocument();
    expect(screen.getByText(/does not check the socket/)).toBeInTheDocument();
  });

  it("GLS-FR-EZRL: on a machine that cannot run it, Microsandbox is disabled, None stays selected, and the reason is stated", async () => {
    host = { ...supportedHost, microsandboxSupported: false, unsupportedReason: "kvm_missing" };
    render(<IsolationSettings />);
    expect(await microsandbox()).toBeDisabled();
    expect(screen.getByRole("radio", { name: "None" })).toBeChecked();
    expect(screen.getByTestId("isolation-unsupported")).toHaveTextContent(/KVM \(\/dev\/kvm\) is missing/);
    expect(unsupportedReasonMessage("unsupported_platform")).toMatch(/Linux and macOS only/);
    expect(unsupportedReasonMessage("apple_silicon_required")).toMatch(/Apple Silicon/);
    expect(unsupportedReasonMessage("kvm_inaccessible")).toMatch(/cannot open KVM/);
  });

  it("GLS-FR-EZRL: a stored Microsandbox on such a machine renders selected with the reason, and None stays selectable", async () => {
    host = { ...supportedHost, microsandboxSupported: false, unsupportedReason: "apple_silicon_required" };
    stored = { ...stored, mode: "microsandbox" };
    render(<IsolationSettings />);
    expect(await microsandbox()).toBeChecked();
    expect(screen.getByTestId("isolation-unsupported")).toHaveTextContent(/Apple Silicon/);
    const none = screen.getByRole("radio", { name: "None" });
    expect(none).toBeEnabled();
    await userEvent.click(none);
    await waitFor(() => expect(lastSaved().mode).toBe("none"));
  });

  it("GLS-FR-GHYV: a refused save keeps the previous value and renders the refusal inline", async () => {
    refuseSave = "cpus_out_of_range";
    render(<IsolationSettings />);
    await userEvent.click(await microsandbox());
    expect(await screen.findByRole("alert")).toHaveTextContent(/CPU count must be between 1/);
    expect(screen.getByRole("radio", { name: "None" })).toBeChecked();
  });
});

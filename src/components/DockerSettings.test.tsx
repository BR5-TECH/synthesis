import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { DockerSettings, dockerVerifyMessage } from "./DockerSettings";
import type { DockerBackend } from "../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const unverified: DockerBackend = {
  mode: "bollard",
  endpoint: "automatic",
  cliPath: null,
  state: "unverified",
  serverVersion: null,
  verifiedAt: null,
};

/**
 * The backend as this suite scripts it: a stored record, a detection answer,
 * and whatever `verify_docker_backend` should do.
 */
function backend(options: {
  stored?: DockerBackend;
  detected?: string | null;
  verify?: (config: unknown) => DockerBackend | never;
  onSave?: (config: unknown) => DockerBackend;
  /** GLS-FR-29: what the native file picker answers with. */
  browse?: "cancelled" | { selected: { path: string } };
}) {
  let stored = options.stored ?? unverified;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    switch (cmd) {
      case "load_docker_backend":
        return stored;
      case "detect_docker_cli_binary":
        return { path: options.detected ?? null };
      case "browse_for_file":
        return options.browse ?? "cancelled";
      case "save_docker_backend": {
        const config = (args as { config: DockerBackend }).config;
        stored = options.onSave
          ? options.onSave(config)
          : {
              ...config,
              // GSS-FR-39: a new selection has not been verified.
              state: "unverified",
              serverVersion: null,
              verifiedAt: null,
            };
        return stored;
      }
      case "verify_docker_backend": {
        const config = (args as { config: DockerBackend }).config;
        if (!options.verify) throw new Error("daemon_unreachable");
        stored = options.verify(config);
        return stored;
      }
      default:
        return undefined;
    }
  });
}

const mode = (label: string) => screen.getByRole("radio", { name: label });
const statusLine = () => screen.getByRole("status");

beforeEach(() => invokeMock.mockReset());
afterEach(cleanup);

describe("Global settings → Docker (GLS-FR-29..GLS-FR-31)", () => {
  it("GLS-FR-03, GLS-FR-29: a machine that has configured nothing reads Bollard, automatic, unverified", async () => {
    backend({});
    render(<DockerSettings />);

    await waitFor(() =>
      expect(mode("Bollard / Docker Engine")).toBeChecked(),
    );
    expect(calls("load_docker_backend").length).toBeGreaterThan(0);
    expect(mode("Docker CLI")).not.toBeChecked();
    // The endpoint control shows the platform's automatic default…
    expect(screen.getByLabelText("Docker endpoint")).toHaveValue("automatic");
    // …and the status line says nothing has verified.
    expect(statusLine()).toHaveTextContent("Not verified yet.");
    // Merely looking at the section wrote nothing and verified nothing.
    expect(calls("save_docker_backend")).toHaveLength(0);
    expect(calls("verify_docker_backend")).toHaveLength(0);
  });

  it("GLS-FR-29, GLS-FR-30, GLS-FR-31: an unreachable daemon, then one that answers, then a mode change", async () => {
    const user = userEvent.setup();
    let daemonUp = false;
    backend({
      detected: "/usr/local/bin/docker",
      verify: (config) => {
        if (!daemonUp) throw new Error("daemon_unreachable");
        return {
          ...(config as DockerBackend),
          state: "verified",
          serverVersion: "27.1.1",
          verifiedAt: "2026-01-01T00:00:00Z",
        };
      },
    });
    render(<DockerSettings />);
    await waitFor(() => expect(mode("Bollard / Docker Engine")).toBeChecked());

    // A TCP endpoint no daemon answers at.
    await user.selectOptions(
      screen.getByLabelText("Docker endpoint"),
      "tcp",
    );
    const endpoint = screen.getByRole("textbox", { name: "Docker endpoint" });
    await user.type(endpoint, "tcp://127.0.0.1:2375");
    await user.tab();

    await user.click(screen.getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(statusLine()).toHaveTextContent(/Docker daemon did not answer/),
    );
    expect(calls("verify_docker_backend")).toHaveLength(1);

    // The same endpoint, with a daemon behind it.
    daemonUp = true;
    await user.click(screen.getByRole("button", { name: "Verify" }));
    await waitFor(() =>
      expect(statusLine()).toHaveTextContent("Verified — Docker 27.1.1."),
    );

    // GLS-FR-31: selecting Docker CLI mode renders the path field, detection
    // fills it, and the status line returns to unverified with no verification
    // invoked for it.
    const verifiesBefore = calls("verify_docker_backend").length;
    await user.click(mode("Docker CLI"));
    await waitFor(() =>
      expect(screen.getByLabelText("Docker CLI path")).toHaveValue(
        "/usr/local/bin/docker",
      ),
    );
    expect(statusLine()).toHaveTextContent("Not verified yet.");
    expect(calls("verify_docker_backend")).toHaveLength(verifiesBefore);
    expect(calls("save_docker_backend").length).toBeGreaterThan(0);
  });

  it("GLS-FR-31: a verified backend stays verified across a remount, with no probe", async () => {
    const verified: DockerBackend = {
      mode: "bollard",
      endpoint: "automatic",
      cliPath: null,
      state: "verified",
      serverVersion: "27.1.1",
      verifiedAt: "2026-01-01T00:00:00Z",
    };
    backend({ stored: verified });

    const first = render(<DockerSettings />);
    await waitFor(() =>
      expect(statusLine()).toHaveTextContent("Verified — Docker 27.1.1."),
    );
    first.unmount();

    // Reopened after the daemon was stopped: the record describes what was
    // configured and proved, not what the daemon is doing now (GSS-FR-39).
    render(<DockerSettings />);
    await waitFor(() =>
      expect(statusLine()).toHaveTextContent("Verified — Docker 27.1.1."),
    );
    expect(calls("verify_docker_backend")).toHaveLength(0);
    // And nothing was saved on account of merely opening the section, so the
    // section contributes nothing to the window's save sweep (GLS-FR-31).
    expect(calls("save_docker_backend")).toHaveLength(0);
  });

  it("GLS-FR-30: the selected mode's failures are told apart from one another", () => {
    // Docker CLI mode: a missing or invalid executable, and a daemon that
    // cannot answer, are different lines.
    expect(dockerVerifyMessage(new Error("cli_not_found"))).toMatch(
      /nothing at that path/,
    );
    expect(dockerVerifyMessage(new Error("cli_not_executable"))).toMatch(
      /cannot run it/,
    );
    expect(dockerVerifyMessage(new Error("not_the_docker_cli"))).toMatch(
      /not the Docker CLI/,
    );
    // Docker Engine mode: an invalid endpoint, and a daemon that cannot answer.
    expect(dockerVerifyMessage(new Error("endpoint_invalid"))).toMatch(
      /socket path, a named pipe, or a URL/,
    );
    const unreachable = dockerVerifyMessage(new Error("daemon_unreachable"));
    expect(unreachable).toMatch(/did not answer/);
    // The two are never folded together: each asks for a different correction.
    expect(unreachable).not.toEqual(
      dockerVerifyMessage(new Error("endpoint_invalid")),
    );
    expect(unreachable).not.toEqual(
      dockerVerifyMessage(new Error("cli_not_found")),
    );
  });

  it("GLS-FR-29: the CLI path can be replaced through a file picker", async () => {
    const user = userEvent.setup();
    backend({
      stored: {
        mode: "docker_cli",
        endpoint: "automatic",
        cliPath: "/usr/local/bin/docker",
        state: "verified",
        serverVersion: "27.1.1",
        verifiedAt: "2026-01-01T00:00:00Z",
      },
      browse: { selected: { path: "/opt/homebrew/bin/docker" } },
    });
    render(<DockerSettings />);
    await waitFor(() =>
      expect(screen.getByLabelText("Docker CLI path")).toHaveValue(
        "/usr/local/bin/docker",
      ),
    );

    await user.click(screen.getByRole("button", { name: "Browse…" }));
    await waitFor(() =>
      expect(screen.getByLabelText("Docker CLI path")).toHaveValue(
        "/opt/homebrew/bin/docker",
      ),
    );
    // GLS-FR-31: the new path is stored at once, and the success the old one
    // earned no longer describes it.
    expect(calls("save_docker_backend")).toHaveLength(1);
    expect(
      (calls("save_docker_backend")[0][1] as { config: DockerBackend }).config
        .cliPath,
    ).toBe("/opt/homebrew/bin/docker");
    expect(statusLine()).toHaveTextContent("Not verified yet.");
    expect(calls("verify_docker_backend")).toHaveLength(0);
  });

  it("GLS-FR-29: a cancelled picker changes nothing", async () => {
    const user = userEvent.setup();
    backend({
      stored: {
        mode: "docker_cli",
        endpoint: "automatic",
        cliPath: "/usr/local/bin/docker",
        state: "verified",
        serverVersion: "27.1.1",
        verifiedAt: "2026-01-01T00:00:00Z",
      },
      browse: "cancelled",
    });
    render(<DockerSettings />);
    await waitFor(() =>
      expect(statusLine()).toHaveTextContent("Verified — Docker 27.1.1."),
    );

    await user.click(screen.getByRole("button", { name: "Browse…" }));
    await waitFor(() => expect(calls("browse_for_file")).toHaveLength(1));
    expect(screen.getByLabelText("Docker CLI path")).toHaveValue(
      "/usr/local/bin/docker",
    );
    expect(calls("save_docker_backend")).toHaveLength(0);
    expect(statusLine()).toHaveTextContent("Verified — Docker 27.1.1.");
  });
});

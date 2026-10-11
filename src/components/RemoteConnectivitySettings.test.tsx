import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  RemoteConnectivitySettings,
  relayStatusLine,
  relayVerifyMessage,
} from "./RemoteConnectivitySettings";
import type { RelayEndpoint } from "../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const unset: RelayEndpoint = {
  url: null,
  state: "unset",
  version: null,
  capabilities: [],
  verifiedAt: null,
};

const verified: RelayEndpoint = {
  url: "https://relay.example.com",
  state: "verified",
  version: "v1.2.3",
  capabilities: ["remote_session", "websocket"],
  verifiedAt: "2026-01-01T00:00:00Z",
};

/** The backend as this suite scripts it. */
function backend(options: {
  stored?: RelayEndpoint;
  verify?: (url: string) => RelayEndpoint | never;
}) {
  let stored = options.stored ?? unset;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    switch (cmd) {
      case "load_relay_endpoint":
        return stored;
      case "save_relay_endpoint": {
        const url = (args as { url: string }).url;
        // GSS-FR-NLDC: a URL that is not the bound one reads unverified.
        stored =
          url.trim() === ""
            ? unset
            : {
                url,
                state:
                  stored.state === "verified" && stored.url === url
                    ? "verified"
                    : "unverified",
                version: stored.url === url ? stored.version : null,
                capabilities: stored.url === url ? stored.capabilities : [],
                verifiedAt: stored.url === url ? stored.verifiedAt : null,
              };
        return stored;
      }
      case "verify_relay_endpoint": {
        const url = (args as { url: string }).url;
        if (!options.verify) throw new Error("unreachable");
        stored = options.verify(url);
        return stored;
      }
      case "log records appended":
        return null;
      default:
        return null;
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(() => {
  cleanup();
});

describe("Remote connectivity", () => {
  // GLS-FR-KVNP: one relay endpoint, initialised from "load relay endpoint" on
  // mount, and empty until the author gives one.
  it("GLS-FR-KVNP: presents one relay address field, read from the store on mount", async () => {
    backend({});
    render(<RemoteConnectivitySettings />);

    const field = await screen.findByLabelText<HTMLInputElement>(
      "Relay address",
    );
    expect(field.value).toBe("");
    expect(calls("load_relay_endpoint")).toHaveLength(1);
    // One endpoint rather than a list: there is exactly one such field.
    expect(screen.getAllByLabelText("Relay address")).toHaveLength(1);
    expect(await screen.findByRole("status")).toHaveTextContent(
      "No relay address yet.",
    );
  });

  // GLS-FR-KVNP / GLS-FR-RBLM: a stored record is what the section renders, and
  // mounting probes nothing.
  it("GLS-FR-RBLM: re-reads a verified endpoint on mount rather than re-probing", async () => {
    backend({ stored: verified });
    render(<RemoteConnectivitySettings />);

    const field = await screen.findByLabelText<HTMLInputElement>(
      "Relay address",
    );
    expect(field.value).toBe("https://relay.example.com");
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Verified — relay v1.2.3.",
    );
    expect(calls("verify_relay_endpoint")).toHaveLength(0);
  });

  // GLS-FR-RBLM: a change applies immediately through "save relay endpoint",
  // and invokes no verification.
  it("GLS-FR-RBLM: a change saves at once and returns the status line to unverified", async () => {
    backend({ stored: verified });
    const user = userEvent.setup();
    render(<RemoteConnectivitySettings />);

    const field = await screen.findByLabelText<HTMLInputElement>(
      "Relay address",
    );
    await user.clear(field);
    await user.type(field, "https://other.example.com");
    await user.tab();

    await waitFor(() => expect(calls("save_relay_endpoint")).toHaveLength(1));
    expect(calls("save_relay_endpoint")[0][1]).toEqual({
      url: "https://other.example.com",
    });
    expect(calls("verify_relay_endpoint")).toHaveLength(0);
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("Not verified yet."),
    );
  });

  // GLS-FR-QWTD: Verify invokes "verify relay endpoint" and the status line
  // names the relay version it answered with.
  it("GLS-FR-QWTD: Verify reports the relay version on a success", async () => {
    backend({ stored: { ...verified, state: "unverified", version: null }, verify: () => verified });
    const user = userEvent.setup();
    render(<RemoteConnectivitySettings />);

    await screen.findByLabelText("Relay address");
    await user.click(screen.getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "Verified — relay v1.2.3.",
      ),
    );
    expect(calls("verify_relay_endpoint")[0][1]).toEqual({
      url: "https://relay.example.com",
    });
  });

  // GLS-FR-QWTD: each typed failure renders as its own line, and a refused
  // attempt never presents the endpoint as verified.
  it("GLS-FR-QWTD: a refused verification renders its own line and verifies nothing", async () => {
    backend({
      stored: { ...verified, state: "unverified", version: null },
      verify: () => {
        throw new Error("capability_missing");
      },
    });
    const user = userEvent.setup();
    render(<RemoteConnectivitySettings />);

    await screen.findByLabelText("Relay address");
    await user.click(screen.getByRole("button", { name: "Verify" }));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "That server is a relay, but it does not offer remote sessions over WebSocket.",
      ),
    );
    expect(screen.getByRole("status")).not.toHaveTextContent("Verified");
  });

  // GLS-FR-RBLM: a URL the author typed and has not left yet is saved before it
  // is verified, so an edit is never lost between the two.
  it("GLS-FR-RBLM: Verify saves the typed address before it verifies it", async () => {
    backend({ stored: verified, verify: (url) => ({ ...verified, url }) });
    const user = userEvent.setup();
    render(<RemoteConnectivitySettings />);

    const field = await screen.findByLabelText<HTMLInputElement>(
      "Relay address",
    );
    await user.clear(field);
    await user.type(field, "https://typed.example.com");
    await user.click(screen.getByRole("button", { name: "Verify" }));

    await waitFor(() => expect(calls("verify_relay_endpoint")).toHaveLength(1));
    // What is verified is what the store was given: no save carries anything
    // else, so the address the author typed is never verified unsaved.
    expect(calls("save_relay_endpoint").length).toBeGreaterThan(0);
    for (const call of calls("save_relay_endpoint")) {
      expect(call[1]).toEqual({ url: "https://typed.example.com" });
    }
    expect(calls("verify_relay_endpoint")[0][1]).toEqual({
      url: "https://typed.example.com",
    });
  });

  // GLS-FR-XDUJ: the section presents no field for a token or a key, and names
  // no secret.
  it("GLS-FR-XDUJ: presents no control for a token or a private key", async () => {
    backend({ stored: verified });
    const { container } = render(<RemoteConnectivitySettings />);
    await screen.findByLabelText("Relay address");

    expect(container.querySelectorAll("input")).toHaveLength(1);
    expect(container.querySelector('input[type="password"]')).toBeNull();
    const text = container.textContent ?? "";
    for (const forbidden of [
      "SYNTHESIS_SERVER_TOKEN",
      "Token",
      "token",
      "Private key",
      "Secret",
    ]) {
      expect(text).not.toContain(forbidden);
    }
    // No command of this section reads or writes a credential.
    for (const call of invokeMock.mock.calls) {
      expect(JSON.stringify(call)).not.toContain("token");
    }
  });
});

describe("relayVerifyMessage", () => {
  // GLS-FR-QWTD: every failure of GSS-FR-HPWE is its own sentence, and none is
  // folded into another.
  it("GLS-FR-QWTD: each typed failure is a different sentence", () => {
    const codes = [
      "endpoint_empty",
      "endpoint_invalid",
      "scheme_unsupported",
      "unreachable",
      "timed_out",
      "not_a_relay",
      "capability_missing",
    ];
    const sentences = codes.map((code) =>
      relayVerifyMessage(new Error(code)),
    );
    expect(new Set(sentences).size).toBe(codes.length);
    for (const sentence of sentences) {
      expect(sentence).not.toBe("");
      expect(sentence).not.toContain("_");
    }
    // An error the section does not know is passed through rather than hidden.
    expect(relayVerifyMessage(new Error("something else"))).toBe(
      "something else",
    );
    expect(relayVerifyMessage(null)).toBe("The relay could not be verified.");
  });

  it("AAP-FR-LRTC: a refused certificate names the host and the cause", () => {
    const wire = "tls_untrusted:unknown_issuer:relay.corp.test";
    for (const rejection of [wire, `Error: ${wire}`, new Error(wire)]) {
      const text = relayVerifyMessage(rejection);
      expect(text).toContain("relay.corp.test");
      expect(text).toMatch(/issuer.*unknown/);
    }
  });
});

describe("relayStatusLine", () => {
  // GLS-FR-QWTD: the four things the status line says.
  it("GLS-FR-QWTD: says unset, unverified, verified, or the one failure", () => {
    expect(relayStatusLine(unset, null)).toBe("No relay address yet.");
    expect(
      relayStatusLine({ ...verified, state: "unverified", version: null }, null),
    ).toBe("Not verified yet.");
    expect(relayStatusLine(verified, null)).toBe("Verified — relay v1.2.3.");
    // A failure stands above the record: the section never presents an endpoint
    // as verified while the last attempt refused.
    expect(relayStatusLine(verified, "Nothing answered at that address.")).toBe(
      "Nothing answered at that address.",
    );
  });
});

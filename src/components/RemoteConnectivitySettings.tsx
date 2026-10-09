/**
 * Global settings → **Remote connectivity** (GLS-FR-KVNP through GLS-FR-XDUJ).
 *
 * Where the one relay this machine reaches is configured. The relay is what a
 * remote client meets this IDE at, so an author gives its URL once and proves
 * that it is there and speaks what the application needs before trying to pair
 * a device.
 *
 * The section presents one endpoint rather than a list, because the application
 * reaches one configured relay. Every change applies at once through "save
 * relay endpoint", so the section holds no dirty state and contributes nothing
 * to the save-before-close sweep of GLS-FR-13; a change returns the status line
 * to unverified without invoking any verification (GSS-FR-NLDC).
 *
 * GLS-FR-XDUJ: no `SYNTHESIS_SERVER_TOKEN`, no relay credential, and no private
 * key is displayed, edited, persisted, or validated here. The token reaches the
 * application, the relay, and the mobile client as runtime configuration, so a
 * control for it here would create a second, weaker home for a credential.
 */
import { useCallback, useEffect, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import { tlsErrorMessage } from "../tlsError";
import type { RelayEndpoint } from "../types";

/**
 * GLS-FR-QWTD: the endpoint's own failures, as sentences.
 *
 * An empty URL, a malformed URL, an unsupported scheme, a relay that did not
 * answer, an attempt that exceeded its bound, an answer that is not a relay
 * health response, and a relay that does not advertise the capabilities the
 * application needs are each a different line: each asks the author to correct
 * a different thing (GSS-FR-HPWE).
 */
export function relayVerifyMessage(error: unknown): string {
  const raw = String(error instanceof Error ? error.message : (error ?? ""));
  // AAP-FR-LRTC: a refused certificate names its host and its cause.
  const tls = tlsErrorMessage(raw);
  if (tls) return tls;
  const known: [string, string][] = [
    ["endpoint_empty", "Give the address of the relay to reach."],
    [
      "scheme_unsupported",
      "The address must start with http:// or https://.",
    ],
    ["endpoint_invalid", "That is not an address a relay can be reached at."],
    [
      "capability_missing",
      "That server is a relay, but it does not offer remote sessions over WebSocket.",
    ],
    ["not_a_relay", "Something answered there, but it is not a relay."],
    ["timed_out", "The relay did not answer in time."],
    [
      "unreachable",
      "Nothing answered at that address. Check it, then verify again.",
    ],
  ];
  for (const [code, sentence] of known) {
    if (raw.includes(code)) return sentence;
  }
  return raw || "The relay could not be verified.";
}

/** GLS-FR-QWTD: what the status line says, given the record and the last attempt. */
export function relayStatusLine(
  endpoint: RelayEndpoint,
  failure: string | null,
): string {
  if (failure) return failure;
  if (endpoint.state === "verified") {
    return `Verified — relay ${endpoint.version ?? "answered"}.`;
  }
  if (endpoint.state === "unset") return "No relay address yet.";
  return "Not verified yet.";
}

interface RemoteConnectivitySettingsProps {
  /**
   * Reported up whenever the stored record changes, so a host that renders the
   * endpoint elsewhere reflects it without a reload. Optional: the section
   * stands alone without one.
   */
  onEndpointChange?: (endpoint: RelayEndpoint) => void;
}

export function RemoteConnectivitySettings({
  onEndpointChange,
}: RemoteConnectivitySettingsProps) {
  // Null until the stored record has landed, so nothing is rendered asserting a
  // state nobody earned.
  const [endpoint, setEndpoint] = useState<RelayEndpoint | null>(null);
  const [urlText, setUrlText] = useState("");
  const [verifying, setVerifying] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  const apply = useCallback(
    (next: RelayEndpoint) => {
      setEndpoint(next);
      setUrlText(next.url ?? "");
      onEndpointChange?.(next);
    },
    [onEndpointChange],
  );

  // GLS-FR-KVNP / GLS-FR-RBLM: initialised from "load relay endpoint" on mount,
  // and re-read rather than re-probed — a verified endpoint stays verified
  // across a relaunch.
  useEffect(() => {
    let cancelled = false;
    void api
      .loadRelayEndpoint()
      .then((loaded) => {
        if (cancelled) return;
        apply(loaded);
      })
      .catch((e) => {
        if (cancelled) return;
        const message = e instanceof Error ? e.message : String(e);
        setSaveError(message);
        logWarn(["frontend", "remote"], "relay endpoint could not be read", {
          error: message,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [apply]);

  /**
   * GLS-FR-RBLM: the change applies immediately, and returns the status line to
   * unverified where it changed the bound URL — which the backend decides
   * rather than this surface guessing (GSS-FR-NLDC).
   */
  const persist = useCallback(
    async (url: string) => {
      setSaveError(null);
      // A new address has not been verified, so whatever the last attempt said
      // about the last one no longer describes this one.
      setFailure(null);
      try {
        const saved = await api.saveRelayEndpoint(url);
        setEndpoint(saved);
        onEndpointChange?.(saved);
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        setSaveError(message);
        // The address is not logged: it names a host the author would not want
        // read back out of an exported log.
        logWarn(["frontend", "remote"], "relay endpoint not saved", {
          error: message,
        });
      }
    },
    [onEndpointChange],
  );

  const onUrlBlur = () => void persist(urlText.trim());

  /**
   * GLS-FR-QWTD: verification is explicit, and it is the one action here that
   * leaves the machine. It carries no credential.
   */
  const verify = async () => {
    if (verifying) return;
    setVerifying(true);
    setFailure(null);
    setSaveError(null);
    try {
      // GLS-FR-RBLM: a URL the author typed and has not left yet is saved
      // before it is verified, so what verifies is what the store holds and an
      // edit is never lost between the two.
      const url = urlText.trim();
      if (url !== (endpoint?.url ?? "")) {
        await persist(url);
      }
      const verified = await api.verifyRelayEndpoint(url);
      apply(verified);
    } catch (e) {
      // The section never presents an endpoint as verified on a relay that did
      // not answer, so a failure leaves the record exactly as the backend
      // reports it and says which correction to make.
      setFailure(relayVerifyMessage(e));
      const message = e instanceof Error ? e.message : String(e);
      logWarn(["frontend", "remote"], "relay endpoint verification refused", {
        reason: message,
      });
    } finally {
      setVerifying(false);
    }
  };

  if (!endpoint) {
    return (
      <div>
        <p className="t-p" style={{ marginBottom: 20 }}>
          The relay your phone reaches this machine through.
        </p>
        <p className="t-p">{saveError ?? "Reading the relay settings…"}</p>
      </div>
    );
  }

  return (
    <div>
      <p className="t-p" style={{ marginBottom: 20 }}>
        The relay your phone reaches this machine through. It belongs to you and
        this machine — a project never carries it, and the relay reads nothing
        it carries between the two.
      </p>

      <div style={{ marginBottom: 20 }}>
        <label className="t-label" htmlFor="relay-endpoint-url">
          Relay address
        </label>
        <input
          id="relay-endpoint-url"
          type="text"
          value={urlText}
          placeholder="https://relay.example.com"
          onChange={(e) => setUrlText(e.target.value)}
          onBlur={onUrlBlur}
          style={{ display: "block", marginTop: 8, width: "100%" }}
        />
      </div>

      <button type="button" onClick={() => void verify()} disabled={verifying}>
        {verifying ? "Verifying…" : "Verify"}
      </button>

      <p className="t-p" role="status" style={{ marginTop: 12 }}>
        {relayStatusLine(endpoint, failure)}
      </p>

      {saveError && (
        <p className="t-p" role="alert" style={{ marginTop: 8 }}>
          {saveError}
        </p>
      )}
    </div>
  );
}

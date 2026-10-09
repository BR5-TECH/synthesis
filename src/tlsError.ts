// The typed TLS error (AAP-ai-api-integrations.md AAP-FR-HZTB, AAP-FR-PKWE).
//
// The backend rejects with the text `tls_untrusted:<cause>:<host>`. This module
// is the one place that reads it and the one place that words it, so every
// surface says the same thing about the same failure.

export const TLS_UNTRUSTED = "tls_untrusted";

export type TlsCause =
  | "unknown_issuer"
  | "expired"
  | "hostname_mismatch"
  | "other";

export interface TlsFailure {
  host: string;
  cause: TlsCause;
}

const CAUSES: readonly TlsCause[] = [
  "unknown_issuer",
  "expired",
  "hostname_mismatch",
  "other",
];

/** The text of a rejection, whatever shape the runtime gave it. */
export function rejectionText(e: unknown): string {
  const raw =
    typeof e === "string" ? e : e instanceof Error ? e.message : "";
  return raw.replace(/^Error:\s*/, "").trim();
}

/** The host and cause in `tls_untrusted:<cause>:<host>`, or null for other text. */
export function parseTlsError(e: unknown): TlsFailure | null {
  const raw = rejectionText(e);
  const prefix = `${TLS_UNTRUSTED}:`;
  if (!raw.startsWith(prefix)) return null;
  const rest = raw.slice(prefix.length);
  const at = rest.indexOf(":");
  if (at === -1) return null;
  const cause = rest.slice(0, at) as TlsCause;
  const host = rest.slice(at + 1);
  if (host === "" || !CAUSES.includes(cause)) return null;
  return { host, cause };
}

/** The reason alone, in words. */
export function tlsCauseText(cause: TlsCause): string {
  switch (cause) {
    case "unknown_issuer":
      return "the issuer of the certificate is unknown";
    case "expired":
      return "the certificate has expired";
    case "hostname_mismatch":
      return "the certificate is not valid for this host name";
    case "other":
      return "the certificate check failed";
  }
}

/** AAP-FR-HZTB: the host and the cause as one sentence. */
export function tlsFailureMessage(failure: TlsFailure): string {
  return `The certificate of ${failure.host} is not trusted: ${tlsCauseText(failure.cause)}.`;
}

/** The sentence for a rejection that is a TLS error, or null for any other. */
export function tlsErrorMessage(e: unknown): string | null {
  const failure = parseTlsError(e);
  return failure ? tlsFailureMessage(failure) : null;
}

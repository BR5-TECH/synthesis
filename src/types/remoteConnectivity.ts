// Remote connectivity (GSS-global-settings-storage.md GSS-FR-ZKQT..GSS-FR-TXAO)
//
// The one relay endpoint this machine reaches, and the validation state it last
// earned. Every name is re-exported from `./index`.

/**
 * GSS-FR-NLDC: whether the URL standing in the store has verified.
 *
 * `unset` while no URL is stored, `unverified` for a stored URL that no success
 * is bound to, and `verified` while the stored URL equals the one the last
 * success used.
 */
export type RelayEndpointState = "unset" | "unverified" | "verified";

/**
 * GSS-FR-ZKQT: what `load relay endpoint`, `save relay endpoint`, and a
 * successful `verify relay endpoint` all return.
 *
 * It holds no `SYNTHESIS_SERVER_TOKEN`, no relay credential, and no private key
 * (GSS-FR-VMRB): the token is runtime configuration, and this record is not
 * where it lives.
 */
export interface RelayEndpoint {
  /** The URL the author gave, or `null` while none has been given. */
  url: string | null;
  state: RelayEndpointState;
  /** The relay version the last success reported. */
  version: string | null;
  /** The capability identifiers the last success reported. */
  capabilities: string[];
  verifiedAt: string | null;
}

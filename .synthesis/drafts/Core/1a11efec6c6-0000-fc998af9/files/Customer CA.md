## Intent

Every outbound HTTP(S) connection of the application must trust the root certificates installed on the User’s machine.

This allows smooth use of the IDE in environments that require custom root certificates (corporate proxies, private gateways), for example for documentation websites or AI providers.

## Scope

In scope: all outbound HTTP(S) of the application — AI API providers (rig, openrouter-rs), AI API verification, documentation fetch, GitHub.

Out of scope: any option to bypass certificate validation (no insecure mode, no per-provider toggle, now or later); a user-chosen CA file in settings.

## User journey

### Happy path

- User tries to use new URL for AI Agent provider
- Certificate is signed by the root CA that’s trusted on User’s computer and connection succeeds.

### Unhappy path

- User tries to use new URL for AI Agent provider
- Certificate is not trusted and connection fails
- User sees a clear error message that names the host and the cause of the TLS failure: unknown issuer, expired certificate, or hostname mismatch.
- The same error shows when a conversation turn fails for the same reason.

## Requirements

- Errors must not be “swallowed".
- Certificates must always be validated. A root CA trusted in the OS store of Windows, macOS or Linux must be trusted in the IDE.
- The OS store is read live on each application start. Bundled Mozilla roots are a fallback: they are used in addition to the OS roots, and alone when the OS store cannot be read.
- One shared TLS configuration (one HTTP client builder) is used by every outbound path in scope, including the clients that rig and openrouter-rs create. No path may keep its own default trust setup.
- AI API verification (`verify_ai_api_integration`) returns a new typed error `tls_untrusted`. The error carries the host and a cause code: `unknown_issuer`, `expired`, `hostname_mismatch`, or `other`. It is distinct from `unreachable` and `rejected`. A TLS failure never maps to `unreachable`.
- Conversation turns that fail for a TLS reason fail with the same typed error, with the same host and cause. They show as a recoverable failure in the conversation.
- Verification of an API-kind agentic integration returns the same typed error.
- Documentation fetch and GitHub calls return the same typed error to their callers, and the UI shows host and cause.
- The error message is shown in the existing verification status line and in the conversation failure display. It is not only a transient toast.
- The message and logs contain no key, token, or request body.

## Specifications to change

- `specifications/core/AAP-ai-api-integrations.md`: add `tls_untrusted` to the typed errors of `verify_ai_api_integration`; state the shared TLS trust rule.
- `specifications/core/AIC-agentic-integrations.md`: add the same error to API-kind verification.
- `specifications/core/AGC-agent-conversations.md`: add the TLS failure to the turn failures (and the recoverable failures of the comment agent turns specification).
- `specifications/ui/AII-ai-integrations.md`: render host and cause in the verification status line.
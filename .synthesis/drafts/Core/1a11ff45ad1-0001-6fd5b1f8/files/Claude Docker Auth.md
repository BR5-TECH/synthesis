## Intent

The Claude Code Agentic AI integration must support a Custom Gateway: a user-provided Anthropic-compatible base URL and token, in addition to the existing Subscription OAuth token.

## User journey

- User opens Agentic AI tab in General settings.
- User selects Claude Code tab
- User has a sub-tab to chose between Subscription and Custom Gateway
- IF: user selects Subscription - behavior stays as is: we show the OAuth token field, and the token is passed down to Docker as CLAUDE_CODE_OAUTH_TOKEN.
- IF: user selects Custom Gateway option, user provides a gateway base URL and a token. The URL is passed down as ANTHROPIC_BASE_URL. The token is passed down under the variable name in the Token variable name field, which the user can edit and which defaults to ANTHROPIC_AUTH_TOKEN. Subscription's CLAUDE_CODE_OAUTH_TOKEN is not used in this mode.
- In both cases User has an optional array to specify env vars in key=value format.
- All key/value pairs and credentials are passed down in both cases. If an array key equals a key set by the typed fields, the array value wins.

## Requirements

- User can verify Custom Gateway using a verify button. It sends a GET request to `<base URL>/v1/models` with the token, and shows the result: the model count on success, or the HTTP status or network error on failure.
- The gateway token is stored in the application's secret vault, like the OAuth token. It never appears in logs, errors, command arguments, or the settings file.
- Specs to amend so that this mode is allowed: `specifications/infra/CCP-claude-code-cli-protocol.md` (CCP-FR-20, which allows only CLAUDE_CODE_OAUTH_TOKEN), `specifications/tools/EAC-execute-agent-cli.md` (EAC-FR-15), and `specifications/core/AIC-agentic-integrations.md` (AIC-FR-30 handoff and the secret-vault description). Update `specifications/ui/AII-ai-integrations.md` for the sub-tab and fields.
- Look-and-feel must be aligned with existing implementation of the application settings.

## Out of scope

- Codex and OpenCode integrations are out of scope at this moment.
- AWS Bedrock and any other credential form (AWS keys, bearer tokens) are out of scope.
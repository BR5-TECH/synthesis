# MCP server connections

**Spec code:** `MCS`

## Intent
A project can connect to **MCP servers** (Model Context Protocol), so that the agents in its conversations can use tools that live outside the application: an issue tracker, a database, a documentation index, an internal service. The author configures each server one time for the project, and every conversation agent of that project can then use its tools (per `../ai/CVL-conversation-loop.md` CVL-FR-TJZI). This module owns the server records, the secrets they need, the trust a local command needs before it runs, the OAuth sign-in a remote server can ask for, and the live connection to each server. A server record is part of the project and is committed, so a team shares one list of servers. A secret, a trust decision, and a sign-in are facts of one machine, so they are never committed. Out of scope: MCP for the agentic CLIs of graduation (`../infra/CCP-claude-code-cli-protocol.md`, `../infra/CDX-codex-cli-protocol.md`), and the MCP resources, prompts, sampling, and elicitation features. This module uses the tools feature of MCP alone.

## Functional requirements

### Records
1. **MCS-FR-EKPA** The MCP server records of a project are project-public. They are the `mcpServers` array of tables in `.synthesis/project.toml` (per `PSS-project-settings-storage.md` PSS-FR-09), one table for each server, in the order the author added them.
2. **MCS-FR-XOSW** A record holds `id`, `name`, `enabled`, `transport`, and the fields of its transport (per the contract surface). `transport` is `stdio` or `http`, and a record with any other `transport`, or with none, is `invalid_transport`. A record holds no secret value: a secret is named in the record and its value is in the vault (MCS-FR-SBND).
3. **MCS-FR-IQOF** A file record with no `enabled` key is enabled. A file record whose `enabled` is not a boolean is not enabled.
   - *Why:* A value such as `"false"` shows that the author meant to turn the server off.
3. **MCS-FR-LLQD** `id` matches `^[a-z0-9][a-z0-9-]{0,31}$` and is unique in the project. `id` does not change after the record is created. `name` is 1 to 80 characters after trimming.
4. **MCS-FR-KALQ** A `stdio` record holds a `command` that is not empty, an `args` list, an optional `cwd`, an `env` table of plain values, and a `secretEnv` list of variable names. A variable name matches `^[A-Za-z_][A-Za-z0-9_]*$`, and one name is not in both `env` and `secretEnv`.
5. **MCS-FR-KLQS** A `stdio` record's `cwd` is a relative path. It resolves inside the active worktree. A `cwd` that is absolute, or that leaves the worktree, is `invalid_cwd`. With no `cwd`, the server runs in the root of the active worktree.
6. **MCS-FR-DINZ** An `http` record holds a `url`, a `headers` table of plain values, a `secretHeaders` list of header names, and an `auth` mode: `none`, `bearer`, or `oauth2`. A header name is an HTTP token, and one name is not in both `headers` and `secretHeaders`. A `headers` value that is not a valid HTTP header value is `invalid_header_value`.
7. **MCS-FR-XDGN** An `http` record's `url` uses `https`. A `url` that uses `http` is valid only when its host is `localhost`, `127.0.0.1`, or `::1`. Any other `url` is `invalid_url`.
   - *Why:* A plain HTTP connection to another host would send a bearer token or an OAuth token in clear text.
8. **MCS-FR-FJLR** When `auth` is `bearer` or `oauth2`, no name in `headers` or `secretHeaders` is `Authorization` in any letter case. Such a record is `authorization_header_conflict`.
9. **MCS-FR-UAAN** An `oauth2` record can hold a `clientId`, a `scopes` list, and a `callbackPort` from 1024 to 65535. Each is optional. A record with a `clientId` can also need the `oauth_client_secret` secret slot (MCS-FR-SBND).
10. **MCS-FR-HBYA** A record holds a `disabledTools` list of MCP tool names. A tool in that list is never offered to an agent (per `../tools/MCT-mcp-tool.md` MCT-FR-GPCO). A name in the list that the server does not have is kept and has no effect.
11. **MCS-FR-FIEE** `list_mcp_servers` returns every record of the project in file order, each with its status (MCS-FR-RXZG), the presence of each of its secret slots, and the tools the server offers now. It returns no secret value.
12. **MCS-FR-HBOD** A record in the file that fails validation is listed with the status `invalid` and the validation code. It never connects. It stays in the file until the author saves a valid record over it or removes it.
13. **MCS-FR-XCOS** `save_mcp_server(server)` validates the record. A valid record replaces the record with the same `id`, or is added at the end when no record has that `id`. A refused record changes nothing and returns the first validation code.
14. **MCS-FR-JZDF** `save_mcp_server` and `remove_mcp_server` change the `mcpServers` array alone. Every other key of `.synthesis/project.toml` is kept as it was (per `PSS-project-settings-storage.md` PSS-FR-04).
15. **MCS-FR-UOIO** `remove_mcp_server(id)` removes the record, stops its connection, removes every secret of that server from the vault, and removes its trust record (MCS-FR-PPPL).

### Secrets
16. **MCS-FR-SBND** A server secret is stored in the vault at `["projects", <project key>, "mcp", <server id>, <slot>]` (per `ASV-application-secret-vault.md` ASV-FR-FFKB). The project key is the key of the project's user-global slot (per `GSS-global-settings-storage.md` GSS-FR-18).
17. **MCS-FR-QBFA** The secret slots of a server are `env:<NAME>` for each name in `secretEnv`, `header:<NAME>` for each name in `secretHeaders`, `bearer` when `auth` is `bearer`, and `oauth_client_secret` when the record holds a `clientId`. The `oauth` slot holds the sign-in state (MCS-FR-MGCJ) and is not a settable slot.
18. **MCS-FR-IXHD** `set_mcp_server_secret(id, slot, value)` stores a value that is not empty in a settable slot of that server. A value for a `header:` slot or the `bearer` slot that holds a line break is `invalid_secret`. A slot the record does not declare is `unknown_slot`.
19. **MCS-FR-WBDV** `clear_mcp_server_secret(id, slot)` removes the value of one settable slot. A slot with no value is not an error.
20. **MCS-FR-EHBW** `save_mcp_server` of a record whose id has no record in the file first removes every secret of that id from the vault, so a record added under an old id starts with no secret.
20. **MCS-FR-OCHR** `save_mcp_server` removes from the vault the value of each slot that the saved record no longer declares. It also removes the `oauth` slot when the record had `auth = oauth2` and the saved record has another `auth` or another `url`.
21. **MCS-FR-PXRJ** A server with a declared slot that has no value has the status `needs_secret` and does not connect. The `oauth_client_secret` slot is optional and does not cause `needs_secret`.

### Trust
22. **MCS-FR-PPPL** A `stdio` server, and an `http` server that uses a credential (MCS-FR-KXYO), connects only when this machine trusts it. The trust record is in the project's user-global slot (per `GSS-global-settings-storage.md` GSS-FR-UTCM). It holds the **trust fingerprint** of the record that the author trusted.
    - *Why:* `.synthesis/project.toml` is committed, so a clone or a pull can bring a command, or a destination for a credential, that nobody on this machine has seen.
23. **MCS-FR-KWCN** The trust fingerprint of a `stdio` record is the SHA-256 of a canonical JSON object of `command`, `args`, `cwd`, `env`, and `secretEnv`. The trust fingerprint of an `http` record is the SHA-256 of a canonical JSON object of `url`, `auth`, `clientId`, and `secretHeaders`. A change of `name`, `enabled`, or `disabledTools` does not change a fingerprint.
24. **MCS-FR-HQJQ** A server that needs trust and has no trust record, or a trust record whose fingerprint is not the trust fingerprint of its record now, has the status `untrusted`. The application starts no process, sends no request, and reads no secret for an untrusted server, also not for `test_mcp_server` or `start_mcp_sign_in`.
25. **MCS-FR-EVRS** `trust_mcp_server(id, shown)` records the trust fingerprint of `shown`, the record the trust dialog showed, when it equals the trust fingerprint of the record in the file now. Otherwise it refuses with `record_changed` and records nothing. It refuses an `http` record that uses no credential with `no_credentials`.
25. **MCS-FR-LXLB** When `trust_mcp_server` trusts an `http` record whose stored trust fingerprint is absent or differs, it first removes the `bearer`, every `header:<NAME>`, the `oauth`, and the `oauth_client_secret` slot of that server.
    - *Why:* A credential is given for one destination, and a trust step for another destination must not send it there.
26. **MCS-FR-EUCA** `save_mcp_server` of a record that needs trust records its trust fingerprint when the save adds the record, or when it changes the trust fingerprint of the record. A save that keeps the trust fingerprint, such as a rename or the `enabled` switch, keeps the trust state as it was.
    - *Why:* The author typed the new values on this machine, but a save that keeps them must not trust a record that a pull brought.
27. **MCS-FR-RISC** An `http` server with `auth = none` and no `secretHeaders` needs no trust record.
27. **MCS-FR-KXYO** An `http` record uses a credential when its `auth` is `bearer` or `oauth2`, or when it has a `secretHeaders` name.
    - *Why:* A pull that changes the `url` must not send a token or a secret header to the new host.

### Status and lifecycle
28. **MCS-FR-RXZG** The status of a server is one of `disabled`, `invalid`, `untrusted`, `needs_secret`, `needs_sign_in`, `signing_in`, `connecting`, `ready`, and `failed`. `ready` carries the count of tools the server offers. `failed` carries a failure class (MCS-FR-JNXV).
29. **MCS-FR-UXPP** The application connects each server whose record is valid, `enabled`, trusted where it is `stdio`, with no missing secret, and signed in where `auth` is `oauth2`. A server with `enabled = false` has the status `disabled` and no connection.
30. **MCS-FR-RYIK** The application reconciles the connections with the records when a project opens, when the active worktree changes (per `WTC-worktree-context.md` WTC-FR-08), at each `list_mcp_servers`, and after each operation of this module that changes a record, a secret, a trust record, or a sign-in. A reconcile starts, stops, or restarts only the servers whose effective configuration changed.
30. **MCS-FR-YEXD** The **effective configuration** of a server is its record without `name` and `disabledTools`, together with the facts of MCS-FR-UXPP. A change of `name` or `disabledTools` alone keeps the session, and a `ready` server then shows the tool count of its new filter.
31. **MCS-FR-KCIH** A change to `.synthesis/project.toml` made outside the application takes effect at the next reconcile, at the latest when the settings section lists the servers again. `reconnect_mcp_server(id)` reconciles that one server at once: it rereads its record and starts a new connection.
32. **MCS-FR-BYBA** A connection opens with the MCP `initialize` handshake and then lists the server's tools with `tools/list`, following every page. The two steps together have a limit of 30 seconds. A server that does not complete them in that time is `failed` with `timed_out`.
33. **MCS-FR-XDZK** A `ready` server keeps one MCP session. When the server sends `notifications/tools/list_changed`, the application lists the tools again and replaces the tools it holds for that server.
34. **MCS-FR-HXJF** A `ready` server whose session closes, or whose process exits, becomes `connecting` and is connected again after waits of 1, 2, 4, 8, and 16 seconds. After the fifth failed attempt it is `failed`.
34. **MCS-FR-EMPU** A session that ends less than 10 seconds after it opened counts as a failed reconnect attempt, so the waits of MCS-FR-HXJF do not start again. A `failed` server stays `failed` until its effective configuration changes (MCS-FR-YEXD) or `reconnect_mcp_server` names it.
35. **MCS-FR-JNXV** A failure class is one of `command_not_found`, `process_exited`, `unreachable`, `tls_untrusted`, `rejected`, `protocol_error`, `timed_out`, `oauth_failed`, and `keychain_unavailable`. `rejected` is an HTTP 401 or 403 response that a refresh did not fix (MCS-FR-SZOY).
36. **MCS-FR-NOKX** Each status change emits the event `mcp-server-status-changed` with the server id and its new status. The event carries no URL, command, argument, header, secret, or tool argument.
37. **MCS-FR-QTSW** When the project closes, and when the application quits, the application ends every MCP session. It stops each `stdio` process and every process that process started, within 5 seconds.

### The stdio transport
38. **MCS-FR-APCF** A `command` that holds no path separator is resolved as the OpenSpec CLI is resolved: the first executable of that name in the directories of `OSM-openspec-mode.md` OSM-FR-EZKW. A `command` with a path separator resolves against the server's working directory. A command that does not resolve is `failed` with `command_not_found`.
39. **MCS-FR-KAYL** The process gets the environment of the application, then `PATH` with the directory of the resolved executable first and the directories of MCS-FR-APCF after it, then the `env` values, then the `secretEnv` values from the vault. A later value replaces an earlier value of the same name.
    - *Why:* `npx` and `uvx` start `node` and `python` by name, and a desktop application on macOS gets a short `PATH`.
40. **MCS-FR-ROVQ** The server speaks MCP on the standard input and output of its process. The application reads its standard error and keeps the last 20 lines of it in memory for `test_mcp_server` alone. No log record and no event carries a line of standard error.
41. **MCS-FR-BLBG** A `stdio` process leads its own process group where the platform has one, and the application stops the whole group when it ends the session.

### The HTTP transport
42. **MCS-FR-MDBI** An `http` server is reached with the MCP Streamable HTTP transport at its `url`. Each request carries the `headers` values, the `secretHeaders` values from the vault, and the `Authorization` header of its `auth` mode. With `auth = none`, no `Authorization` header is added.
43. **MCS-FR-PBFI** With `auth = bearer`, each request carries `Authorization: Bearer <value>`, where the value is the `bearer` slot.
44. **MCS-FR-LZBV** TLS certificates are checked against the trust store the AI API calls use (per `AAP-ai-api-integrations.md` AAP-FR-05). A certificate that the check refuses is `failed` with `tls_untrusted`.

### OAuth 2 sign-in
45. **MCS-FR-MGCJ** With `auth = oauth2`, the `oauth` slot holds the sign-in state: the client id and client secret a dynamic registration gave, the access token, the refresh token, the time the access token expires, and the scopes granted. A server with no access token and no refresh token has the status `needs_sign_in`.
46. **MCS-FR-YWVC** `start_mcp_sign_in(id)` refuses a server whose `auth` is not `oauth2` with `not_oauth2`, and a server that already has the status `signing_in` with `sign_in_in_progress`. Otherwise it sets the status `signing_in` and returns. The result arrives as a status change (MCS-FR-NOKX).
47. **MCS-FR-SAQY** The sign-in discovers the authorization server from the protected resource metadata of the MCP server (RFC 9728), found from the `WWW-Authenticate` header of a 401 response or at the well-known path. It then reads the authorization server metadata (RFC 8414, else OpenID Connect discovery).
48. **MCS-FR-PJNN** The sign-in uses the record's `clientId` when it holds one. Otherwise it uses the client in the `oauth` slot, and when there is none it registers a client with dynamic client registration (RFC 7591) as a public client named `Synthesis`, and stores that client in the `oauth` slot.
49. **MCS-FR-RBCD** The sign-in listens for the redirect on `127.0.0.1` alone, at `callbackPort` or at a free port, on the path `/callback`. It opens the authorization URL in the system browser. The request uses the authorization code flow with PKCE `S256`, a random `state`, and the `resource` parameter set to the server `url` (RFC 8707).
50. **MCS-FR-NRXZ** The request asks for the record's `scopes`. With no `scopes` in the record, it asks for the `scopes_supported` of the protected resource metadata, and it asks for no scope when that list is absent.
51. **MCS-FR-TGFR** A redirect with the sent `state` and an `error` parameter ends the sign-in as `failed` with `oauth_failed`. A request to the callback path with no `state`, or with another `state`, gets the failure page, and the sign-in waits on. Each answer is a short page that says whether the sign-in succeeded and tells the author to go back to Synthesis.
51. **MCS-FR-GHOV** The listener serves several connections at the same time, so a connection that sends nothing does not delay a redirect on another connection.
51. **MCS-FR-ZCPK** A sign-in whose record has another trust fingerprint when it ends stores no token, and ends as `failed` with `oauth_failed`.
52. **MCS-FR-GTTD** A valid redirect is exchanged for tokens at the token endpoint, and the tokens are stored in the `oauth` slot. The server then connects (MCS-FR-UXPP).
53. **MCS-FR-CMWV** A sign-in with no valid redirect after 5 minutes ends as `failed` with `oauth_failed`. `cancel_mcp_sign_in(id)` ends a sign-in at once and sets the status the server had before it. Both close the listener.
54. **MCS-FR-SZOY** Before a request, an access token that expires in less than 30 seconds is refreshed with the refresh token. A request that gets a 401 response refreshes the token one time and sends the request again.
55. **MCS-FR-SQWD** A refresh that the authorization server refuses removes the tokens from the `oauth` slot, keeps the registered client, and sets the status `needs_sign_in`. A refresh that cannot reach the server keeps the tokens and sets `failed` with `unreachable`.
56. **MCS-FR-QRIB** `sign_out_mcp_server(id)` removes the tokens from the `oauth` slot, keeps the registered client, ends the session, and sets the status `needs_sign_in`.

### Test
57. **MCS-FR-ETIS** `test_mcp_server(id)` opens a new connection to the saved record, completes the handshake and the tool list, closes the connection, and returns the server's tools with their descriptions. It does not change the status or the tools held, and it writes no store, but for the refreshed token of an `oauth2` server whose access token was about to expire (MCS-FR-SZOY).
58. **MCS-FR-ERQP** A test that fails returns the failure class. For a `stdio` server it also returns the last 20 lines of standard error. A test of a server that is `invalid`, `untrusted`, `needs_secret`, or `needs_sign_in` returns that status as its refusal and opens no connection.

### Use by conversations
59. **MCS-FR-ENII** `mcp_tools_now(project key)` returns the tools of every `ready` server of the project, from what the application holds in memory. It waits for no connection, no listing, and no network.
60. **MCS-FR-LTTM** `call_mcp_tool(server id, tool name, arguments, deadline)` sends `tools/call` on the server's current session. When the deadline passes first, it sends `notifications/cancelled` for that request and returns `timed_out`. A server that is not `ready` returns `not_connected`.

### Logging
61. **MCS-FR-KEEF** The module logs under the `backend` domain through `LGC-logging.md`: an `INFO` record for each connection opened and closed, each sign-in that starts and ends, and each status change; a `WARN` record for each failure, with its failure class.
62. **MCS-FR-MABN** A log record of this module names the server id, the transport, the auth mode, the status, the failure class, and a duration. It names no URL, command, argument, environment value, header, secret, token, authorization code, tool argument, or tool result.

## Contract surface

### The record
```
McpServer {
  id:            string           // MCS-FR-LLQD
  name:          string
  enabled:       bool
  disabledTools: string[]         // MCS-FR-HBYA
  transport:     McpStdio | McpHttp
}

McpStdio {
  kind:      "stdio"
  command:   string
  args:      string[]
  cwd:       string | null        // relative to the active worktree
  env:       { [name]: string }   // plain values, committed
  secretEnv: string[]             // names only; values in the vault
}

McpHttp {
  kind:          "http"
  url:           string
  headers:       { [name]: string }
  secretHeaders: string[]
  auth:          "none" | "bearer" | "oauth2"
  clientId:      string | null    // oauth2 only
  scopes:        string[]         // oauth2 only
  callbackPort:  number | null    // oauth2 only
}
```

In `.synthesis/project.toml` (keys in camelCase, the transport kind in `transport`):

```
[[mcpServers]]
id = "tracker"
name = "Issue tracker"
enabled = true
transport = "http"
url = "https://mcp.tracker.example/mcp"
auth = "oauth2"
scopes = ["issues:read", "issues:write"]

[[mcpServers]]
id = "docs"
name = "Docs index"
enabled = true
transport = "stdio"
command = "npx"
args = ["-y", "docs-mcp-server"]
secretEnv = ["DOCS_API_KEY"]
disabledTools = ["reindex"]
```

### The listed entry
```
McpServerEntry {
  id:          string
  record:      McpServer | null            // null when invalid
  invalid:     McpConfigError | null       // MCS-FR-HBOD
  status:      McpServerStatus
  secrets:     { slot: string, present: bool }[]   // settable slots only
  tools:       McpToolInfo[]               // empty unless ready
}

McpServerStatus =
  | { state: "disabled" | "invalid" | "untrusted" | "needs_secret"
           | "needs_sign_in" | "signing_in" | "connecting" }
  | { state: "ready", toolCount: number }
  | { state: "failed", failure: McpFailure }

McpFailure = "command_not_found" | "process_exited" | "unreachable" | "tls_untrusted"
           | "rejected" | "protocol_error" | "timed_out" | "oauth_failed"
           | "keychain_unavailable"

McpToolInfo { name: string, description: string, disabled: bool }

McpTestResult =
  | { ok: true,  tools: McpToolInfo[] }
  | { ok: false, failure: McpFailure | McpServerStatus["state"], stderrTail: string[] }
```

### Tauri commands
- `"list MCP servers"` → `list_mcp_servers()` → `McpServerEntry[]`
- `"save MCP server"` → `save_mcp_server(server: McpServer)` → `McpServerEntry`, or `McpConfigError`
- `"remove MCP server"` → `remove_mcp_server(id)` → `()`, or `not_found`
- `"set MCP server secret"` → `set_mcp_server_secret(id, slot, value)` → `McpServerEntry`, or `unknown_slot`, `invalid_secret`, `not_found`, `keychain_unavailable`
- `"clear MCP server secret"` → `clear_mcp_server_secret(id, slot)` → `McpServerEntry`, or `unknown_slot`, `not_found`, `keychain_unavailable`
- `"trust MCP server"` → `trust_mcp_server(id, shown: McpServer)` → `McpServerEntry`, or `record_changed`, `no_credentials`, `not_found`
- `"test MCP server"` → `test_mcp_server(id)` → `McpTestResult`, or `not_found`
- `"reconnect MCP server"` → `reconnect_mcp_server(id)` → `()`, or `not_found`
- `"start MCP sign-in"` → `start_mcp_sign_in(id)` → `()`, or `not_oauth2`, `sign_in_in_progress`, `not_found`
- `"cancel MCP sign-in"` → `cancel_mcp_sign_in(id)` → `()`
- `"sign out of MCP server"` → `sign_out_mcp_server(id)` → `McpServerEntry`, or `not_oauth2`, `not_found`, `keychain_unavailable`

Every command refuses with `no_project_open` when no project is open, and with `malformed_project_config` when `.synthesis/project.toml` does not parse (per `PSS-project-settings-storage.md` PSS-FR-10).

```
McpConfigError = "invalid_id" | "duplicate_id" | "invalid_name" | "invalid_transport"
               | "invalid_command" | "invalid_cwd" | "invalid_url" | "invalid_env_name"
               | "invalid_header_name" | "invalid_header_value" | "duplicate_name"
               | "authorization_header_conflict" | "invalid_callback_port" | "invalid_auth"
```

### Tauri event
- `mcp-server-status-changed` → `{ id: string, status: McpServerStatus }` (MCS-FR-NOKX)

### Internal (no UI consumer; not Tauri commands)
- `mcp_tools_now(project_key) -> McpToolSnapshot[]` — MCS-FR-ENII. `McpToolSnapshot { server_id, server_name, tool_name, description, input_schema }`, for each tool that is not in `disabledTools`.
- `call_mcp_tool(server_id, tool_name, arguments, deadline) -> Result<CallToolResult, McpCallError>` — MCS-FR-LTTM. `McpCallError = not_connected | timed_out | protocol_error`.

## Non-functional requirements
- The MCP client is the `rmcp` crate. It is pure Rust and links no host library on macOS (per `../infra/NLL-native-library-linkage.md`).
- MCP servers run on the host machine with the rights of the user, not in a sandbox. The trust step (MCS-FR-PPPL) is the one gate before a local command runs.
- Secrets stay in the vault and in process memory alone (per `ASV-application-secret-vault.md` ASV-FR-28).
- A record that a pull removes leaves its secrets and its trust record on this machine. They do nothing without a record.
- One session serves every turn of the project. Requests of several turns run at the same time on that session.

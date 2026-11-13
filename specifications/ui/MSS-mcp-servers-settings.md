# MCP servers settings

**Spec code:** `MSS`

## Intent
The **MCP servers** section of the Project group of the Settings window (per `SET-project-settings.md` SET-FR-06). Here the author adds the MCP servers whose tools the agents of this project can use, gives each server the secrets it needs on this machine, trusts a local command before it runs, signs in to a remote server that asks for OAuth, and sees at a glance which servers are connected. The list of servers is committed with the project, so the section also tells the author clearly what a teammate's server needs on this machine before it can work. The records, connections, and sign-in are `../core/MCS-mcp-server-connections.md`'s.

## Functional requirements

### The list
1. **MSS-FR-DZIQ** The section heading is followed by the sentence `The MCP servers whose tools the agents of this project can use.`
2. **MSS-FR-XALV** The section lists every server that `list_mcp_servers` returns, in that order. Each row shows the server name, its id, a transport badge (`Local` for `stdio`, `Remote` for `http`), and a status chip.
3. **MSS-FR-CSHM** With no server, the section shows `No MCP servers yet. Add one to give the agents of this project its tools.` and the `Add server` button.
4. **MSS-FR-PEGO** The status chip text is: `Off` (disabled), `Invalid` (invalid), `Not trusted` (untrusted), `Needs a secret` (needs_secret), `Sign in needed` (needs_sign_in), `Signing in…` (signing_in), `Connecting…` (connecting), `Connected · <n> tools` (ready), and `Failed` (failed).
5. **MSS-FR-YIMR** A row with the status `failed` shows one line under it with the text of its failure class (MSS-FR-VUWB). A row with the status `invalid` shows the text of its validation code under it.
6. **MSS-FR-VUWB** The failure class text is exactly this:

   ```
   command_not_found     The command was not found on this computer.
   process_exited        The server process stopped.
   unreachable           The server cannot be reached.
   tls_untrusted         The server certificate is not trusted.
   rejected              The server refused the credentials.
   protocol_error        The server did not answer as an MCP server.
   timed_out             The server did not answer in time.
   oauth_failed          The sign-in did not complete.
   keychain_unavailable  The system keychain is not available.
   ```
7. **MSS-FR-CLQR** The section subscribes to `mcp-server-status-changed` while it is open and updates the chip and the failure line of that row at once, with no reload of the list.

### Row actions
8. **MSS-FR-GBOM** Each row has an `Enabled` switch, a `Test` button, an `Edit` button, and a `Remove` button. The switch saves the record with the new `enabled` value through `save_mcp_server`, and goes back to its last value when the save is refused.
9. **MSS-FR-YFPL** A row with the status `untrusted` has a `Trust…` button, for a `Local command` and for a `Remote (HTTP)` server alike. It opens the trust dialog (MSS-FR-VJPZ, MSS-FR-SCFN).
10. **MSS-FR-HXVP** A row with the status `needs_sign_in`, or `failed` with `oauth_failed` or `rejected` where `auth` is `oauth2`, has a `Sign in` button. It calls `start_mcp_sign_in`. A row with the status `signing_in` shows `Waiting for the browser…` and a `Cancel` button that calls `cancel_mcp_sign_in`.
11. **MSS-FR-ANUU** A row of a server with `auth = oauth2` that is `ready` has a `Sign out` button that calls `sign_out_mcp_server`.
12. **MSS-FR-PDVK** A row with the status `needs_secret` shows `Add the secrets this server needs on this computer.` and a `Set secrets` button that opens the edit dialog with focus on the first secret that has no value.
13. **MSS-FR-WHXM** A row with the status `failed` has a `Reconnect` button that calls `reconnect_mcp_server`.
14. **MSS-FR-VWKQ** `Test` calls `test_mcp_server` and shows the result under the row until the next test or until the section closes. A success shows `Connected. <n> tools.` A failure shows the failure text (MSS-FR-VUWB) and, where the result has them, the standard error lines in a monospace block.
14. **MSS-FR-TMUX** A test that the backend refused for the status of the server (per `../core/MCS-mcp-server-connections.md` MCS-FR-ERQP) shows the text of that status, exactly this:

    ```
    disabled       The server is off.
    invalid        The server record is not valid.
    untrusted      Trust this server before it runs.
    needs_secret   Add the secrets this server needs on this computer.
    needs_sign_in  Sign in to this server first.
    signing_in     The sign-in is not complete.
    connecting     The server is connecting.
    ```
15. **MSS-FR-KQNF** `Remove` opens a confirmation: `Remove <name>? Its secrets and its sign-in on this computer are removed too.` with `Cancel` and `Remove`. `Remove` calls `remove_mcp_server` and removes the row.

### Tools
16. **MSS-FR-QMTG** A row of a `ready` server has a `Tools (<n>)` disclosure. Open, it lists each tool of the entry with its name, the first line of its description, and a switch.
17. **MSS-FR-MMLA** A tool switch that is turned off adds the tool name to `disabledTools`, and one that is turned on removes it, through `save_mcp_server`. A refused save puts the switch back.

### The add and edit dialog
18. **MSS-FR-MWAV** `Add server` opens the dialog empty, and `Edit` opens it with the record of the row. The dialog has `Name`, `ID`, and a transport choice of `Local command` and `Remote (HTTP)`.
19. **MSS-FR-DVDX** On add, `ID` follows `Name` as a slug until the author edits `ID`. On edit, `ID` is read-only.
20. **MSS-FR-XMNV** For `Local command`, the dialog has `Command`, `Arguments` (one argument on each line), `Working folder` (relative to the project, optional), and an environment list.
21. **MSS-FR-HIWH** For `Remote (HTTP)`, the dialog has `URL`, an `Authentication` choice of `None`, `Bearer token`, and `OAuth 2`, and a header list. `Bearer token` shows a `Token` secret field. `OAuth 2` shows `Client ID` (optional), `Client secret` (optional secret field, shown only with a client ID), `Scopes` (separated by spaces, optional), and `Callback port` (optional).
22. **MSS-FR-BHNZ** Each row of the environment list and of the header list has a name, a value, and a `Secret` check box. A plain row saves its value into the record. A secret row saves its name into `secretEnv` or `secretHeaders` and its value into the vault.
23. **MSS-FR-IUMU** A secret field shows `Set on this computer` or `Not set` from the entry, and is empty. An empty secret field keeps the stored value. A typed value replaces it. A `Clear` link beside a set field clears the value when the dialog saves.
24. **MSS-FR-BEQD** The dialog checks the fields as the core does (per `../core/MCS-mcp-server-connections.md` MCS-FR-LLQD, MCS-FR-KALQ, MCS-FR-KLQS, MCS-FR-DINZ, MCS-FR-XDGN, MCS-FR-FJLR, MCS-FR-UAAN) and shows each problem under its field. `Save` is disabled while a problem shows.
24. **MSS-FR-OKDG** A validation code shows this text under its field, exactly this:

    ```
    invalid_id                     Use 1 to 32 lowercase letters, digits, and hyphens. Start with a letter or a digit.
    duplicate_id                   Another server of this project has this ID.
    invalid_name                   Enter a name of 1 to 80 characters.
    invalid_transport              Choose a transport.
    invalid_command                Enter a command.
    invalid_cwd                    Use a relative path to a folder in the project.
    invalid_url                    Enter an https URL.
    invalid_env_name               Use letters, digits, and underscores. Do not start with a digit.
    invalid_header_name            Use a valid HTTP header name.
    invalid_header_value           A header value cannot hold a line break.
    duplicate_name                 This name is in the list more than one time.
    authorization_header_conflict  Remove this header. The authentication sets the Authorization header.
    invalid_callback_port          Enter a port from 1024 to 65535.
    invalid_auth                   Choose an authentication.
    ```
25. **MSS-FR-UWZQ** `Save` calls `save_mcp_server`, then `set_mcp_server_secret` for each typed secret, then `clear_mcp_server_secret` for each cleared secret. A refusal shows its text under the field it names, or at the top of the dialog, and the dialog stays open.
26. **MSS-FR-STXZ** A refusal of `save_mcp_server` keeps every field as the author typed it. A secret step that fails after a save keeps the dialog open and shows which secret was not stored.
27. **MSS-FR-GHDO** A `Remote (HTTP)` `URL` that starts with `http://` and has a host other than `localhost`, `127.0.0.1`, or `::1` shows `Use https for a server on another computer.`

### Trust
28. **MSS-FR-VJPZ** The trust dialog of a `Local command` shows `This project runs this command on your computer:`, then the command and its arguments as one line, the working folder, each plain environment variable with its value, and the names of the secret variables. It shows no secret value. It has `Cancel` and `Trust and start`.
28. **MSS-FR-SCFN** The trust dialog of a `Remote (HTTP)` server shows `This project sends your credentials for this server to:`, then the URL, the authentication, and the names of the secret headers. It shows no secret value. It has `Cancel` and `Trust and start`.
28. **MSS-FR-VNIK** The trust dialog of a `Remote (HTTP)` server also shows `Stored credentials for this server are removed. Enter them again after you trust it.` (per `../core/MCS-mcp-server-connections.md` MCS-FR-LXLB).
29. **MSS-FR-GJVD** `Trust and start` calls `trust_mcp_server` with the record the dialog shows, and closes the dialog. A record that the dialog adds, or whose trusted values it changes, needs no trust step (per `../core/MCS-mcp-server-connections.md` MCS-FR-EUCA).
29. **MSS-FR-UVWX** A `record_changed` refusal shows `The record changed. Close this and look at it again.` in the trust dialog, and the dialog stays open.
29. **MSS-FR-TBQY** After `Trust and start`, focus goes to the row of the server. After a confirmed `Remove`, focus goes to the `Add server` button.

### Loading and errors
30. **MSS-FR-RYEQ** The section loads the list when it mounts and shows `Loading MCP servers…` until it arrives. A load that fails shows its error in the section and a `Try again` button.
31. **MSS-FR-GMHC** The section holds no dirty state: each action writes at once (per `SET-project-settings.md` SET-FR-08). The dialog holds the unsaved edits, and closing it with `Cancel` or Escape drops them.
32. **MSS-FR-VSMZ** Each action logs its start at `INFO` and each refusal at `WARN` with the server id and the error code, through `src/logging.ts`. No record holds a field value.

## User stories
- As an author, I want to add our issue tracker's MCP server one time, so that every agent in the project can read and file issues.
- As a teammate who pulls the project, I want to see which servers need a secret or a sign-in on my computer, so that I can make them work in one place.
- As an author, I want to see the exact command before a local server runs, so that a pulled change cannot run a command I did not check.

## Wireframes

```
MCP servers
The MCP servers whose tools the agents of this project can use.

┌──────────────────────────────────────────────────────────────────────────┐
│ Issue tracker   tracker   Remote   ● Connected · 14 tools                │
│   [on]  [ Test ] [ Edit ] [ Sign out ] [ Remove ]                        │
│   ▸ Tools (14)                                                           │
├──────────────────────────────────────────────────────────────────────────┤
│ Docs index      docs      Local    ● Not trusted                         │
│   [on]  [ Trust… ] [ Test ] [ Edit ] [ Remove ]                          │
├──────────────────────────────────────────────────────────────────────────┤
│ Metrics         metrics   Remote   ● Failed                              │
│   The server cannot be reached.                                          │
│   [on]  [ Reconnect ] [ Test ] [ Edit ] [ Remove ]                       │
└──────────────────────────────────────────────────────────────────────────┘
                                                        [ + Add server ]
```

```
┌─ Edit MCP server ──────────────────────────────────────────┐
│ Name       [ Issue tracker                         ]       │
│ ID         tracker                                         │
│ Transport  ( ) Local command  (•) Remote (HTTP)            │
│                                                            │
│ URL             [ https://mcp.tracker.example/mcp     ]    │
│ Authentication  ( ) None  ( ) Bearer token  (•) OAuth 2    │
│ Client ID       [                                     ]    │
│ Scopes          [ issues:read issues:write            ]    │
│ Callback port   [      ]                                   │
│                                                            │
│ Headers                                                    │
│ [ X-Team    ] [ platform        ] [ ] Secret        ✕      │
│ [ X-Api-Key ] [                 ] [x] Secret  Set on this  │
│                                       computer · Clear  ✕  │
│ [ + Add header ]                                           │
│                                                            │
│                                  [ Cancel ]  [ Save ]      │
└────────────────────────────────────────────────────────────┘
```

```
┌─ Trust this server? ───────────────────────────────────────┐
│ This project runs this command on your computer:           │
│                                                            │
│   npx -y docs-mcp-server                                   │
│   Working folder: (project root)                           │
│   Environment: DOCS_REGION=eu                              │
│   Secret environment: DOCS_API_KEY                         │
│                                                            │
│                         [ Cancel ]  [ Trust and start ]    │
└────────────────────────────────────────────────────────────┘
```

Layout notes:
- The status chip uses the colour of the state: green for ready, amber for a state that needs the author, red for failed, grey for off.
- The test result and the failure line sit directly under their row, never at the top of the section.
- The dialog uses the wide modal layout and scrolls its body when it is taller than the window.

## UI contract boundary
- **Owned by the UI**: the list and its rows, the status chip text and colour, the failure texts, the dialog and its field checks, the slug of the ID, the trust dialog, the remove confirmation, the tools disclosure, the test result display, and the order of the save steps.
- **Delegated to backend (abstract)**: `list_mcp_servers`, `save_mcp_server`, `remove_mcp_server`, `set_mcp_server_secret`, `clear_mcp_server_secret`, `trust_mcp_server`, `test_mcp_server`, `reconnect_mcp_server`, `start_mcp_sign_in`, `cancel_mcp_sign_in`, `sign_out_mcp_server`, and the event `mcp-server-status-changed`, each owned by `../core/MCS-mcp-server-connections.md`.

## Non-functional requirements
- Each control has an accessible name, and each is operable from the keyboard.
- A secret value never shows after it is stored, and the UI never receives one from the backend.

# MCP tool

**Spec code:** `MCT`

## Intent
An **MCP tool** is a tool of one of the project's MCP servers, lent to a conversation agent for one turn. The server gives its name, its description, and its parameter schema, and the server does the work. The application translates each part into the form of a `rig` tool definition, sends the call to the server, and translates the result into text that the model reads. This lets an agent reach the systems the project already uses, with the same tool loop it uses for the tools of the application (per `../ai/CVL-conversation-loop.md` CVL-FR-TJZI). This is the fourth tool class of `TLC-tool-conventions.md` (TLC-FR-WOBX). The server records, the connections, and the sign-in are `../core/MCS-mcp-server-connections.md`'s. Out of scope: MCP resources and prompts, and a confirmation of the author before a call.

## Functional requirements

### The offered set
1. **MCT-FR-GPCO** A turn is offered one MCP tool for each tool that `mcp_tools_now` returns for the project when the turn's plan is built (per `../core/MCS-mcp-server-connections.md` MCS-FR-ENII). A tool in its server's `disabledTools` is not offered.
2. **MCT-FR-RDUX** The MCP tools of a turn are fixed when its plan is built. A change to a server's tools during the turn does not add, remove, or change a definition of that turn.
   - *Why:* Each call of a turn presents the same tool definitions, so that the provider cache holds (per `../ai/CVL-conversation-loop.md` CVL-FR-23).
3. **MCT-FR-JEFK** A turn is offered at most 100 MCP tools: the tools of the servers in file order, and the tools of one server in the order the server listed them. The tools after the hundredth are not offered, and the loop logs one `WARN` record that names each server id that lost tools and the count it lost.
   - *Why:* Some providers refuse a request with more than 128 tools, and the application's own tools take part of that limit.

### The definition
4. **MCT-FR-TZMA** The name of an MCP tool is `mcp__<server id>__<tool name>`. Each character of the tool name that is not in `[A-Za-z0-9_-]` becomes `_`.
5. **MCT-FR-VDFB** A name longer than 64 characters, or a name that another MCP tool of the turn has already, is cut to 55 characters and ends with `_` and the first 8 hexadecimal characters of the SHA-256 of `<server id>/<tool name>`.
6. **MCT-FR-EXQO** The description of an MCP tool is `[<server name>] ` and then the description the server gave, or the tool name where the server gave none. A description longer than 2,000 characters is cut there.
7. **MCT-FR-GNUX** The parameter schema of an MCP tool is the `inputSchema` the server gave, with its top-level `$schema` key removed. A schema with no `type` gets `"type": "object"`. A tool list in which an `inputSchema` is not a JSON object does not decode, so its server is `failed` with `protocol_error` (per `../core/MCS-mcp-server-connections.md` MCS-FR-JNXV) and offers no tool.

### The call
8. **MCT-FR-OSXG** A call sends `tools/call` to the server with the tool name the server gave and the arguments object the model composed, unchanged, through `call_mcp_tool` (per `../core/MCS-mcp-server-connections.md` MCS-FR-LTTM). Arguments that are not a JSON object are refused before the call (per `TLC-tool-conventions.md` TLC-FR-SPRR).
9. **MCT-FR-NJNW** The deadline of a call is the time that remains under the whole-turn deadline (per `../ai/CVL-conversation-loop.md` CVL-FR-16).
10. **MCT-FR-YSVS** A call never ends the turn. Its result goes into the exchange, and the loop continues (per `../ai/CVL-conversation-loop.md` CVL-FR-12).
11. **MCT-FR-WVNU** The application asks the author for no confirmation before a call. The side effects of a call are what the server does, and the application declares none of them.

### The result
12. **MCT-FR-NMXZ** The text of a result is its content blocks in order, separated by one blank line. A text block gives its text. An embedded resource with text gives that text. A resource link gives `[Resource: <name> <uri>]`.
13. **MCT-FR-LZUH** An image block or an audio block gives the line `[The tool returned <image|audio> content (<MIME type>). It is not shown here.]`. The application sends no binary content of a result to the model.
14. **MCT-FR-KUIJ** A result with no content block but with `structuredContent` gives that structured content as JSON text. A result with neither gives `The tool returned no content.`
15. **MCT-FR-ASCH** A result text longer than 50,000 characters is cut there, and the line `[The result was cut at 50,000 characters.]` follows it.
16. **MCT-FR-QDEE** A result with `isError: true` is a refusal of kind `Other`, marked retryable. Its message is the result text of MCT-FR-NMXZ, cut at 2,000 characters.
17. **MCT-FR-BMFR** A JSON-RPC error from the server is a refusal that holds the error message the server gave, cut at 500 characters. The code `-32602` is a refusal of kind `InvalidArgs`, marked retryable. Every other code is kind `Other`, not retryable.
18. **MCT-FR-BZVS** A server that is not connected gives the refusal `The <server name> server is not connected now. Answer without its tools.`, of kind `Other`, not retryable.
19. **MCT-FR-QWPU** A call whose deadline passes gives the refusal `The <server name> server did not answer in time.`, of kind `Timeout`, not retryable.

### Logging and secrets
20. **MCT-FR-RIWQ** Each call logs as `TLC-tool-conventions.md` TLC-FR-14 states. The records name the model-facing tool name, the server id, the outcome, and the duration. They name no argument, no result, and no error message of the server.
21. **MCT-FR-CFDC** No secret value, header value, or token is in the name, the description, the parameter schema, or a refusal message that the application composes for an MCP tool (per `TLC-tool-conventions.md` TLC-FR-18).

## Contract surface
This spec registers no Tauri command and emits no Tauri event. One MCP tool is one `rig::tool::DynamicTool`:

```
DynamicTool::new(
  name,          // MCT-FR-TZMA, MCT-FR-VDFB
  description,   // MCT-FR-EXQO
  parameters,    // MCT-FR-GNUX
  call,          // MCT-FR-OSXG: arguments -> call_mcp_tool -> text (MCT-FR-NMXZ) or refusal
)

mcp_conversation_tools(project_key) -> DynamicTool[]   // MCT-FR-GPCO, MCT-FR-JEFK
```

The call holds the server id and the tool name the server gave. It holds no session and no secret: the session belongs to `../core/MCS-mcp-server-connections.md`.

## Non-functional requirements
- The description and the schema of an MCP tool come from a server the author chose. `TLC-tool-conventions.md` TLC-FR-05 does not govern them.
- A server's result text is material, not instruction. It enters the exchange as a tool result alone (per `../ai/CVL-conversation-loop.md` CVL-FR-07).

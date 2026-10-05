## Intent

The IDE, while open, must communicate with a remote thin client. The remote client can control the development process, including conversations with agents, draft discussions, draft creation and graduation management.

V1 provides one configured relay server instance. The relay is a blind transport for registration and application traffic. It keeps only in-memory connection and routing state and has no durable application state. High availability, multiple relay instances, inter-relay forwarding, shared routing state, and session migration are out of scope for v1.

### Transport authentication

Except for `GET /v1/health`, every HTTP request and WebSocket upgrade requires `Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>`. The relay validates this token before route or frame processing. It is the only transport-authentication mechanism in v1; token acquisition and storage by the mobile client are out of scope. The token authenticates transport access only. Client handles, worker ownership, and end-to-end client authentication remain separate session and application-protocol checks.


## Ontology

### Registration

A **registration** is the one-time exchange that binds a remote client to a worker and produces an opaque mobile connection handle. The QR payload contains opaque routing and registration data, including `registration_id`, target `instance_id`, relay URL, IDE public key, exposed-project information, challenge, and expiry.

The relay may retain short-lived opaque routing state and may use only `registration_id` and `instance_id` for routing. It must not inspect or validate the remaining QR or registration fields.

### Mobile connection handle

A **mobile connection handle** is an opaque relay-assigned identifier created for a successful registration route. It remains valid for reconnects until the IDE revokes or expires it, except that a protocol reset permanently invalidates it. The relay checks only handle existence, worker ownership, and lifecycle state.

### Application message

An **application message** is an opaque, end-to-end encrypted and authenticated payload exchanged between a worker and its clients. The relay broadcasts worker-originated application messages to every authenticated client of the worker and forwards each client-originated message to the worker only. The relay does not interpret project selection or any other application operation, including user identity, ownership, membership, or permissions.

### Capability

A **capability** is a stable identifier advertised by the unauthenticated health response. V1 advertises exactly `remote_session` and `websocket`. Each capability has a dedicated feature specification defining its contract. Unknown capabilities must not be treated as supported.

## Endpoint contract

### `GET /v1/health`

Returns a JSON object containing:

```json
{
  "version": "<relay-version>",
  "capabilities": ["remote_session", "websocket"]
}
```

The endpoint is unauthenticated. It must not expose keys, QR contents, device registrations, application payloads, user identity, project ownership, or connection state. `BMS-backend-microservice.md` must be updated from its current version-only health contract.

### `GET /v1/projects`

Returns the currently exposed projects available to an authenticated remote client. The request must identify a registered client with its valid opaque handle. It must not expose a global unauthenticated project list.

The response must identify the worker and expose each worker project's opaque `project_id` and human-compatible `display_name`. The exact authenticated-handle transport, response schema, stale-data rules, and behaviour after relay restart must be defined in the `remote_session` and `websocket` specifications without allowing the endpoint to expose private keys, application payloads, or application-level user and ownership data.

### `POST /v1/projects`

The worker may publish its complete current exposed-project list through an authenticated HTTP request if the final endpoint design requires HTTP project announcements. The request must identify the worker using its authenticated worker connection and carry the same project descriptor rules as `worker_hello` and `projects_changed`.

The dedicated specifications must decide whether this POST is required or whether worker WebSocket frames are the only project-announcement mechanism. The two mechanisms must not create conflicting project state.

### `GET /v1/worker` WebSocket upgrade

The worker WebSocket follows the common transport-authentication rule above. After the authenticated upgrade, the first frame is `worker_hello`. It identifies the worker with `instance_id` and announces its complete exposed-project list. The relay registers the worker route only after a valid envelope. Duplicate project identifiers in one announcement are rejected.

The worker may send `projects_changed`, `registration_result`, `application_message`, and typed delivery failures. A replacement worker causes the relay to close the old worker connection, notify all affected clients, and provide the replacement `instance_id`. The relay does not buffer messages during replacement. Clients reconnect and record the current `instance_id`.

### `GET /v1/client` WebSocket upgrade

The client WebSocket follows the common transport-authentication rule above. The remote client sends `client_attach` with the opaque handle and target `instance_id`. Before IDE authentication, only `client_attach`, `client_authenticate`, `client_authentication_result`, `protocol_reset`, and transport-error frames are accepted.

The IDE returns `client_authentication_result`. The relay may use only the result status to update routing state; it must not inspect the device proof, key-agreement payload, or encrypted data.

After acceptance, the relay forwards client-originated application messages to the worker and broadcasts worker-originated application messages to all authenticated clients of that worker.

### `GET /v1/registration` WebSocket upgrade

The mobile client opens the registration WebSocket and starts registration with `registration_start`, containing opaque `registration_id`, target `instance_id`, and an opaque payload. The relay routes the exchange to the matching worker. If the worker or registration route is absent, it returns a typed transport failure and closes the registration connection.

The relay assigns a fresh opaque handle and sends `registration_bound`. It forwards `registration_payload` frames without inspection or modification. The IDE sends exactly one terminal `registration_result`. On acceptance, the handle becomes eligible for `/v1/client`; on rejection, the handle is invalidated and the connection closes.

The relay never accepts a registration frame as a worker, client, or application frame, and never accepts a worker or client frame as a registration frame.

## Common frame contract

Every WebSocket message is exactly one UTF-8 JSON object. Trailing JSON values, unknown required fields, unsupported versions, invalid shapes, and endpoint-protocol mismatches are rejected.

```json
{
  "protocol": "worker | client | registration",
  "version": 1,
  "type": "protocol-specific-frame-type",
  "request_id": "opaque-request-id",
  "body": {}
}
```

`protocol` must match the endpoint. `version` is the integer protocol version. `type` is a stable identifier. `request_id` is required for request/response frames and absent for notifications; responses copy the request identifier they answer. Binary values, keys, proofs, encrypted payloads, and other opaque protocol data use unpadded base64url strings.

The relay may read only envelope fields and explicitly defined opaque routing fields. It must not parse, validate, decrypt, authorize, or rewrite end-to-end payloads.

## Required v1 frame types

The dedicated specifications must define complete schemas and state transitions for these frames:

- Worker: `worker_hello`, `projects_changed`, `registration_result`, `application_message`, `delivery_failed`.
- Client: `client_attach`, `client_authenticate`, `client_authentication_result`, `application_message`, `protocol_reset`, `worker_replaced`.
- Registration: `registration_start`, `registration_bound`, `registration_payload`, `registration_result`, `registration_failed`.
- Relay failures and close reasons: `unsupported_version`, `protocol_mismatch`, `invalid_frame_shape`, `missing_worker_authentication`, `invalid_worker_authentication`, `worker_not_found`, `registration_not_found`, `handle_not_found`, `handle_expired`, `handle_revoked`, `handle_reset`, `worker_replaced`, `delivery_failed`, `relay_restarted`, and `protocol_reset`.

A relay-generated failure identifies the affected `request_id` when one exists, closes the affected connection after an unrecoverable protocol error, and never reports delivery success for a frame it did not deliver.

## Planned remote operations

The application protocol carried inside encrypted messages must later support:

- creating, editing, archiving, and deleting drafts;
- creating folders and moving drafts;
- discussions for drafts, notes, and other entities;
- starting, observing, pausing, and discarding graduations, including streamed logs;
- starting, merging, and closing work streams;
- receiving and answering user escalations.

These operations are not part of the relay's routing or authorization logic. User identity, project ownership, collaboration membership, and operation permissions are also not part of the relay contract.

For example, creating a draft is carried inside an authenticated `application_message`: the client sends an encrypted application request to the worker, the worker routes it to the IDE/application service, and the IDE returns an encrypted application response. The application protocol must define operation names, request and response schemas, correlation and error semantics, authorization results, and streamed or incremental events where required. Relay frame types must not be added for individual application operations; `application_message` remains opaque to the relay.


## Required specifications and project changes

Create dedicated feature specifications for `remote_session` and `websocket`. They must define the user-visible and backend contracts for registration, device identity, authenticated worker and client sessions, project discovery, worker and project exposure, shared project selection, replacement, reconnect, relay restart, message failure, protocol reset, and end-to-end encryption/authentication.

The specifications must state that `User` is an application-level entity. They must define any user, ownership, collaboration, or permission data only in the encrypted application protocol or in the IDE/application specifications; they must not add relay-side user storage or authorization requirements in v1.

Update `specifications/server/BMS-backend-microservice.md` to permit the health response with `version` and `capabilities`, authenticated HTTP requests and WebSocket upgrades, `/v1/worker`, `/v1/client`, `/v1/registration`, and any authenticated project-discovery endpoint selected by the dedicated specifications. Update `specifications/server/SRB-server-relay-boundary.md` to use `SYNTHESIS_SERVER_TOKEN` for every worker, client, and registration transport except health, and to remove its conflicting relay-side administrator User and Device authentication/state requirements. Tests and documentation must cover capability identifiers, endpoint boundaries, frame-protocol separation, bearer-token authentication, route cleanup, delivery failures, and restart behaviour.

Update `specifications/core/GSS-global-settings-storage.md` and `specifications/ui/GLS-global-settings.md` with a Remote connectivity section. It must define the user-global relay endpoint, its validation states and transitions, Verify and retry behaviour, and typed validation failures. The UI must not display, edit, persist, or validate `SYNTHESIS_SERVER_TOKEN` or private keys. Token provisioning, storage, rotation, and injection for the IDE, relay, and mobile client remain runtime-configuration concerns outside this feature.

The new specifications must cross-reference the updated BMS health contract and must not duplicate unrelated server-foundation requirements.

## Out of scope

- Mobile application implementation and UI.
- Relay-side user authentication, user storage, project ownership, collaboration membership, or application authorization.
- Relay-side inspection, validation, decryption, or authorization of application payloads.
- High availability, multiple relay instances, inter-relay forwarding, shared coordination storage, redirect-based routing, and session migration.
- Treating worker, client, and registration traffic as one shared application protocol.
- Defining application-operation semantics inside the relay specification.
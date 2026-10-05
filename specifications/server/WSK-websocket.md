# Websocket

**Spec code:** `WSK`

## Intent
The WebSocket transport of the relay: three endpoints, one envelope, and one closed set of frames. A worker, a remote client, and a registering client each open their own endpoint, and each speaks its own protocol over it, so a frame of one protocol is never accepted as a frame of another. It exists so that the session rules of `RSN-remote-session.md` have exactly one wire form, and so that a relay that reads only an envelope can still refuse a malformed, mismatched, or unsupported frame before it routes anything. Out of scope: what a payload means, which is end-to-end and opaque; the session semantics themselves, which are `RSN-remote-session.md`'s; the routing state, which is `SRB-server-relay-boundary.md`'s; TLS termination; and any application operation, for which this specification defines no frame type.

## Functional requirements

### The capability and the endpoints
1. **WSK-FR-PLQD** The relay advertises the capability identifier `websocket` in the health response of `BMS-backend-microservice.md` BMS-FR-11. This specification is the whole contract of that identifier.
2. **WSK-FR-KTRB** `GET /v1/worker` upgrades to the worker protocol, `GET /v1/client` to the client protocol, and `GET /v1/registration` to the registration protocol. The relay opens no other WebSocket route.
3. **WSK-FR-BJTN** The registration endpoint carries the registration exchange of `RSN-remote-session.md` RSN-FR-LQAF, and carries no worker frame, no client frame, and no application message.
4. **WSK-FR-HGVU** Every upgrade carries `Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>`. The relay validates the token before it accepts the upgrade, and answers `401 Unauthorized` with the `unauthenticated` error code without upgrading when the header is absent, of another scheme, or wrong.
5. **WSK-FR-NAXC** A request to one of the three paths that is not a WebSocket upgrade is answered `400 Bad Request`, and the relay serves no other method on them.
6. **WSK-FR-ZWOE** The relay never accepts a registration frame as a worker, client, or application frame, and never accepts a worker or client frame as a registration frame.

### The envelope
7. **WSK-FR-DMJT** Every WebSocket message is exactly one UTF-8 JSON object. A binary message, a message that is not valid UTF-8, a message that is not one JSON object, and a message that carries a trailing JSON value are each refused with `invalid_frame_shape`.
8. **WSK-FR-VQPH** The envelope holds `protocol`, `version`, `type`, an optional `request_id`, and `body`. A member the envelope does not define is refused with `invalid_frame_shape`.
9. **WSK-FR-CLYA** `protocol` is `worker`, `client`, or `registration`, and must equal the protocol of the endpoint the frame arrived on. Another value is refused with `protocol_mismatch`.
10. **WSK-FR-RUKN** `version` is the integer protocol version, and V1 accepts `1` alone. Another integer, and a value that is not an integer, are each refused with `unsupported_version`.
11. **WSK-FR-EBTS** `type` is a stable identifier from the frame table of the frame's protocol. A type outside that table is refused with `invalid_frame_shape`.
12. **WSK-FR-WJIC** `request_id` is an opaque string, is required on a request frame and on the response that answers it, and is absent on a notification. A response copies the `request_id` of the request it answers byte for byte.
13. **WSK-FR-QOZF** `body` is a JSON object. A body that omits a member the frame type requires, or that carries a member the frame type does not define, is refused with `invalid_frame_shape`.
14. **WSK-FR-AGVX** A binary value, a key, a proof, an encrypted payload, and every other opaque protocol datum are carried as unpadded base64url strings. A value that is not unpadded base64url where the frame type requires one is refused with `invalid_frame_shape`.
15. **WSK-FR-SYNB** The relay reads the envelope members and the opaque routing members the frame tables name, and nothing else. It forwards a body it routes unchanged, and stamps the destination endpoint's `protocol` on the envelope it delivers.

### The worker protocol
16. **WSK-FR-TDKM** The first frame on the worker endpoint is `worker_hello`. Any other first frame is refused with `missing_worker_authentication` and closes the connection.
17. **WSK-FR-FEHL** `worker_hello` is a request whose body holds `instance_id`, `projects`, and `selected_project_id`. The relay registers the worker route only after the whole envelope and body are valid, and answers `worker_ready` with the same `request_id`.
18. **WSK-FR-XRVO** An `instance_id` that is absent, empty, or longer than 200 characters is refused with `invalid_worker_authentication`, and no worker route is registered.
19. **WSK-FR-JBQZ** `projects_changed` is a notification whose body holds `projects` and `selected_project_id`, and it replaces the whole exposed set of that worker (per `RSN-remote-session.md` RSN-FR-CQEF).
20. **WSK-FR-MZQD** `handle_invalidated` is a worker notification whose body holds `handle` and a `reason` of `handle_revoked` or `handle_expired`. The relay applies the reason to that handle alone (per `RSN-remote-session.md` RSN-FR-DWLS).
21. **WSK-FR-UPCG** `registration_result` is a worker notification whose body holds `registration_id`, a `status` of `accepted` or `rejected`, and an optional opaque `payload`. It is terminal for that registration.
22. **WSK-FR-ONWT** `client_authentication_result` is a worker notification whose body holds `handle`, a `status` of `accepted` or `rejected`, and an optional opaque `payload`. The relay reads `status` alone.
23. **WSK-FR-IKVE** `application_message` on the worker endpoint is a notification whose body holds one opaque `payload`, and the relay broadcasts it to every authenticated client of that worker.
24. **WSK-FR-LSAD** `delivery_failed` from a worker is a notification whose body holds `handle`, a `reason`, and the `request_id` of the frame it answers when that frame carried one. The relay routes it to that client and reads nothing else in it.
25. **WSK-FR-GRTY** The worker endpoint delivers `registration_start`, `registration_payload`, `client_attach`, `client_authenticate`, and `application_message` to the worker, each stamped with the `worker` protocol and each naming the `handle` the frame belongs to.

### The client protocol
26. **WSK-FR-YHBP** `client_attach` is a client request whose body holds `handle` and `instance_id`. The relay answers `client_attached` with the same `request_id`, holding the `instance_id` and the authentication state of the connection.
27. **WSK-FR-EQRV** Before the connection is authenticated the relay accepts `client_attach`, `client_authenticate`, `client_authentication_result`, `protocol_reset`, and transport-error frames alone. Any other frame is refused with `invalid_frame_shape` and closes the connection.
28. **WSK-FR-CJWA** `client_authenticate` is a client notification whose body holds one opaque `payload`, which the relay forwards to the worker of the attached route without reading it.
29. **WSK-FR-TMOB** `client_authentication_result` reaches the client with the `status` and the optional `payload` the worker sent. An accepted status authenticates that connection, and a rejected status closes it.
30. **WSK-FR-VDNQ** `application_message` on the client endpoint is a notification whose body holds one opaque `payload`, and the relay forwards it to the worker of the attached route alone.
31. **WSK-FR-ZUFK** `worker_replaced` is a relay notification to a client whose body holds the `instance_id` the client attaches to next. The relay sends it to every client of a route that a replacement detached.
32. **WSK-FR-ARJP** `protocol_reset` is a notification either the client or the relay sends. The relay invalidates the handle, drops the routing state, and closes the connection with the reason `protocol_reset`.

### The registration protocol
33. **WSK-FR-HNSC** `registration_start` is a registration request whose body holds `registration_id`, `instance_id`, and one opaque `payload`. It is the first frame of the endpoint, and any other first frame is refused with `invalid_frame_shape`.
34. **WSK-FR-QBLW** `registration_bound` answers `registration_start` with the same `request_id` and a body holding the fresh opaque `handle` the relay assigned.
35. **WSK-FR-FXTA** `registration_payload` is a notification whose body holds one opaque `payload`. The relay forwards it between the mobile client and the worker without inspection or modification.
36. **WSK-FR-DPGI** `registration_result` reaches the registration client with the `status` and the optional `payload` the worker sent, and the relay closes the connection after it.
37. **WSK-FR-KOEV** `registration_failed` is a relay notification whose body holds a `reason` and the `request_id` of the frame it answers when that frame carried one. It closes the registration connection.

### Failures and close reasons
38. **WSK-FR-BWZT** The reason vocabulary is `unsupported_version`, `protocol_mismatch`, `invalid_frame_shape`, `missing_worker_authentication`, `invalid_worker_authentication`, `worker_not_found`, `registration_not_found`, `handle_not_found`, `handle_expired`, `handle_revoked`, `handle_reset`, `worker_replaced`, `delivery_failed`, `relay_restarted`, and `protocol_reset`. No other reason is sent.
39. **WSK-FR-NQXD** A relay-generated failure on the worker and the client endpoints is a `relay_failure` frame whose body holds a `reason` from WSK-FR-BWZT and the `request_id` of the frame it answers when that frame carried one. On the registration endpoint it is `registration_failed`.
40. **WSK-FR-OGLM** The relay closes the affected connection after an unrecoverable protocol error, with the WebSocket close code `1008` and the reason of WSK-FR-BWZT as the close reason. A graceful shutdown closes with `relay_restarted`.
41. **WSK-FR-RCUY** The relay reports delivery success for no frame it did not deliver. A frame it cannot route is answered with a failure naming `delivery_failed` or the more exact reason it holds (per `RSN-remote-session.md` RSN-FR-HZTV).
42. **WSK-FR-SEBH** A failure the relay answers with names no payload, no key, no proof, no token, and no handle value. It names the reason and the `request_id` alone.
43. **WSK-FR-WKAI** Dropping a connection drops every route entry that named it, so a later frame for a dropped worker is refused with `worker_not_found` and a later frame for a dropped client is refused with `handle_not_found`.

### Limits and ordering
44. **WSK-FR-WVLC** A frame larger than 1 MiB is refused with `invalid_frame_shape` and closes the connection, so one client cannot make the relay hold an unbounded message.
45. **WSK-FR-TPJS** The relay forwards the frames of one connection in the order it received them, and reorders no frame. It orders no frame against a frame of another connection.
46. **WSK-FR-GZDU** The relay buffers no frame for a route that is absent, and holds no queue for a connection that is closed (per `RSN-remote-session.md` RSN-FR-BQZL).
47. **WSK-FR-YMOR** The relay answers a WebSocket ping with a pong and sends its own ping on an idle connection, so a connection that no longer carries traffic is still found to be closed and its routes are dropped.

## Contract surface
This specification owns the WebSocket endpoints of the `synthesis-server` crate, the envelope, and the frame tables. It exposes no Tauri command, is reachable from neither `src/**` nor `src-tauri/**`, and defines no application operation.

### The modules
```text
server/src/adapters/relay/ws/mod.rs          the three upgrade handlers
server/src/adapters/relay/ws/envelope.rs     the envelope and its validation
server/src/adapters/relay/ws/worker.rs       the worker protocol
server/src/adapters/relay/ws/client.rs       the client protocol
server/src/adapters/relay/ws/registration.rs the registration protocol
```

### The WebSocket surface
```text
GET /v1/worker        upgrade -> the worker protocol
GET /v1/client        upgrade -> the client protocol
GET /v1/registration  upgrade -> the registration protocol
```

Each upgrade carries `Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>` (WSK-FR-HGVU). `api/openapi.yaml` describes the three upgrades beside the other relay routes (per `SAS-server-application-service.md` SAS-FR-JOAX).

### The envelope
```json
{
  "protocol": "worker | client | registration",
  "version": 1,
  "type": "protocol-specific-frame-type",
  "request_id": "opaque-request-id",
  "body": {}
}
```

### The worker frames
| Type | Direction | Kind | Body |
| --- | --- | --- | --- |
| `worker_hello` | worker → relay | request | `instance_id`, `projects`, `selected_project_id` |
| `worker_ready` | relay → worker | response | `instance_id`, `connection_id` |
| `projects_changed` | worker → relay | notification | `projects`, `selected_project_id` |
| `handle_invalidated` | worker → relay | notification | `handle`, `reason` |
| `registration_start` | relay → worker | notification | `registration_id`, `handle`, `payload` |
| `registration_payload` | relay ↔ worker | notification | `handle`, `payload` |
| `registration_result` | worker → relay | notification | `registration_id`, `status`, `payload?` |
| `client_attach` | relay → worker | notification | `handle` |
| `client_authenticate` | relay → worker | notification | `handle`, `payload` |
| `client_authentication_result` | worker → relay | notification | `handle`, `status`, `payload?` |
| `application_message` | relay ↔ worker | notification | `handle` inbound, `payload` |
| `delivery_failed` | relay ↔ worker | notification | `handle`, `reason`, `request_id?` |
| `relay_failure` | relay → worker | notification | `reason`, `request_id?` |

### The client frames
| Type | Direction | Kind | Body |
| --- | --- | --- | --- |
| `client_attach` | client → relay | request | `handle`, `instance_id` |
| `client_attached` | relay → client | response | `instance_id`, `authenticated` |
| `client_authenticate` | client → relay | notification | `payload` |
| `client_authentication_result` | relay → client | notification | `status`, `payload?` |
| `application_message` | client ↔ relay | notification | `payload` |
| `worker_replaced` | relay → client | notification | `instance_id` |
| `protocol_reset` | client ↔ relay | notification | `reason` |
| `delivery_failed` | relay → client | notification | `reason`, `request_id?` |
| `relay_failure` | relay → client | notification | `reason`, `request_id?` |

### The registration frames
| Type | Direction | Kind | Body |
| --- | --- | --- | --- |
| `registration_start` | client → relay | request | `registration_id`, `instance_id`, `payload` |
| `registration_bound` | relay → client | response | `handle` |
| `registration_payload` | client ↔ relay | notification | `payload` |
| `registration_result` | relay → client | notification | `status`, `payload?` |
| `registration_failed` | relay → client | notification | `reason`, `request_id?` |

### The project descriptor
```text
{ "project_id": "opaque, 1..=200 characters",
  "display_name": "1..=200 characters" }
```

An announcement carries at most 500 descriptors, and `selected_project_id` is one of their `project_id` values or `null`.

### The reasons
```text
unsupported_version  protocol_mismatch  invalid_frame_shape
missing_worker_authentication  invalid_worker_authentication
worker_not_found  registration_not_found  handle_not_found
handle_expired  handle_revoked  handle_reset  worker_replaced
delivery_failed  relay_restarted  protocol_reset
```

## Non-functional requirements
- The transport holds no state of its own: every route it reads and writes belongs to the port of `SRB-server-relay-boundary.md` SRB-FR-DYAE.
- The endpoints are plain WebSocket over plain HTTP. Whatever terminates TLS is outside the image, exactly as it is for the HTTP surface.
- One relay of V1 carries tens of connections rather than thousands, so a frame is validated and routed without a pool, a batch, or a scheduler of its own.
- Frame validation is a pure function of the frame bytes and the protocol of the endpoint, so it is tested without a socket.
- No frame body, no payload, and no handle value reaches a log record, a metric label, or an error body.

# Server relay boundary

**Spec code:** `SRB`

## Intent
The transport half of the Synthesis server. It is the boundary at which an IDE worker and a remote client meet: it authenticates a worker connection, holds which worker exposes which projects, holds which client connection is attached to which worker, and routes traffic between the two. It knows nothing about what it carries — every application message between a worker and a client is encrypted end to end, and the relay reads none of it. This specification owns the boundary, the routing state, and the ports through which a transport adapter drives them, so that the WebSocket transport reaches a relay whose state model is already decided and already tested. Out of scope: the WebSocket routes, their upgrade handling, the frame schemas, message limits, and ordering, which are `WSK-websocket.md`'s; the registration and session protocol, reconnect, and handle lifecycle, which are `RSN-remote-session.md`'s; and relay-side user storage, project ownership, collaboration membership, and application authorization, which the relay holds in no form.

## Functional requirements

### The boundary
1. **SRB-FR-QMDT** Every worker, client, and registration transport of the relay requires the header `Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>`, the token of `SAS-server-application-service.md` SAS-FR-QJWD, and `GET /v1/health` is the one exception. A request or upgrade with no token, or with another token, is refused before any route is recorded and before any frame is read.
2. **SRB-FR-JEBM** The token authenticates transport access alone. It names no principal, the relay stores no credential and no user of its own, and the handle, worker-ownership, and end-to-end client checks stand beside it rather than behind it.
3. **SRB-FR-BXNA** The relay validates the transport envelope and the routing fields this specification names, and nothing else. It does not read, validate, decrypt, authorize, or rewrite a QR content, a key, a challenge, a proof, a registration payload, or an encrypted application payload.
4. **SRB-FR-TJEC** The relay evaluates no project permission. Every access-control decision belongs to the application service (`SAS-server-application-service.md` SAS-FR-TKPZ).
5. **SRB-FR-ZUPL** The relay holds its whole state in memory: connection ownership, worker routes, client attachments, opaque client handles, worker-owned selected projects, and replacement notices. It persists no device registration, no key, no QR content, no application payload, and no message queue.
6. **SRB-FR-WHRO** A restart or a process failure drops every connection and every route. The relay reports no session as active after a restart, and a client reconnects to the configured relay and repeats session establishment.
7. **SRB-FR-WLXF** V1 configures one relay instance. The relay serves many workers and many clients at once, and holds no knowledge of another relay instance.

### Workers
8. **SRB-FR-YCFA** A worker route is one running IDE application, identified by the `instance_id` the application generates in memory when it starts. The relay treats the identifier as opaque, never generates it, and never reuses one after its route is dropped.
9. **SRB-FR-DKPM** A worker route holds the `instance_id`, the connection identifier, the connection time, the projects the worker exposes, and the selected project of that worker.
10. **SRB-FR-LGQB** At most one worker route is active for one `instance_id`. A second connection with that identifier replaces the first: the older route is dropped, the newer route takes its place, and a replacement notice naming the older connection is held for the older connection to read.
11. **SRB-FR-EASV** A worker announcement carries, for each exposed project, an opaque `project_id` and a `display_name`. The relay routes by `project_id` alone and never routes, matches, or compares by `display_name`.
12. **SRB-FR-NIUT** A later announcement replaces the whole exposed set of that worker. A project that the newer announcement omits stops being exposed at once.
13. **SRB-FR-OPBZ** Each worker route holds its own selected project, so two workers hold two selected projects at the same time and neither observes the other's.
14. **SRB-FR-FVJD** A selected project that is not in the exposed set of that worker is refused, and the selected project of the route does not change.

### Clients
15. **SRB-FR-XSAG** A client connection is identified by an opaque handle the relay assigns at registration. The handle is unique among the handles the process has assigned, and is never assigned again after the connection it names is closed or reset.
16. **SRB-FR-MTHQ** A client connection is attached to exactly one worker route at a time, and the relay refuses an attachment to an `instance_id` that holds no active route.
17. **SRB-FR-RQOD** One handle may hold several client connections at once, and several handles may be attached to one worker route. The attachment and the authentication state are recorded against the handle rather than against a socket, and the relay records no owner for either.
18. **SRB-FR-CBWU** A client connection observes the selected project of the worker route it is attached to. The observation belongs to the connection; the selected project itself belongs to the worker route.
19. **SRB-FR-HZLE** The relay records whether a client connection is authenticated by its worker, and routes a connection that is not yet authenticated only for the pre-authentication frames `WSK-websocket.md` WSK-FR-EQRV defines. The relay authenticates no application message.
20. **SRB-FR-AVTK** A protocol failure of a client or of a worker resets that client connection: the relay invalidates the handle, detaches the connection, drops its routing state, and refuses every later frame that carries the handle. A new registration is needed, and the handle is never reused.
21. **SRB-FR-GJSP** Dropping a worker route detaches every client connection attached to it, and each detached connection needs a new attachment before it is routed again.
22. **SRB-FR-PWNK** A message a worker broadcasts is routed to every authenticated client connection attached to that worker route, and to no connection attached to another route.

### The ports and the adapter
23. **SRB-FR-DYAE** The routing state is reached through one inbound port with the operations of the contract surface, so the WebSocket adapter of `WSK-websocket.md` holds no state of its own.
24. **SRB-FR-VUCM** The routing state is safe to drive from many connections at once, and every operation of the port either applies whole or changes nothing.
25. **SRB-FR-SEKN** The relay serves two read-only introspection routes, `GET /v1/relay/workers` and `GET /v1/relay/workers/{instance_id}`, which report the routes and their client counts. They need the bearer token of SRB-FR-QMDT and report no handle content and no payload.
26. **SRB-FR-TOQF** The WebSocket routes and their upgrade handlers belong to `WSK-websocket.md` and are driven against the port of SRB-FR-DYAE. This specification opens none of them and holds no frame schema.
27. **SRB-FR-IZAB** A relay log record names the connection identifier, the `instance_id`, and the operation. It names no token, no handle, no key, and no payload.

## Contract surface
This specification owns the relay boundary of the `synthesis-server` crate: the routing state, its port, and the two introspection routes. The session rules and the project-discovery route are `RSN-remote-session.md`'s, and the WebSocket endpoints and the frames are `WSK-websocket.md`'s.

### The modules
```text
server/src/adapters/relay/**   the routing state, the port, the introspection routes
```

### The port
```text
register_worker(instance_id, connection_id, projects) -> WorkerRoute | Replaced(previous_connection_id)
announce_projects(instance_id, projects)              -> WorkerRoute
select_project(instance_id, project_id)               -> WorkerRoute
drop_worker(instance_id, connection_id)               -> detached client handles
register_client(registration_id, instance_id)         -> handle
attach_client(handle, instance_id)                    -> attachment
invalidate_handle(handle, reason)                     -> the closed client connections
authenticate_client(handle)                           -> attachment
observe_selected_project(handle)                      -> project_id | none
broadcast_targets(instance_id)                        -> the authenticated handles of that route
reset_client(handle)                                  -> the handle is invalidated
take_replacement_notice(connection_id)                -> notice | none
workers()                                             -> the active routes
```

### The HTTP surface
```text
GET /v1/relay/workers                  the active worker routes
GET /v1/relay/workers/{instance_id}    one worker route
```

Each answer names the `instance_id`, the connection time, the exposed projects, the selected project, and the number of attached client connections.

## Non-functional requirements
- The state is one lock over one set of maps. A worker route and a client attachment are cheap to add and cheap to drop, because a relay of V1 holds tens of them rather than thousands.
- The relay is transport only. Nothing in it reads an application payload, so a payload never reaches a log, a metric, or an error message.
- The routing state is lost with the process, which is the whole of its durability contract. A client's reconnect path is what recovers a session, not the relay's memory.
- The port is written so that a WebSocket adapter, a test, and a future transport all drive one state model.

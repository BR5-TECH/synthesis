# Remote session

**Spec code:** `RSN`

## Intent
The session half of the relay: how a remote client is bound to one running IDE, and what the relay holds about that binding while it lasts. An author who is away from the machine reaches the IDE from a phone, so the phone registers once against a QR code the IDE shows, receives an opaque connection handle, and then holds an authenticated session through which encrypted application traffic passes. This specification owns the `remote_session` capability: the registration exchange, the handle and its lifecycle, worker exposure and project discovery, worker replacement, reconnect, relay restart, delivery failure, protocol reset, and the end-to-end encryption the relay carries but never opens. The frames that express all of it, and the endpoints that carry them, are `WSK-websocket.md`'s. Out of scope: the mobile application and its interface; relay-side user authentication, user storage, project ownership, collaboration membership, and application authorization; any inspection, validation, decryption, or authorization of an application payload; high availability, several relay instances, inter-relay forwarding, shared coordination storage, and session migration; and the semantics of the application operations the encrypted payloads carry.

## Functional requirements

### The capability and its boundary
1. **RSN-FR-QHVA** The relay advertises the capability identifier `remote_session` in the health response of `BMS-backend-microservice.md` BMS-FR-11. This specification is the whole contract of that identifier.
2. **RSN-FR-VZKP** A client treats a capability identifier it does not know as unsupported, and reaches no endpoint of this specification when the health response omits `remote_session` or `websocket`.
3. **RSN-FR-XBRC** Every HTTP request and every WebSocket upgrade of this specification carries `Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>` and is refused before route or frame processing without it (per `SRB-server-relay-boundary.md` SRB-FR-QMDT). `GET /v1/health` is the one unauthenticated route.
4. **RSN-FR-TWQJ** The bearer token authenticates transport access alone. It names no principal, and it never stands in place of the handle check, the worker-ownership check, or the end-to-end client authentication of RSN-FR-GKZP.
5. **RSN-FR-MDLB** A `User` is an application-level entity of `SAS-server-application-service.md` SAS-FR-KDSM. The relay stores no user, no device, no ownership, no membership, and no permission, and answers no authorization question.
6. **RSN-FR-PJHN** User, ownership, collaboration, and permission data reach a remote client only inside an encrypted application message or through the routes of `SAS-server-application-service.md`. No frame and no relay route of this specification carries them in the clear.
7. **RSN-FR-CVUT** The relay reads the envelope fields and the opaque routing fields this specification names, and nothing else. It parses, validates, decrypts, authorizes, and rewrites no end-to-end payload (per `SRB-server-relay-boundary.md` SRB-FR-BXNA).

### Registration
8. **RSN-FR-LQAF** A registration is the one-time exchange that binds a remote client to one worker and produces one opaque mobile connection handle. It is held over the registration endpoint of `WSK-websocket.md` WSK-FR-BJTN and over no other.
9. **RSN-FR-ZHRD** The QR payload the IDE presents carries the `registration_id`, the target `instance_id`, the relay URL, the IDE public key, the exposed-project information, a challenge, and an expiry. The IDE produces it, and the mobile client reads it.
10. **RSN-FR-NGKW** The relay reads `registration_id` and `instance_id` alone, and uses them for routing alone. It does not read, validate, store, or expire the relay URL, the public key, the exposed-project information, the challenge, or the expiry.
11. **RSN-FR-BUXE** The relay routes a registration to the worker route that holds the target `instance_id`. When no such route is active it answers `worker_not_found`, and when the registration route it holds does not match it answers `registration_not_found`; each answer closes the registration connection.
12. **RSN-FR-YAOC** The relay retains registration state only while the registration connection is open, and drops it when that connection closes. It persists no registration and no QR content.
13. **RSN-FR-EIVS** The relay assigns one fresh opaque handle for a registration it routes, and reports it to the mobile client alone. It assigns the handle before it forwards the first payload, so the worker learns the handle the exchange belongs to.
14. **RSN-FR-RKMD** The relay forwards every registration payload between the mobile client and the worker unchanged, in the order it received them, and adds, removes, and rewrites nothing in them.
15. **RSN-FR-WOFT** The IDE proves the device identity and the challenge inside the registration payloads, and the relay verifies neither. The device key and the device proof reach no relay state and no relay log.
16. **RSN-FR-GPLZ** The IDE sends exactly one terminal registration result for one registration. A second result for that registration is refused, and the registration connection is closed.
17. **RSN-FR-HSNA** An accepted result makes the handle eligible for the client endpoint and closes the registration connection. A rejected result invalidates the handle at once and closes the registration connection; the handle is never eligible afterwards.

### The mobile connection handle
18. **RSN-FR-JXBQ** A mobile connection handle is an opaque relay-assigned identifier. It is unique among the handles the process has assigned, carries no readable structure, and is never assigned again (per `SRB-server-relay-boundary.md` SRB-FR-XSAG).
19. **RSN-FR-UDCM** A handle is bound to the one `instance_id` its registration named. The relay refuses a handle presented against another `instance_id` with `handle_not_found`, and never moves a handle to another worker route.
20. **RSN-FR-FKRE** A handle holds one lifecycle state: `pending` while the registration is unresolved, `active` after an accepted result, and `invalid` after it is revoked, expired, or reset. The relay checks handle existence, worker ownership, and lifecycle state, and nothing else.
21. **RSN-FR-ATYV** An `active` handle stays valid across any number of client reconnects. Closing a client connection returns the handle to the relay's use and invalidates nothing.
22. **RSN-FR-DWLS** The IDE invalidates a handle by naming it in the handle-invalidation frame of `WSK-websocket.md` WSK-FR-MZQD with the reason `handle_revoked` or `handle_expired`. The relay applies the reason it is given and computes no expiry of its own.
23. **RSN-FR-KVBO** Invalidating a handle closes every client connection that holds it, with the reason the IDE named. A later frame carrying that handle is refused with the same reason.
24. **RSN-FR-SGQX** A protocol reset invalidates a handle permanently with the reason `handle_reset`. A new registration is the only way the client is routed again, and the reset handle is never reused (per `SRB-server-relay-boundary.md` SRB-FR-AVTK).

### Worker sessions and project exposure
25. **RSN-FR-OZMC** A worker is one running IDE, identified by the `instance_id` it generates in memory when it starts (per `SRB-server-relay-boundary.md` SRB-FR-YCFA). The relay registers the worker route only after a valid first announcement.
26. **RSN-FR-BPTL** A worker announcement carries the worker's complete exposed-project list. Each project descriptor holds an opaque `project_id` and a human-compatible `display_name`, and the relay routes by `project_id` alone (per `SRB-server-relay-boundary.md` SRB-FR-EASV).
27. **RSN-FR-XNUH** Two descriptors with one `project_id` in one announcement are refused with `invalid_frame_shape`. The exposed set does not change, and a first announcement that is refused registers no worker route.
28. **RSN-FR-CQEF** A later announcement replaces the whole exposed set of that worker, so a project the newer announcement omits stops being exposed at once (per `SRB-server-relay-boundary.md` SRB-FR-NIUT).
29. **RSN-FR-VTKA** An announcement carries the worker's selected `project_id`, or none. The relay records it as opaque routing state, and a value that is not in the same announcement's exposed set is refused with `invalid_frame_shape`.
   - *Why:* The relay records the selection the worker states rather than interpreting a selection request, which is an application operation it never reads.
30. **RSN-FR-IRZW** Every client of one worker observes that worker's one selected project, and no client observes the selection of another worker (per `SRB-server-relay-boundary.md` SRB-FR-OPBZ, SRB-FR-CBWU).
31. **RSN-FR-YBGP** A worker announcement is the only mechanism by which the exposed-project list and the selected project reach the relay. The relay serves no route through which a worker publishes projects.

### Project discovery
32. **RSN-FR-MHDV** `GET /v1/relay/projects` reports the projects one worker exposes to one registered client. The request carries the bearer token of RSN-FR-XBRC, the client's handle in `X-Synthesis-Client-Handle`, and the target worker in `X-Synthesis-Instance-Id`.
33. **RSN-FR-QLEC** The response is one JSON object holding `instance_id`, `selected_project_id`, and `projects`, where each project holds `project_id` and `display_name` alone. It holds no key, no handle, no application payload, no user, and no ownership data.
34. **RSN-FR-ANWK** The endpoint reports the projects of the named worker alone. It serves no global list, and no request without a valid handle reaches any project data.
35. **RSN-FR-TFJU** A handle that no route holds is answered `404 Not Found` with `handle_not_found`; a handle that another worker owns is answered the same way; an invalidated handle is answered `403 Forbidden` with `handle_revoked`, `handle_expired`, or `handle_reset`; and an absent worker route is answered `404 Not Found` with `worker_not_found`.
36. **RSN-FR-GXWB** The endpoint reports the announcement the worker route holds when the request is served, and the relay caches nothing beyond it. The relay pushes no project update to a client, so a client that needs a newer list reads the endpoint again.
37. **RSN-FR-ZDPA** After a relay restart no handle and no worker route exists, so the endpoint answers `handle_not_found` until the worker announces again and the client registers again.

### Client sessions
38. **RSN-FR-NUAB** A client session starts when the client attaches its handle to the target `instance_id` over the client endpoint. The relay refuses an attachment to an `instance_id` that holds no active route (per `SRB-server-relay-boundary.md` SRB-FR-MTHQ).
39. **RSN-FR-GKZP** An attached client is not authenticated. The client sends its device proof and key-agreement payload, the relay forwards them to the worker, and the IDE returns the authentication result.
40. **RSN-FR-PXVC** The relay reads the result status alone. It does not read the device proof, the key-agreement payload, or any encrypted data in the result.
41. **RSN-FR-LOJD** Before the result is accepted, the relay routes the pre-authentication frames of `WSK-websocket.md` WSK-FR-EQRV alone, and refuses an application message from that connection.
42. **RSN-FR-WBHS** After the result is accepted, the relay forwards each client-originated application message to the worker alone, and broadcasts each worker-originated application message to every authenticated client of that worker (per `SRB-server-relay-boundary.md` SRB-FR-PWNK).
43. **RSN-FR-DRQK** A rejected authentication result leaves the connection unauthenticated and closes it. The handle stays as it was, so the client may attach again.
44. **RSN-FR-JAVE** Several client connections may hold one handle, and several handles may attach to one worker route, at the same time. The attachment and the authentication state belong to the handle, so every connection that holds one handle observes the same state (per `SRB-server-relay-boundary.md` SRB-FR-RQOD).

### Replacement, reconnect, and restart
45. **RSN-FR-EHUT** A second worker connection for one `instance_id` replaces the first: the relay closes the older worker connection with `worker_replaced` and puts the newer route in its place (per `SRB-server-relay-boundary.md` SRB-FR-LGQB).
46. **RSN-FR-SVMD** A replacement detaches every client connection of that route and notifies each one with the replacement notice, which names the `instance_id` the client attaches to next.
47. **RSN-FR-BQZL** The relay buffers no message during a replacement. A frame that arrives while no route is active is refused with `worker_not_found` rather than held.
48. **RSN-FR-FOMR** A notified client reconnects, records the `instance_id` the notice named, and attaches again with its handle. Its handle survives the replacement.
49. **RSN-FR-UKCA** Dropping a worker route detaches every client connection attached to it, and each detached connection attaches again before it is routed (per `SRB-server-relay-boundary.md` SRB-FR-GJSP).
50. **RSN-FR-XTPN** A relay restart drops every connection, every worker route, and every handle (per `SRB-server-relay-boundary.md` SRB-FR-WHRO). The relay reports no session as active afterwards.
51. **RSN-FR-CLGY** During a graceful shutdown the relay closes each open connection with `relay_restarted`, so a client distinguishes a stopping relay from a protocol failure of its own.
52. **RSN-FR-RWIB** After a restart a worker reconnects and announces again, and a client whose handle the restart dropped holds a new registration before it is routed again.

### Failure and reset
53. **RSN-FR-HZTV** The relay reports delivery success for no frame it did not deliver. A frame it cannot forward is answered to its sender with a delivery failure that names the affected `request_id` when the frame carried one.
54. **RSN-FR-MFCX** The worker reports a delivery failure of its own when it cannot deliver a forwarded frame to the IDE application. The relay routes that failure to the client the frame came from and reads nothing else in it.
55. **RSN-FR-OKPD** A protocol failure of a client or of a worker resets the affected client session: the relay invalidates the handle, detaches the connection, drops its routing state, and closes the connection with `protocol_reset`.
56. **RSN-FR-ADXL** A reset is permanent for that handle. Every later frame carrying it is refused with `handle_reset`, whatever the connection it arrives on.

### The application protocol
57. **RSN-FR-VJIQ** An application message is opaque to the relay. It is encrypted and authenticated end to end between the mobile client and the IDE, and the relay holds no key and performs no cryptographic operation on it.
58. **RSN-FR-TGSE** The relay adds no frame type for an application operation. Creating a draft, holding a discussion, starting a graduation, and every other remote operation travel inside an application message.
59. **RSN-FR-WPQA** The application protocol carried inside application messages supports creating, editing, archiving, and deleting drafts; creating folders and moving drafts; discussions for drafts, notes, and other entities; starting, observing, pausing, and discarding graduations, with streamed logs; starting, merging, and closing work streams; and receiving and answering user escalations.
60. **RSN-FR-KNRU** The application protocol defines its own operation names, request and response schemas, correlation, error semantics, authorization results, and streamed events. The IDE and application specifications own them, and no requirement of this specification constrains them.
61. **RSN-FR-BSXO** A relay log record names the connection identifier, the `instance_id`, the handle state, and the operation. It names no token, no handle value, no key, no proof, and no payload (per `SRB-server-relay-boundary.md` SRB-FR-IZAB).

## User stories
- As an author away from my machine, I want to scan the QR code my IDE shows once and then reach that IDE from my phone whenever I open the app, so that I do not repeat a setup for every session.
- As an author, I want the relay to carry my traffic without being able to read it, so that a server I do not control never holds my work.
- As an author, I want my phone to say which project the IDE is on and which projects it exposes, so that I know what I am talking to before I ask for anything.
- As an author, I want a lost or stolen phone to lose its access as soon as I revoke it in the IDE, so that a handle outlives no device I still trust.

## Contract surface
This specification owns the session rules of the relay half of the `synthesis-server` crate and one authenticated HTTP route. It exposes no Tauri command, is reachable from neither `src/**` nor `src-tauri/**`, and defines no application operation.

### The modules
```text
server/src/adapters/relay/session.rs   the handle lifecycle and the registration routes
server/src/adapters/relay/projects.rs  the project-discovery route
```

The routing state, its port, and the introspection routes are `SRB-server-relay-boundary.md`'s, and the frames and the WebSocket endpoints are `WSK-websocket.md`'s.

### The HTTP surface
```text
GET /v1/relay/projects   the projects one worker exposes to one registered client
                         Authorization: Bearer <SYNTHESIS_SERVER_TOKEN>
                         X-Synthesis-Client-Handle: <opaque handle>
                         X-Synthesis-Instance-Id: <instance_id>
                      -> 200 OK
                         {"instance_id":"…","selected_project_id":"…"|null,
                          "projects":[{"project_id":"…","display_name":"…"}]}
```

The relay serves no route through which a worker publishes projects: `worker_hello` and `projects_changed` of `WSK-websocket.md` are the whole of the announcement mechanism (RSN-FR-YBGP). `api/openapi.yaml` describes this route beside the other relay routes (per `SAS-server-application-service.md` SAS-FR-JOAX).

### The handle states
```text
pending   the registration is routed and unresolved
active    the registration was accepted; the handle may attach
invalid   revoked, expired, or reset; every later frame is refused
```

### The refusal codes of the HTTP route
```text
unauthenticated  worker_not_found  handle_not_found
handle_revoked   handle_expired    handle_reset
```

## Non-functional requirements
- The session state is the in-memory routing state of `SRB-server-relay-boundary.md` SRB-FR-ZUPL. Nothing this specification adds is durable, and a restart is a full loss by design.
- A handle carries no meaning a reader can recover: it is drawn from a cryptographic random source and is long enough that guessing one is not practical.
- The relay is sized for one author's devices rather than for a fleet: tens of worker routes and tens of client connections at a time.
- Registration is bounded rather than open-ended. A registration connection that reaches no terminal result is closed with the connection it was opened on, and holds no relay state afterwards.
- Nothing in this specification writes an application payload, a key, a proof, or a handle value to a log, a metric, or an error body.

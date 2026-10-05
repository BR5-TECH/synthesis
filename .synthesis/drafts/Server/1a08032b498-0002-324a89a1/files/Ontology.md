## Intent

The server component must expose IDE workers and project data to remote clients. V1 defines the server ontology and prepares a backend API for a client-server architecture with REST APIs and a relay boundary. Detailed WebSocket routes, frames, and state transitions are out of scope here.

The server has two related responsibilities:

1. **Application service** — authenticated REST APIs for users, teams, organizations, memberships, invitations, devices, projects, access control, drafts, and conversations.
2. **Relay** — authenticated worker connections and routing between IDE workers and remote clients. The relay remains transport-focused and must not inspect encrypted application payloads.

The application service owns identity, persistence, project access control, Draft access, and Conversation access. The relay owns connection and route state only. The Backend comms draft defines the detailed relay protocol.

V1 uses an in-memory persistence adapter. The domain and ports must use hexagonal architecture so a durable persistence adapter can replace it later without changing the API contract.

## Ontology

### User

A **user** is a person who uses the IDE and the remote client. A user may own projects, drafts, conversations, devices, and other application entities. Multiple users may collaborate on one project.

A User has a stable application ID, normally a UUID v4. The model must support future OIDC identity mapping by `(issuer, subject)`; email is a profile attribute and must not be the permanent identity key.

V1 also has one persisted administrator User, provisioned from server configuration. The administrator owns all records and bypasses normal project ACL checks.

### Team

A **team** is a group of users. A team has a unique ID, a name, an owner, and memberships. A team contains users only; it cannot contain organizations or other teams.

A team must not be deleted while it would violate ownership or membership invariants. The exact deletion or archival policy must be defined by the API implementation.

### Organization

An **organization** groups teams and may also contain users directly. An organization has a unique ID, a name, and memberships. It cannot contain another organization.

### Membership

A **membership** is an explicit relation between a User and a Team or Organization. It stores the member ID, target ID, role, status, and creation/update timestamps.

Membership is a separate entity and must not be represented only as an array on User, Team, or Organization. Roles must be defined for each target type, including at least owner, administrator, member, and viewer where applicable.

For project ACL evaluation, an active direct User membership grants only the permissions explicitly assigned to that user. An active Team membership grants the permissions of grants addressed to that Team. An active Organization membership grants the permissions of grants addressed to that Organization; membership in a Team does not grant access through an Organization unless the API defines an explicit organization-to-team membership relation, which V1 does not have.

### Invitation

An **invitation** represents a pending request to add a User to a Team or Organization. It stores the target, inviter, invitee identity, role, expiration time, and accepted, revoked, or pending state.

An invitation is single-use. The API must reject expired, revoked, and already accepted invitations.

### Device

A **device** is a registered IDE or remote-client installation belonging to a User. Device identity is separate from the temporary worker `instance_id` and the temporary client handle.

A Device has a stable ID, User owner, device type, display name, creation time, last-seen time, and revocation state. V1 may use the static administrator token for development, but the model must support device-specific authentication when OIDC and device registration are added.

### Relay

A **relay** is the configured server instance to which IDE workers and remote clients connect. It exposes health handling, relay connection endpoints, worker authentication, client registration, project discovery, and relay routing. The exact WebSocket routes and frame contracts belong to the Backend comms draft.

The relay:

- authenticates application requests and IDE relay connections with the configured static bearer token in V1;
- treats that token as the persisted administrator User's credential;
- routes worker, client, and registration connections;
- validates only transport envelopes and explicitly defined opaque routing fields;
- does not inspect, validate, decrypt, authorize, or rewrite QR contents, keys, challenges, proofs, registration payloads, or encrypted application payloads;
- stores connection ownership, active worker routes, authenticated-session routing data, opaque mobile handles, and replacement-notification data in memory only;
- does not persist device registrations, keys, QR contents, application payloads, or message queues;
- drops all active connections and in-memory routes after restart or process failure.

The static token is a V1 development credential for one administrator principal. It is not a model for multi-user authentication. OIDC authentication must be designed as a replacement or additional authentication mode in a later change.

V1 uses one configured relay instance. Clients reconnect to that configured relay after a failure and repeat any required session establishment. The relay must not claim that a session remains active after restart.

### Worker

A **worker** is one running IDE application. It occupies one worker slot and is identified by a cryptographically random, temporary `instance_id` generated in memory when the application starts. The identifier is not persisted and is never reused by a later application run.

A worker:

- authenticates to the relay with the configured bearer token;
- announces the projects it currently exposes;
- accepts registration traffic routed to it;
- serves many connected remote clients;
- broadcasts worker-originated application messages to all authenticated clients attached to it;
- owns the selected-project state shared by all clients attached to it.

There is at most one active worker connection for one `instance_id`. A new connection with the same `instance_id` replaces the old connection.

Each active worker has an independent selected project. Therefore, different IDE workers may have different selected projects, and clients connected to different workers may work with different projects concurrently. A remote application may maintain multiple client sessions at the same time, including sessions attached to different workers; each session follows the selected-project state of its attached worker.

### Project

A **project** is an application entity that can be exposed by a worker. It has a stable project ID, display name, owner, timestamps, and project access-control grants.

A worker announcement maps the application project to an opaque exposed `project_id` and a human-compatible `display_name`. `display_name` is presentation-only and is never a routing key.

Project ACLs may grant access to a User, Team, or Organization. Each grant has a stable grant ID, target type and target ID, role, explicit permissions, creation/update timestamps, and optional revocation metadata. A grant must reference an existing target and project. A project owner is stored explicitly and always retains owner permissions; the owner grant cannot be removed or reduced without first transferring ownership to another User.

V1 defines these project permissions: `project.read`, `project.update`, `project.manage_acl`, `draft.read`, `draft.create`, `draft.update`, `draft.archive`, `draft.delete`, `draft.graduate`, `conversation.read`, `conversation.create`, `conversation.append`, `conversation.update`, `conversation.resolve` and `conversation.lock`. Roles are named bundles of these permissions. The API must publish the role-to-permission mapping and must reject unknown roles or permissions.

The effective permissions for a User are the union of permissions from the user's direct grants, grants to active Teams of which the user is a member, and grants to active Organizations of which the user is a member. Membership status must be active at evaluation time. V1 has no nested Team or Organization membership and no permission inheritance from a Team through an Organization. The administrator bypasses these checks.

When multiple grants apply, permissions are additive. There are no deny grants, and removing one grant must not remove permission supplied by another grant. A duplicate grant for the same project, target type, target ID, and role is rejected. Grant replacement is an explicit update of that grant; it must not silently create a duplicate. Revoking a grant makes it ineffective immediately but preserves its audit fields until the grant is deleted according to the API's referential-integrity rules.

The server enforces project access before serving project-scoped Draft and Conversation operations. The administrator has unrestricted access.

All authenticated clients attached to one worker share that worker's selected project and receive the same project state and events. Clients attached to different workers do not share selected-project state. Any client may request a project switch for its attached worker. The IDE performs the switch atomically, broadcasts the resulting `project_changed` event to every client attached to that worker, and accepts project-scoped messages for the new project only after that event.

If a switch is active, the newest request replaces the older request. The IDE completes the current safe transition step, rejects pending project-scoped messages for the older request with `project_switch_cancelled`, and starts the newest request. An invalid, closed, or unexposed target returns a typed error; the current project remains active and no project-change event is broadcast. The detailed request, response, event, and error frames are defined by the Backend comms draft.

### Client

A **client** is a registered consumer, such as the mobile application. Its connection is established through the relay using an opaque handle assigned during registration. A Client belongs to one User and may be associated with one Device.

Each client connection attaches to one worker at a time. One User or Device may have multiple concurrent client connections, and those connections may attach to the same worker or to different workers. The relay must keep connection state and selected-project observations per connection, while the selected project itself is owned by the attached worker.

The relay may route a client before the IDE authenticates it, but it forwards only the defined pre-authentication frames. The IDE validates the device identity, proof, encrypted session protocol, and application messages. The relay does not authenticate application messages.

Every application message between the worker and client is encrypted and authenticated end-to-end. A client or IDE protocol failure causes a full protocol reset: the relay invalidates the handle, closes the client session, rejects later frames, and requires a new QR registration. The handle is never reused after reset. The detailed reset and reconnect protocol is defined by the Backend comms draft.

### Draft

A **Draft** is project-scoped application data stored by the application service. It has a stable ID, project ID, owner, title or name, content, timestamps, and revision information.

A user may access a Draft only when the server-side project ACL grants the required permission, directly or through a Team or Organization. Draft operations must enforce the same authorization rules for REST requests and any worker/client operation that reaches the application service.

V1 stores Drafts in memory. Restart loses Draft data unless a durable adapter is added later.

### Conversation

A **Conversation** is project-scoped application data associated with a Draft, project, or both. It has a stable ID, project ID, owner, participants or access metadata, timestamps, and an append-only sequence of messages or events.

The server enforces project ACLs before a user can list, read, append to, or modify a Conversation. Conversation history must preserve message order and must not allow an older revision to overwrite a newer one.

V1 stores Conversations in memory. Restart loses Conversation data unless a durable adapter is added later.

## Requirements

- Implement the application service and the relay boundary in one server while keeping their domain and transport responsibilities separate.
- Use hexagonal architecture with domain types, application services, inbound ports, outbound persistence ports, and HTTP/WebSocket adapters.
- Implement in-memory persistence for all V1 application entities: User, Team, Organization, Membership, Invitation, Device, Project, project ACL grant, Draft, and Conversation.
- Define REST CRUD or command endpoints for the application entities and document request, response, validation, authentication, authorization, and error contracts.
- Define the high-level relay connection boundary and adapter interfaces needed by the server skeleton. Defer detailed WebSocket routes, frame schemas, state transitions, message limits, ordering, and reconnect rules to the separate Backend comms draft.
- Support one configured static bearer token for V1. The token authenticates the persisted administrator User for both application API requests and worker relay connections.
- At startup, read the configured administrator ID and static bearer token. Fail startup before serving requests when either value is missing or invalid.
- Create or reconcile the administrator User idempotently at startup. Repeated starts with the same configured administrator ID must not create duplicate users. The administrator role and unrestricted access must be restored if the in-memory record is absent.
- Never expose the configured bearer token through API responses, logs, errors, persistence records, or relay messages. Compare presented credentials using a constant-time method where practical.
- Store explicit owner IDs and ACL grants on application records even when the administrator bypasses authorization. Do not use the static token as a substitute for ownership data.
- Define role names and permission checks for User, Team, Organization, Project, Draft, and Conversation operations. The administrator bypass is the only V1 authorization exception.
- Define ACL grant payloads with target type, target ID, role, explicit permissions, grant identity, lifecycle timestamps, and revocation state. Document the role-permission mapping, membership inheritance, additive conflict behavior, owner protections, duplicate-grant behavior, replacement, revocation, and deletion rules.
- Enforce ACL permissions before every project, Draft, and Conversation operation. Document the required permission for each create, read, update, delete, list, append, project-switch, and ACL-management operation. The relay must not evaluate these permissions.
- Validate ownership, membership, invitation state, project access, and referential integrity in the in-memory adapter or application service before mutating state.
- Define conflict behavior for duplicate IDs, duplicate memberships, stale updates, deleting referenced entities, and concurrent project or conversation updates.
- Keep relay connection state, worker routes, client handles, per-connection worker attachment, worker-owned selected-project state, and replacement notices in memory only.
- Make restart behavior explicit: application records and relay sessions are lost when using the V1 in-memory implementation, and clients must reconnect and repeat session establishment.
- Design the authentication boundary so a future OIDC provider can map `(issuer, subject)` to a persisted User without changing domain ownership or ACL models. OIDC is not required for this V1 implementation.
- Implement server-side Draft and Conversation APIs and storage, but do not integrate the IDE or relay with them; the IDE continues to use local storage.

## Expected deliverables

- OpenAPI specification for the Server in `/api` folder
- REST endpoints for application entities and project ACL operations implementing the OpenAPI specification.
- Hexagonal server architecture with separate domain, application, adapter, and persistence layers.
- In-memory persistence implementation with automated tests for invariants and authorization.
- Startup configuration and reconciliation for the persisted administrator User and shared static bearer token.
- Skeleton relay adapters and routing-state interfaces sufficient for the separate Backend comms draft to add the concrete WebSocket protocol.
- Tests for authentication, administrator access, ownership fields, ACL enforcement, role-permission mapping, membership inheritance, additive grant conflicts, owner protection, grant replacement and revocation, membership and invitation rules, CRUD behavior, restart semantics, and the relay routing-state skeleton, including multiple workers, multiple client connections, worker-scoped project selection, and concurrent clients attached to different workers.
- An update to `specifications/server/BMS-backend-microservice.md` because that specification currently excludes REST endpoints, authentication, authorization, persistence, and application features. The update must preserve the existing health, container, startup, and deployment requirements unless this prompt explicitly changes them. WebSocket route and protocol requirements must be added to that specification only when the separate Backend comms draft is graduated.
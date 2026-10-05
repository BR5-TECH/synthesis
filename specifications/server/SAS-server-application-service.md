# Server application service

**Spec code:** `SAS`

## Intent
The authenticated application half of the Synthesis server. It owns identity, membership, project access control, and the project-scoped application data — Drafts and Conversations — that a remote client reads through the relay. It exists so that a project, a Draft, and a Conversation have one server-side owner, one access-control model, and one REST contract, no matter which worker or client reaches them. The service is arranged as a hexagon: the domain and the ports hold the rules, an in-memory adapter holds the records of V1, and a durable adapter replaces that adapter later without a change to the REST contract. Out of scope: the WebSocket protocol, the relay frames and the relay state machine, which belong to `SRB-server-relay-boundary.md`, `RSN-remote-session.md`, and `WSK-websocket.md`; the desktop application, which keeps its local storage and reaches none of these endpoints in V1.

## Functional requirements

### Startup, configuration, and the administrator
1. **SAS-FR-QJWD** The service reads the administrator user identifier from `SYNTHESIS_SERVER_ADMIN_ID` and the static bearer token from `SYNTHESIS_SERVER_TOKEN` at startup, before the listener is bound.
2. **SAS-FR-VKTP** A missing, empty, or malformed administrator identifier, and a missing or empty token, are each a startup failure: the process writes one diagnostic that names the setting, exits with the configuration status of `BMS-backend-microservice.md` BMS-FR-07, binds no socket, and serves no request.
3. **SAS-FR-HGZL** The administrator identifier is a UUID; a value of another shape is refused as malformed. The token is at least 16 characters after white space is removed.
4. **SAS-FR-PMRB** No diagnostic, log record, error body, response body, persisted record, or relay message holds the token or any part of it. A diagnostic about the token names the variable alone.
5. **SAS-FR-DTXV** Startup creates the administrator User with the configured identifier when no user holds it, and reconciles the existing record to the administrator role when one does. Repeated starts with one identifier create one user.
6. **SAS-FR-NWQE** The administrator User carries the administrator role and the ownership fields of every record it creates. The token is never stored on a user record and is never a substitute for an owner identifier.

### Authentication and authorization
7. **SAS-FR-LFCA** Every route of this specification requires the header `Authorization: Bearer <token>`. A request with no header, a header of another scheme, or a token that does not match the configured token is answered `401 Unauthorized` with the `unauthenticated` error code.
8. **SAS-FR-BZUK** The presented token is compared to the configured token by a method whose duration does not depend on how many leading characters match.
9. **SAS-FR-XRPD** A request that presents the configured token acts as the administrator User of SAS-FR-DTXV. V1 has one authenticated principal, and the administrator bypasses every access-control check of this specification.
10. **SAS-FR-CJNV** The authentication boundary resolves a principal from a credential and returns the identifier of a persisted User. A later OIDC mode maps `(issuer, subject)` onto a persisted User through the same boundary, and changes neither the ownership fields nor the access-control model.
11. **SAS-FR-TWEL** A User record holds an optional identity claim of an issuer and a subject, which is unique across users when present. The email address is a profile attribute, is not the identity key, and never identifies a user for authentication.

### Ontology
12. **SAS-FR-KDSM** A User has a stable identifier, an optional identity claim, a display name, an optional email address, a role of `administrator` or `member`, a status, and creation and update times.
13. **SAS-FR-GVAB** A Team has a stable identifier, a name, an owner User, and creation and update times. A Team holds Users alone: no Team and no Organization is a member of a Team.
14. **SAS-FR-RHYT** An Organization has a stable identifier, a name, an owner User, and creation and update times. An Organization holds Teams and Users, and holds no other Organization.
15. **SAS-FR-MZQF** A Membership is a record of its own, with a stable identifier, a member User identifier, a target type of `team` or `organization`, a target identifier, a role, a status of `active` or `suspended`, and creation and update times. No User, Team, or Organization record holds its memberships as a list.
16. **SAS-FR-EPXN** Team and Organization membership roles are `owner`, `administrator`, `member`, and `viewer`. A role outside that set is refused with the `invalid_role` error code.
17. **SAS-FR-YCWK** An Invitation has a stable identifier, a target type and target identifier, an inviter User, an invitee email address or User identifier, a role, an expiry time, a state of `pending`, `accepted`, or `revoked`, and creation and update times.
18. **SAS-FR-ATLB** An Invitation is single-use: accepting a pending invitation before its expiry creates one active Membership and moves the invitation to `accepted`. An invitation that is expired, revoked, or already accepted is refused `409 Conflict` with the `invitation_not_pending` error code, and creates no membership.
19. **SAS-FR-FQVS** A Device has a stable identifier, an owner User, a device type of `ide` or `client`, a display name, a creation time, a last-seen time, and a revocation state. A Device identifier is separate from the worker `instance_id` and from the opaque client handle of `SRB-server-relay-boundary.md`.
20. **SAS-FR-OKUC** A Project has a stable identifier, a display name, an owner User, creation and update times, and its access-control grants. The display name is presentation only and is never a routing key.
21. **SAS-FR-WBDI** A Draft has a stable identifier, a project identifier, an owner User, a title, a content string, a state of `active`, `archived`, or `graduated`, a revision counter, and creation and update times.
22. **SAS-FR-SVMH** A Conversation has a stable identifier, a project identifier, an optional draft identifier, an owner User, a state of `open`, `resolved`, or `locked`, a revision counter, creation and update times, and an append-only sequence of messages.
23. **SAS-FR-NUZG** A Conversation message has a stable identifier, a sequence number that starts at 1 and rises by 1, an author User identifier, a body, and a creation time. A message is never changed and never removed, and no request may write a sequence number.

### Project access control
24. **SAS-FR-IJRO** The project permissions are `project.read`, `project.update`, `project.manage_acl`, `draft.read`, `draft.create`, `draft.update`, `draft.archive`, `draft.delete`, `draft.graduate`, `conversation.read`, `conversation.create`, `conversation.append`, `conversation.update`, `conversation.resolve`, and `conversation.lock`. A request that names another permission is refused with the `invalid_permission` error code.
25. **SAS-FR-HXAP** The project roles are `owner`, `administrator`, `contributor`, and `viewer`, each a named bundle of the permissions of SAS-FR-IJRO as the contract surface tabulates them. A request that names another role is refused with the `invalid_role` error code.
26. **SAS-FR-LEQB** `GET /v1/roles` returns the role-to-permission mapping for project, team, and organization roles, so a client reads the mapping rather than assuming it.
27. **SAS-FR-CGTM** An access-control grant has a stable identifier, a project identifier, a target type of `user`, `team`, or `organization`, a target identifier, a role, the explicit permissions the role resolves to, creation and update times, and revocation metadata of a revocation time and a revoking User.
28. **SAS-FR-UPKA** A grant whose project or whose target does not exist is refused `404 Not Found` before any state changes.
29. **SAS-FR-ZDVR** The effective permissions of a User on a Project are the union of the permissions of the grants to that User, of the grants to each Team of which the user holds an active Membership, and of the grants to each Organization of which the user holds an active Membership. A revoked grant contributes nothing.
30. **SAS-FR-BJHF** Membership is read at the time the permission is evaluated, and a suspended Membership contributes nothing. V1 holds no nested membership: a Team membership grants no permission through an Organization, and an Organization membership grants no permission through a Team.
31. **SAS-FR-QSLY** Grants are additive and there is no deny grant. Removing or revoking one grant leaves every permission that another grant still supplies.
32. **SAS-FR-VNTC** A second grant with the same project, target type, target identifier, and role is refused `409 Conflict` with the `duplicate_grant` error code. Replacing a grant is an update of that grant by its identifier, which never creates a second record.
33. **SAS-FR-MWOD** The project owner is stored on the project record and holds every permission of SAS-FR-IJRO at all times. The owner grant cannot be deleted, revoked, or reduced to another role; an attempt is refused `409 Conflict` with the `owner_protected` error code.
34. **SAS-FR-RAKX** `POST /v1/projects/{project_id}/transfer-ownership` moves ownership to another existing User, moves the owner grant with it, and is the only way the owner changes. The former owner keeps no permission that no other grant supplies.
35. **SAS-FR-EYUB** Revoking a grant makes it ineffective at once and keeps its identifier, role, permissions, and audit fields readable until the grant is deleted. Deleting a grant removes the record.
36. **SAS-FR-TKPZ** Every project, Draft, and Conversation operation checks the permission the contract surface names for it before it reads or changes any record. A principal without that permission is answered `403 Forbidden` with the `forbidden` error code, and a principal that may not read the project is answered `404 Not Found`, so the existence of a project is not disclosed.
37. **SAS-FR-DHLM** Managing grants needs `project.manage_acl`, reading a project needs `project.read`, and updating a project needs `project.update`. Deleting a project is reserved to its owner and to the administrator.

### Application data
38. **SAS-FR-GBWS** Draft operations need these permissions: list and read `draft.read`, create `draft.create`, update `draft.update`, archive `draft.archive`, graduate `draft.graduate`, and delete `draft.delete`.
39. **SAS-FR-XONP** Conversation operations need these permissions: list and read `conversation.read`, create `conversation.create`, append a message `conversation.append`, update metadata `conversation.update`, resolve `conversation.resolve`, and lock `conversation.lock`.
40. **SAS-FR-JZAC** A Draft update carries the revision the client holds. A revision that is not the current revision is refused `409 Conflict` with the `stale_revision` error code and changes nothing; a successful update raises the revision by 1.
41. **SAS-FR-CQVE** A Conversation metadata update follows the same rule as SAS-FR-JZAC. Appending a message raises the revision, takes the next sequence number, and never overwrites an earlier message.
42. **SAS-FR-PYSU** A message appended to a `locked` Conversation is refused `409 Conflict` with the `conversation_locked` error code. A `resolved` Conversation still accepts a message, which returns it to `open`.
43. **SAS-FR-KRMF** A Draft belongs to exactly one Project and a Conversation to exactly one Project; the project identifier is set when the record is created and never changes.
44. **SAS-FR-WLIG** The same permission check applies to a Draft or Conversation operation whichever adapter reaches the application service, so a relay-borne operation is authorized exactly as the REST route is.

### Validation, conflicts, and referential integrity
45. **SAS-FR-OZET** A create request may carry the identifier of the record. An identifier already held by a record of that kind is refused `409 Conflict` with the `duplicate_id` error code; no identifier is supplied, one UUID version 4 is generated.
46. **SAS-FR-BFHN** A second active Membership for one member, one target type, and one target identifier is refused `409 Conflict` with the `duplicate_membership` error code.
47. **SAS-FR-SGXQ** A record that another record references is not deleted while the reference stands: a User that owns a Team, an Organization, or a Project; a Team or an Organization that a grant addresses; and a Project that holds a Draft or a Conversation. The refusal is `409 Conflict` with the `referenced` error code.
48. **SAS-FR-AVDJ** Deleting a Project deletes its grants, its Drafts, and its Conversations only when the request carries `cascade=true`; without it the Project is refused as referenced.
49. **SAS-FR-TQIM** A field that is absent, empty after white space is removed, or longer than the contract surface allows is refused `400 Bad Request` with the `invalid_field` error code, which names the field.
50. **SAS-FR-NKBC** An identifier in a path that is not a UUID is refused `400 Bad Request`, and a well-formed identifier that no record holds is refused `404 Not Found` with the `not_found` error code.
51. **SAS-FR-XUFA** Every write is applied whole or not at all: a request refused by any check leaves every record as it was.
52. **SAS-FR-HEIV** Concurrent writes to one record are serialized by the persistence adapter, so a revision counter never repeats and a message sequence never repeats.
53. **SAS-FR-ZMPC** Every error answer is one JSON object with an `error` member holding `code` and `message`, and holds no other member. The codes are those this specification names.

### Architecture and persistence
54. **SAS-FR-VBQJ** The service is arranged as domain types, application services, inbound ports, outbound persistence ports, and adapters. The domain and the application layers name no HTTP type and no adapter type.
55. **SAS-FR-IRWO** Each entity of SAS-FR-KDSM to SAS-FR-SVMH has an outbound repository port. The V1 adapter holds every record in memory behind those ports, and a durable adapter replaces it without a change to the ports or to the REST contract.
56. **SAS-FR-GLTD** The in-memory adapter, or the application service before it, checks ownership, membership, invitation state, project access, and referential integrity before it changes any record.
57. **SAS-FR-PDNU** The V1 store holds nothing between processes: a restart loses every user, team, organization, membership, invitation, device, project, grant, Draft, and Conversation, and then reconciles the administrator User of SAS-FR-DTXV again.
58. **SAS-FR-JOAX** The OpenAPI document at `api/openapi.yaml` describes every route, request, response, error code, and WebSocket upgrade of this specification, `SRB-server-relay-boundary.md`, `RSN-remote-session.md`, and `WSK-websocket.md`, and agrees with the routes the service serves.

## Contract surface
This specification owns the application layers of the `synthesis-server` crate, the REST surface below `/v1`, and the OpenAPI document. It exposes no Tauri command and is reachable from neither `src/**` nor `src-tauri/**`.

### The modules
```text
server/src/domain/**          entities, identifiers, roles, permissions, errors
server/src/application/**     inbound ports, outbound ports, application services
server/src/persistence/**     the in-memory adapter of the outbound ports
server/src/adapters/http/**   the Axum routes, the extractors, the payloads
server/src/identity_config.rs the administrator identifier and the static token
api/openapi.yaml              the OpenAPI document
```

### The configuration
| Setting | Environment variable | Required |
| --- | --- | --- |
| Administrator user identifier | `SYNTHESIS_SERVER_ADMIN_ID` | yes |
| Static bearer token | `SYNTHESIS_SERVER_TOKEN` | yes |

Both are read from the environment alone. A command line is readable by every process on the host, so no option carries the token.

### The HTTP surface
```text
POST   /v1/users                                     create a user
GET    /v1/users                                     list users
GET    /v1/users/{user_id}                           read a user
PATCH  /v1/users/{user_id}                           update a user
DELETE /v1/users/{user_id}                           delete a user
POST   /v1/teams                                     create a team
GET    /v1/teams                                     list teams
GET    /v1/teams/{team_id}                           read a team
PATCH  /v1/teams/{team_id}                           update a team
DELETE /v1/teams/{team_id}                           delete a team
POST   /v1/organizations                             create an organization
GET    /v1/organizations                             list organizations
GET    /v1/organizations/{organization_id}           read an organization
PATCH  /v1/organizations/{organization_id}           update an organization
DELETE /v1/organizations/{organization_id}           delete an organization
POST   /v1/memberships                               create a membership
GET    /v1/memberships                               list memberships, filtered
GET    /v1/memberships/{membership_id}               read a membership
PATCH  /v1/memberships/{membership_id}               update a role or a status
DELETE /v1/memberships/{membership_id}               delete a membership
POST   /v1/invitations                               create an invitation
GET    /v1/invitations                               list invitations
GET    /v1/invitations/{invitation_id}               read an invitation
POST   /v1/invitations/{invitation_id}/accept        accept, once
POST   /v1/invitations/{invitation_id}/revoke        revoke
POST   /v1/devices                                   register a device
GET    /v1/devices                                   list devices
GET    /v1/devices/{device_id}                       read a device
PATCH  /v1/devices/{device_id}                       update a device
POST   /v1/devices/{device_id}/revoke                revoke a device
DELETE /v1/devices/{device_id}                       delete a device
GET    /v1/roles                                     the role-permission mapping
POST   /v1/projects                                  create a project
GET    /v1/projects                                  list readable projects
GET    /v1/projects/{project_id}                     read a project
PATCH  /v1/projects/{project_id}                     update a project
DELETE /v1/projects/{project_id}                     delete a project
POST   /v1/projects/{project_id}/transfer-ownership  move ownership
GET    /v1/projects/{project_id}/permissions         effective permissions of a user
GET    /v1/projects/{project_id}/grants              list grants
POST   /v1/projects/{project_id}/grants              create a grant
GET    /v1/projects/{project_id}/grants/{grant_id}   read a grant
PATCH  /v1/projects/{project_id}/grants/{grant_id}   replace the role of a grant
POST   /v1/projects/{project_id}/grants/{grant_id}/revoke  revoke a grant
DELETE /v1/projects/{project_id}/grants/{grant_id}   delete a grant
GET    /v1/projects/{project_id}/drafts              list drafts
POST   /v1/projects/{project_id}/drafts              create a draft
GET    /v1/drafts/{draft_id}                         read a draft
PATCH  /v1/drafts/{draft_id}                         update a draft
POST   /v1/drafts/{draft_id}/archive                 archive a draft
POST   /v1/drafts/{draft_id}/graduate                graduate a draft
DELETE /v1/drafts/{draft_id}                         delete a draft
GET    /v1/projects/{project_id}/conversations       list conversations
POST   /v1/projects/{project_id}/conversations       create a conversation
GET    /v1/conversations/{conversation_id}           read a conversation
PATCH  /v1/conversations/{conversation_id}           update metadata
GET    /v1/conversations/{conversation_id}/messages  read the message sequence
POST   /v1/conversations/{conversation_id}/messages  append a message
POST   /v1/conversations/{conversation_id}/resolve   resolve a conversation
POST   /v1/conversations/{conversation_id}/lock      lock a conversation
```

`GET /v1/health` stays the unauthenticated route of `BMS-backend-microservice.md` BMS-FR-11. The relay routes are `SRB-server-relay-boundary.md`'s and `RSN-remote-session.md`'s, and the WebSocket upgrades are `WSK-websocket.md`'s. `POST /v1/projects` and `GET /v1/projects` above are the application service's own project routes and carry no relay meaning.

### The project roles
| Role | Permissions |
| --- | --- |
| `viewer` | `project.read`, `draft.read`, `conversation.read` |
| `contributor` | the viewer permissions, `draft.create`, `draft.update`, `draft.archive`, `conversation.create`, `conversation.append`, `conversation.update`, `conversation.resolve` |
| `administrator` | the contributor permissions, `project.update`, `project.manage_acl`, `draft.delete`, `draft.graduate`, `conversation.lock` |
| `owner` | every permission of SAS-FR-IJRO |

### The error codes
```text
unauthenticated  forbidden  not_found  invalid_field  invalid_role
invalid_permission  duplicate_id  duplicate_membership  duplicate_grant
owner_protected  referenced  stale_revision  conversation_locked
invitation_not_pending  internal
```

### The field limits
```text
name, title, display name   1..=200 characters
email                       1..=320 characters, holds one @
content, message body       0..=1_000_000 characters
list page size              default 100, maximum 500
```

## Non-functional requirements
- The V1 store is a map in memory behind a lock. It is sized for a development instance rather than for a fleet, and every read of it is a read of one process's memory.
- The service holds no session. A request carries its credential, so any number of processes may serve requests, and none of them shares state with another in V1.
- The domain layer holds no clock and no random source of its own: a time and a generated identifier reach it from the application layer, so a test drives it without waiting and without a random result.
- The REST surface is versioned by its `/v1` prefix. A change that breaks a client takes a new prefix rather than a new shape under this one.
- The service logs the route, the status, and the identifiers of the records it touched. It logs no token, no email address, no Draft content, and no Conversation message body.
- The permission model is written for growth: a new permission is a new member of the permission set and a new column of the role table, and needs no change to the evaluation rule of SAS-FR-ZDVR.

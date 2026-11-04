# Architecture diagram

Declaration: `architecture-beta`

Use an architecture diagram to show services and resources of a system, the groups they belong to (for example a cloud account or a network), and the connections between them.

## Syntax

The diagram has four statement kinds: `group`, `service`, `junction`, and edges. Optional `align` statements give layout hints. Put each statement on its own line.

### Groups

```
group id(icon)[Title]
group id(icon)[Title] in parentId
```

- The icon and the title are optional: `group vpc[VPC]`, `group vpc`.
- `in parentId` nests the group in another group. Declare the parent group first.

### Services

```
service id(icon)[Title]
service id(icon)[Title] in groupId
service id[Title]
service id "Text in place of an icon" [Title]
```

- An ID is letters, digits, `_`, and `-`. It cannot start or end with `-`.
- The title is in square brackets. It can hold spaces. Use quotes when it holds `[` or `]`: `["Queue [main]"]`.
- `in groupId` puts the service in a group. Only a group can be a parent.

### Icons

Built-in icons: `cloud`, `database`, `disk`, `internet`, `server`, and `blank`.

Icons from an Iconify pack use `pack:name`, for example `logos:aws-lambda`. A pack works only when the host page registers it. The app does not register packs, so an unknown icon draws as a question mark. Use the built-in icons.

### Junctions

```
junction id
junction id in groupId
```

A junction is an invisible point. Use it to split one edge into many edges.

### Edges

Form: `lhsId:SIDE <line> SIDE:rhsId`. `SIDE` is the side of the box where the edge connects: `L`, `R`, `T`, or `B`.

| Syntax | Meaning |
|---|---|
| `a:R -- L:b` | Line from the right side of `a` to the left side of `b` |
| `a:R --> L:b` | Arrow into `b` |
| `a:R <-- L:b` | Arrow into `a` |
| `a:R <--> L:b` | Arrows at both ends |
| `a:R -[Label]- L:b` | Line with a label |
| `a:R -[Label]-> L:b` | Labeled arrow |
| `a{group}:R --> L:b{group}` | Edge from the group border of `a` to the group border of `b` |

- `{group}` makes the edge start or end at the border of the group that holds the service. Use it only when the two services are in different groups.
- Edges connect services and junctions. A group ID cannot be an edge end.

### Alignment hints

```
align row a b c
align column d e
```

- `align row` puts the services on one horizontal line, in the given order from left to right.
- `align column` puts them on one vertical line, from top to bottom.
- An `align` needs two or more services or junctions. Each ID can appear once per statement.

### Other statements

| Statement | Effect |
|---|---|
| `title Text` | Diagram title |
| `accTitle: text` / `accDescr: text` | Accessibility text |
| `%% text` | Comment |

## Styling and configuration

Architecture diagrams have no `classDef` or `style`.

```
---
config:
  architecture:
    iconSize: 80
    fontSize: 16
    padding: 40
    nodeSeparation: 75
    idealEdgeLengthMultiplier: 1.5
    edgeElasticity: 0.45
    seed: 1          # fixed seed gives the same layout each time
    randomize: false
---
```

## Examples

```mermaid
architecture-beta
  title Web application on one cloud account
  group cloud_acct(cloud)[Cloud account]
  group private(server)[Private subnet] in cloud_acct
  service users(internet)[Users]
  service gateway(server)[API gateway] in cloud_acct
  service app(server)[App server] in private
  service db(database)[Primary DB] in private
  service files(disk)[File store] in private
  users:R -[HTTPS]-> L:gateway
  gateway:R --> L:app
  app:R -- L:db
  app:B -- T:files
  align row users gateway app db
```

```mermaid
architecture-beta
  group edge_net(internet)[Edge]
  group core(cloud)[Core services]
  group data(database)[Data] in core
  service cdn(internet)[CDN] in edge_net
  service lb(server)[Load balancer] in edge_net
  service api(server)[API] in core
  service worker(server)[Worker] in core
  junction split in core
  service pg(database)[Postgres] in data
  service cache "R" [Cache] in data
  cdn:B --> T:lb
  lb{group}:R -[requests]-> L:split{group}
  split:T --> B:api
  split:B --> T:worker
  api:R --> L:pg
  worker:R --> L:cache
  pg:B <--> T:cache
  align column api split worker
```

## Common mistakes

- A side letter other than `L`, `R`, `T`, `B` fails: `a:X -- L:b`.
- Two elements with the same ID fail, also a group and a service.
- `in parent` with a parent that is a service fails, and an undeclared parent fails.
- An edge that names a group ID fails. Connect services or junctions, or use `{group}`.
- `{group}` on an edge between two services in the same group fails.
- `align row a` with one member fails.
- A title with spaces needs square brackets: `service db(database)[Main DB]`. Quotes alone are icon text, not a title.
- Declare each service before you use it in an edge, so the file is easy to read.

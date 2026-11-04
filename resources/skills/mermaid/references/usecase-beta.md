# Use case diagram

Declaration: `usecase-beta`

Use a use case diagram to show who uses a system and what they can do with it: actors, use cases, system boundaries, and the include, extend, and generalization relations between them.

## Syntax

The first line is `usecase-beta`. Each statement goes on its own line. Indentation is free.

### Actors

| Form | Result |
|---|---|
| `actor Customer` | An actor with the id and label `Customer`. |
| `actor Admin(Main administrator)` | An actor with the id `Admin` and its own label. |
| `actor Admin("Main administrator")` | The same, with a quoted label. |
| `actor "Bank clerk"` | An actor with a generated id. You cannot refer to it later by id. |
| `actor A, B, C` | Three actors on one line. |
| `actor Customer --> Checkout` | An actor and a relation on one line. |

Actor ids are words: letters, digits, and `_`.

### Use cases

| Form | Shape |
|---|---|
| `Login(Sign in)` | ellipse |
| `Report[Generate report]` | rectangle |
| `"Pay bill"` | ellipse, with a generated id |

An id that you use only in a relation becomes a use case with the id as its label. Each element has one label. If you write the same id again with a different label, the parse fails. So give the label once and use the bare id after that.

### Relations

| Operator | Meaning |
|---|---|
| `A --> B` | association, arrow at `B` |
| `A <-- B` | association, arrow at `A` |
| `A -- B` | association with no arrowhead |
| `A --o B`, `A o-- B` | association with a circle end |
| `A --x B`, `A x-- B` | association with a cross end |
| `A ..> : include B` | `A` includes `B`. Both must be use cases. |
| `A ..> : extend B` | `A` extends `B`. Both must be use cases. |

Generalization: `A` is a special kind of `B`. Both must be actors, or both must be use cases. The operator is `--` and then `|>`, with no space:

```
A --|> B
```

Labels go between the dashes: `A -- uses --> B`, `A -- "pays for" --> B`, `A <-- returns -- B`, `A -- fails --x B`.

An edge id goes before the operator: `Customer e1@--> Checkout`. Then `e1@{ animate: true }` or `e1@{ animation: fast }` (or `slow`) animates the edge.

### System boundaries

```
systemBoundary Shop[Online shop]
  actor Clerk
  Checkout(Place order)
  Refund(Refund order)
end
```

- The title form is `Id[Title]`, `Id(Title)`, or `"Title"`.
- Inside a boundary, write only declarations: actors and use cases. Write the relations after `end`.
- `@{ type: package }` after the title draws a package with a tab. The default is `rect`.

### Metadata

A `@{ … }` block goes after a declaration, or alone after the id: `Clerk@{ type: hollow }`.

| Target | Key | Values |
|---|---|---|
| actor | `type` | `normal` (stick figure), `hollow`, `awesome` |
| actor | `icon` | an icon name in quotes, such as `"fa:user"` |
| actor | `business` | `true` or `false` |
| use case | `business` | `true` or `false`. Ellipse use cases only. |
| boundary | `type` | `rect` or `package` |
| edge | `animate` | `true` or `false` |
| edge | `animation` | `fast` or `slow` |

Any other key fails.

### Other statements

| Statement | Effect |
|---|---|
| `direction LR` | `TB`, `TD`, `BT`, `LR`, or `RL`. The default is `LR`. |
| `actor A <<system>>`, `Pay(Pay) <<secondary>>` | A stereotype, after the label or the metadata. |
| `note for Checkout Needs a login` | A note attached to an actor or use case. |
| `json cfg @ { "limit": 5 }` | A table node from a JSON object. Connect it with `-->`, `<--`, or `--` only. |
| `%% comment` | A comment. It must be on its own line. |
| `accTitle: text`, `accDescr: text`, `accDescr { … }` | Accessibility text. |

## Styling and configuration

```
classDef staff fill:#eef,stroke:#336
actor Clerk:::staff
class Checkout, Refund staff
style Customer stroke-width:2px
```

`:::` goes at the end of a declaration. `class` and `style` also work on boundaries, JSON nodes, and edges with an id.

A title comes only from frontmatter. Frontmatter `config:` keys under `usecase:`:

| Key | Default | Effect |
|---|---|---|
| `colorScheme` | `role` | `role` colours each kind the same. `rotate` gives each element its own palette colour. |
| `nodeSpacing`, `rankSpacing` | `50` | Space between elements. |
| `diagramPadding` | `20` | Space around the diagram. |
| `wrappingWidth`, `minNodeWidth` | `120` | Label width. |
| `actorFontSize`, `actorFontFamily`, `actorFontWeight` | `14`, Open Sans, `normal` | Actor labels. |
| `usecaseFontSize`, `usecaseFontFamily`, `usecaseFontWeight` | `12`, Open Sans, `normal` | Use case labels. |

## Examples

```mermaid
---
title: Online shop
---
usecase-beta
  direction LR
  actor Customer
  actor Clerk@{ type: hollow }
  systemBoundary Shop[Online shop]
    Browse(Browse products)
    Checkout(Place order)
    Pay(Pay for order)
    Coupon(Apply coupon)
    Refund[Issue refund]
  end
  Customer --> Browse
  Customer --> Checkout
  Checkout ..> : include Pay
  Coupon ..> : extend Checkout
  Clerk -- approves --> Refund
  note for Coupon Only one coupon per order
```

```mermaid
usecase-beta
  direction TB
  actor User
  actor Admin(Administrator) <<staff>>
  actor Auditor@{ icon: "fa:user-secret" }
  Admin --|> User
  systemBoundary Portal["Customer portal"]@{ type: package }
    Login("Sign in")
    Reset(Reset password)
    Manage(Manage accounts)
  end
  User e1@--> Login
  Reset ..> : extend Login
  Admin --> Manage
  Auditor -- reads --o Manage
  e1@{ animation: slow }
  json policy @ { "minLength": 12, "mfa": true }
  Login -- policy
  classDef staff fill:#eef,stroke:#336
  class Admin staff
```

## Common mistakes

- `include` and `extend` need the colon: `A ..> : include B`. `A ..> B` fails.
- `-.->`, `==>`, and other flowchart arrows fail.
- `--|>` between an actor and a use case fails. `include` or `extend` with an actor fails.
- A relation inside a `systemBoundary` block fails. Put it after `end`.
- An element in two boundaries fails. A second, different label for one id fails.
- A `%%` comment at the end of a statement fails. Put it on its own line.
- `title …`, `package`, `rectangle`, and `skinparam` lines fail. These are PlantUML forms.
- `end`, `actor`, `note`, `class`, `style`, `direction`, `include`, and `extend` are keywords. Do not use them as ids.
- An id with `-` fails. Use `_` or camelCase.
- An unquoted label cannot contain `(` or `)`. Use quotes: `Login("Sign (in)")`.
- `business: true` on a rectangle use case fails. An actor cannot have both `icon` and `type: hollow`.

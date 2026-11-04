# Entity relationship diagram

Declaration: `erDiagram`

Use an entity relationship diagram to show a data model: tables or entities, their attributes and keys, and the cardinality of the relations between them.

## Syntax

### Relationships

Form: `ENTITY_A <left><line><right> ENTITY_B : label`. The label is required.

```
CUSTOMER ||--o{ ORDER : places
ORDER ||--|{ LINE_ITEM : contains
PRODUCT ||..o{ LINE_ITEM : "appears in"
```

Cardinality markers. The left marker describes the left entity side, the right marker the right side.

```
Left   Right   Meaning
|o     o|      zero or one
||     ||      exactly one
}o     o{      zero or more
}|     |{      one or more
```

Combine them with a line: `||--o{`, `}|..|{`, `|o--o|`.

Line kinds:

| Line | Meaning |
|---|---|
| `--` | Identifying (solid line) |
| `..` | Non-identifying (dashed line) |
| `.-` or `-.` | Non-identifying (same as `..`) |

Word aliases for cardinality: `one or zero`, `zero or one`, `one or more`, `one or many`, `many(1)`, `1+`, `zero or more`, `zero or many`, `many(0)`, `0+`, `only one`, `one`, `many`. Word aliases for the line: `to` (identifying), `optionally to` (non-identifying).

```
CUSTOMER only one to zero or more ORDER : places
ORDER one or more optionally to one PAYMENT : "paid by"
```

`u` is a parent marker for a parent-child relation: `PARENT u--|| CHILD : owns`.

### Labels

- One word needs no quotes: `: places`.
- Two or more words need double quotes: `: "is part of"`.
- An empty label is `: ""`.

### Entities and attributes

```
CUSTOMER {
  string id PK "Primary key"
  string email UK
  string name
  int age
}
ORDER {
  int id PK
  string customer_id FK
  decimal(10,2) total
  string[] tags
}
LINE_ITEM {
  int order_id PK, FK
  int product_id PK, FK
}
```

- Each attribute is `type name`, then optional keys, then an optional comment in double quotes.
- Keys: `PK`, `FK`, `UK`. Separate many keys with a comma: `PK, FK`.
- Type and name allow letters, digits, `_`, `-`, `()`, `[]`, `.`, `,`, and `*`. Generic types use `~`: `List~string~`.
- An entity can appear only in relationships, with no attribute block.

### Entity names and aliases

```
p[Person] {
  string name
}
"Order Line" {
  int qty
}
c["Customer account"]
```

- An entity name with spaces needs double quotes.
- `id[Alias]` or `id["Alias text"]` shows the alias. Use `id` in relationships.

### Grouping

```
subgraph sales[Sales]
  CUSTOMER
  ORDER
end
subgraph "Inventory"
  direction LR
  PRODUCT
end
```

Subgraphs can nest and can hold relationships.

### Other statements

| Statement | Effect |
|---|---|
| `direction LR` | `TB`, `BT`, `LR`, or `RL` |
| `title Text` | Diagram title |
| `accTitle: text` / `accDescr: text` | Accessibility text |
| `%% text` | Comment |

## Styling and configuration

```
classDef hot fill:#f96,stroke:#333
class CUSTOMER,ORDER hot
PRODUCT:::hot
CUSTOMER:::hot ||--o{ ORDER:::hot : places
style PAYMENT fill:#bbf,stroke:#333
```

```
---
config:
  er:
    layoutDirection: TB   # TB, BT, LR, RL (the direction statement does the same)
    minEntityWidth: 100
    minEntityHeight: 75
    entityPadding: 15
    nodeSpacing: 140
    rankSpacing: 80
---
```

Top-level keys `look` (`classic`, `handDrawn`, `neo`) and `layout` (`elk`, `dagre`) also apply.

## Examples

```mermaid
---
title: Online shop
---
erDiagram
  direction LR
  CUSTOMER ||--o{ ORDER : places
  ORDER ||--|{ LINE_ITEM : contains
  PRODUCT ||--o{ LINE_ITEM : "is ordered in"
  CUSTOMER |o--o| LOYALTY_CARD : holds
  ORDER }o..|| SHIPPER : "ships with"
  CUSTOMER {
    uuid id PK
    string email UK "Login name"
    string full_name
    date created_at
  }
  ORDER {
    int id PK
    uuid customer_id FK
    decimal(10,2) total
    string status "new, paid, shipped"
  }
  LINE_ITEM {
    int order_id PK, FK
    string sku PK, FK
    int qty
  }
  PRODUCT {
    string sku PK
    string name
    string[] tags
  }
```

```mermaid
erDiagram
  subgraph hr[Human resources]
    emp[Employee] {
      int id PK
      int dept_id FK
      string name
    }
    dept["Department"] {
      int id PK
      string title
    }
    emp }|--|| dept : "works in"
  end
  subgraph projects[Projects]
    PROJECT {
      int id PK
      string code UK
    }
    ASSIGNMENT {
      int emp_id PK, FK
      int project_id PK, FK
      float share
    }
  end
  emp one or more to zero or more ASSIGNMENT : has
  PROJECT only one optionally to zero or many ASSIGNMENT : staffs
  classDef core fill:#e0f0ff,stroke:#36c
  class emp,dept core
  style PROJECT stroke:#c60,stroke-width:2px
```

## Common mistakes

- A relationship without `: label` fails. Add a label, or `: ""`.
- A label with spaces and no quotes fails: write `: "has many"`.
- An attribute name with spaces fails. Use `_`.
- An attribute comment without quotes fails. Write `"comment"`.
- Wrong marker order fails: `||-->|` is not valid. There are no arrowheads in ER diagrams.
- Use the alias ID, not the alias text, in relationships: `emp }|--|| dept`.
- Attribute type comes first, then the name: `string email`.

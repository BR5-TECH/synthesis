# Cynefin framework diagram

Declaration: `cynefin-beta`

Use this diagram to sort situations, problems, or work items into the five Cynefin domains (complex, complicated, clear, chaotic, confusion). It also shows how a situation moves from one domain to another.

## Syntax

The layout is fixed. Complex is top left, complicated is top right, chaotic is bottom left, clear is bottom right, and confusion is the ellipse in the center. A thick "cliff" line separates clear from chaotic. All five domains are always drawn. You only add items and transitions.

```
cynefin-beta
title <text>
<domain>
  "<item>"
  "<item>"
<domain> --> <domain> : "<label>"
```

| Statement | Example | Effect |
| --- | --- | --- |
| `cynefin-beta` or `cynefin-beta:` | `cynefin-beta` | Header. Lowercase. |
| `title <text>` | `title Incident response` | Title above the diagram. |
| `accTitle: <text>` | `accTitle: Team work types` | Accessible title. |
| `accDescr: <text>` or `accDescr { ... }` | `accDescr: Work sorted by domain` | Accessible description. |
| `<domain>` followed by strings | `complex` then `"Run probes"` | Domain block. Each string is one item badge in that domain. |
| `<from> --> <to>` | `chaotic --> complex` | Arrow between two domains. |
| `<from> --> <to> : "<label>"` | `clear --> chaotic : "Complacency"` | Arrow with a label. |
| `%% text` | `%% comment` | Comment. |

Domain names (lowercase only): `complex`, `complicated`, `clear`, `chaotic`, `confusion`.

Item rules:

- Each item is a quoted string: `"text"` or `'text'`. Unquoted text fails.
- Write items on separate lines under the domain name, with any indentation. Several items can also share one line: `complex "a" "b"`.
- Escape a quote inside a string with a backslash: `"say \"hi\""`.
- If a domain name appears in two blocks, the last block replaces the first.
- The confusion ellipse shows at most 3 items, then a "+N more" badge.
- Long items are not wrapped. Keep items short (about 3 to 5 words).

Transition rules:

- Use `-->` only. A transition must end at the end of the line.
- The label must be a quoted string after `:`.
- A transition from a domain to the same domain is ignored.

## Styling and configuration

Frontmatter `config:` keys under `cynefin`:

| Key | Default | Effect |
| --- | --- | --- |
| `width` | `800` | Width of the domain area, in pixels. |
| `height` | `600` | Height of the domain area, in pixels. |
| `padding` | `40` | Space around the domain area. |
| `showDomainDescriptions` | `true` | Shows the decision model (for example "Probe → Sense → Respond") and practice type under each domain name. |
| `boundaryAmplitude` | `8` | Waviness of the domain borders. `0` gives straight lines. |
| `seed` | `0` | Seed for the wavy borders. `0` uses the SVG id, so the shape changes per render. Set a non-zero number for a stable shape. |
| `useMaxWidth` | `true` | Scales the SVG to the container width. |

Theme variables under `themeVariables.cynefin`: `complexBg`, `complicatedBg`, `clearBg`, `chaoticBg`, `confusionBg`, `domainFontSize`, `itemFontSize`, `boundaryColor`, `boundaryWidth`, `cliffColor`, `cliffWidth`, `arrowColor`, `arrowWidth`, `textColor`, `labelColor`.

## Examples

Incident response work sorted by domain, with transitions:

```mermaid
cynefin-beta
title Incident response
complex
  "New failure mode"
  "Run safe-to-fail probes"
complicated
  "Capacity planning"
  "Ask the database expert"
clear
  "Restart a stuck worker"
  'Follow the runbook'
chaotic
  "Full outage"
  "Stabilise first"
confusion
  "Unclear ownership"
chaotic --> complex : "Stabilised"
complex --> complicated : "Pattern found"
complicated --> clear : "Runbook written"
clear --> chaotic : "Complacency"
```

Product backlog view with straight borders, a stable seed, and custom colors:

```mermaid
---
config:
  cynefin:
    width: 900
    height: 560
    boundaryAmplitude: 0
    seed: 42
    showDomainDescriptions: false
  themeVariables:
    cynefin:
      complexBg: "#E6F4EA"
      cliffColor: "#B00020"
---
cynefin-beta
accTitle: Backlog by Cynefin domain
accDescr: Product backlog items sorted into the five Cynefin domains
complex "AI assistant MVP" "New market entry"
complicated "Payment provider migration"
clear "Update privacy page"
chaotic "Security breach response"
confusion "Partner integration request"
%% Expected movement after discovery work
complex --> complicated : "Discovery done"
```

## Common mistakes

- Capitalised domain names fail: write `complex`, not `Complex`.
- Unknown domain names fail. Use only the five names above.
- Unquoted items fail: write `"Ask experts"`, not `Ask experts`.
- Unquoted transition labels fail: write `: "Stabilised"`, not `: Stabilised`.
- `->` fails. Use `-->`.
- The header `cynefin` without `-beta` is not detected.
- You cannot add custom domains, move domains, or style a single item.

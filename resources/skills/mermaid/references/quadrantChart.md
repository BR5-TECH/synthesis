# Quadrant chart

Declaration: `quadrantChart`

Use a quadrant chart to place items on two axes and sort them into four groups. Typical uses are priority matrices, effort-and-impact grids, and competitor maps.

## Syntax

The first statement is `quadrantChart`. Each following line is one statement.

| Statement | Form | Notes |
|---|---|---|
| Title | `title Text` | Text to end of line. |
| X axis | `x-axis Left text --> Right text` | Labels the left and right ends. |
| X axis, one end | `x-axis Left text` | Labels only the left end. |
| Y axis | `y-axis Bottom text --> Top text` | Labels the bottom and top ends. |
| Y axis, one end | `y-axis Bottom text` | Labels only the bottom end. |
| Quadrant label | `quadrant-1 Text` to `quadrant-4 Text` | See the layout below. |
| Point | `Label: [x, y]` | `x` and `y` are from `0` to `1`. |
| Point with class | `Label:::className: [x, y]` | Uses the styles of a `classDef`. |
| Point with styles | `Label: [x, y] radius: 10, color: #ff3300` | Inline styles, comma-separated. |
| Class | `classDef className style: value, style: value` | Shared point styles. |
| Accessibility | `accTitle: Text`, `accDescr: Text`, `accDescr { ... }` | Not shown. |
| Comment | `%% text` | On its own line. |

Quadrant layout:

```
quadrant-2 | quadrant-1      (top-left | top-right)
-----------+-----------
quadrant-3 | quadrant-4      (bottom-left | bottom-right)
```

Point coordinates:

- `x` grows to the right. `y` grows up. `[0, 0]` is bottom-left.
- A coordinate is `0`, `1`, or `0.` followed by digits (`0.25`).
- `1.0`, `.5`, values above `1`, and negative values fail.

Point styles (inline or in `classDef`):

| Style | Value format | Example |
|---|---|---|
| `radius` | Integer | `radius: 12` |
| `color` | Hex color, 3 or 6 digits | `color: #ff3300` |
| `stroke-color` | Hex color, 3 or 6 digits | `stroke-color: #10f0f0` |
| `stroke-width` | Integer with `px` | `stroke-width: 2px` |

Inline styles override class styles. A `classDef` can come after the points that use it.

Text rules:

- `title` takes all text to the end of the line. It can contain any character.
- Unquoted point, axis, and quadrant text can contain letters (also non-ASCII), digits, spaces, and `- _ . , + * & # ! ? ' / = %`.
- Put that text in double quotes when it has `(`, `)`, `:`, `[`, or `]`: `"Plan (2025)": [0.4, 0.7]`.
- A quoted text cannot contain a double quote.

## Styling and configuration

Frontmatter `config.quadrantChart` keys: `chartWidth` (500), `chartHeight` (500), `titleFontSize` (20), `titlePadding` (10), `quadrantPadding` (5), `quadrantTextTopPadding` (5), `quadrantLabelFontSize` (16), `xAxisLabelFontSize` (16), `yAxisLabelFontSize` (16), `xAxisLabelPadding` (5), `yAxisLabelPadding` (5), `pointRadius` (5), `pointLabelFontSize` (12), `pointTextPadding` (5), `xAxisPosition` (`top` or `bottom`), `yAxisPosition` (`left` or `right`), `quadrantInternalBorderStrokeWidth` (1), `quadrantExternalBorderStrokeWidth` (2), `useMaxWidth` (true).

Theme variables (`config.themeVariables`): `quadrant1Fill` to `quadrant4Fill`, `quadrant1TextFill` to `quadrant4TextFill`, `quadrantPointFill`, `quadrantPointTextFill`, `quadrantXAxisTextFill`, `quadrantYAxisTextFill`, `quadrantInternalBorderStrokeFill`, `quadrantExternalBorderStrokeFill`, `quadrantTitleFill`.

## Examples

```mermaid
quadrantChart
    title Reach and engagement of campaigns
    x-axis Low Reach --> High Reach
    y-axis Low Engagement --> High Engagement
    quadrant-1 We should expand
    quadrant-2 Need to promote
    quadrant-3 Re-evaluate
    quadrant-4 May be improved
    Campaign A: [0.3, 0.6]
    Campaign B: [0.45, 0.23]
    Campaign C: [0.57, 0.69]
    Campaign D: [0.78, 0.34]
    Campaign E: [0.40, 0.34]
    Campaign F: [0.35, 0.78]
```

```mermaid
quadrantChart
    title Backlog triage
    accTitle: Effort and impact of backlog items
    accDescr: Each point is one backlog item, placed by effort and impact.
    x-axis Low effort --> High effort
    y-axis Low impact --> High impact
    quadrant-1 Major projects
    quadrant-2 Quick wins
    quadrant-3 Fill-ins
    quadrant-4 Thankless tasks
    Dark mode:::done: [0.2, 0.8]
    "SSO (SAML)":::risk: [0.85, 0.9]
    Export to CSV: [0.15, 0.35]
    Rewrite billing: [0.9, 0.2] radius: 9, color: #d62828
    Tooltip polish: [0.1, 0.1]
    classDef done color: #2a9d8f, radius: 8
    classDef risk color: #e76f51, stroke-color: #264653, stroke-width: 3px
```

```mermaid
---
config:
  quadrantChart:
    chartWidth: 400
    chartHeight: 400
    xAxisPosition: bottom
    pointLabelFontSize: 14
  themeVariables:
    quadrant1Fill: "#e0f2fe"
    quadrant2Fill: "#dcfce7"
    quadrant3Fill: "#f1f5f9"
    quadrant4Fill: "#fef9c3"
    quadrantPointFill: "#1e3a8a"
---
quadrantChart
    x-axis Cheap --> Expensive
    y-axis Slow --> Fast
    Vendor 1: [0.25, 0.75]
    Vendor 2: [0.7, 0.9]
    Vendor 3: [0.5, 0.3]
```

## Common mistakes

- `1.0` fails as a coordinate. Write `1`.
- `.5` fails. Write `0.5`.
- Coordinates outside `0` to `1` fail. Scale your data to `0` to `1` first.
- `color: red` fails. Colors must be hex: `color: #ff0000`.
- `stroke-width: 2` fails. Add the unit: `stroke-width: 2px`.
- `radius: 1.5` fails. `radius` takes an integer.
- Only `radius`, `color`, `stroke-color`, and `stroke-width` are valid point styles.
- `Point (big): [0.5, 0.5]` fails. Quote it: `"Point (big)": [0.5, 0.5]`. The same applies to axis and quadrant text.
- HTML such as `<br>` in text fails.
- A trailing `-->` with no right text is valid but shows an arrow after the left text.

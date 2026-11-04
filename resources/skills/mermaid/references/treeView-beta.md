# Tree view

Declaration: `treeView-beta`

Use a tree view to show a hierarchy as an indented list with connector lines, for example a file tree, a project layout, or a menu structure.

## Syntax

### Header

The first line is `treeView-beta`. Optional `accTitle:`, `accDescr:`, and `title` lines come next, before the first node. The renderer does not draw the title.

### Nodes and indentation

Each line is one node. The indentation sets the parent: a node is a child of the nearest line above it with less indentation. Lines with the same indentation are siblings. More than one node can be at the top level.

```
treeView-beta
  project/
    src/
      main.ts
      utils.ts
    README.md
```

- Use spaces, and use the same step for each level. Each space or tab counts as one column.
- A name runs to the end of the line, so it can contain spaces, dots, `#`, and brackets: `my file (copy).txt`.
- A trailing `/` marks a directory: `src/`. The `/` does not show. A directory label is bold and gets the folder icon.
- Put a name in `"…"` or `'…'` when it starts with a quote character, or to make the text exact. A quoted name cannot contain its own quote character.

### Annotations

Annotations go after the name. Each one needs a space before it.

| Annotation | Effect |
|---|---|
| `## text` | A description, shown in a column to the right. It takes the rest of the line, so put it last. |
| `:::className` | Adds a CSS class to the label. The class `highlight` draws a yellow background behind the row. |
| `icon(folder)`, `icon(file)` | Uses a built-in icon. |
| `icon(pack:name)` | Uses an icon from an iconify pack. The host app must register the pack. |
| `icon()` | Hides the icon of this node. |

```
main.ts :::highlight icon(file) ## The entry point
```

### Box-drawing input

You can also paste a tree that uses box-drawing characters. The parser converts `├──`, `└──`, and `│` (also the heavy forms `┣━━`, `┗━━`, `┃`) to levels.

```
treeView-beta
root/
├── src/
│   └── main.ts
└── README.md
```

In this form, the header line must contain only `treeView-beta`. A top-level line has no indentation. Every indented line must start with a branch character.

### Comments

A `%%` comment must be on its own line. After a name, `%%` becomes part of the name.

## Styling and configuration

Frontmatter `config:` keys under `treeView:`:

| Key | Default | Effect |
|---|---|---|
| `showIcons` | `false` | `true` shows the file and folder icons on all nodes. |
| `rowIndent` | `10` | Horizontal step for each level. |
| `paddingX`, `paddingY` | `5` | Padding of each label. |
| `lineThickness` | `1` | Width of the connector lines. |
| `defaultIconPack` | `""` | Pack for `icon(name)` without a prefix. |
| `filenameIcons` | `{}` | Map from exact file name to icon. Needs `showIcons: true`. |
| `extensionIcons` | `{}` | Map from extension (`.ts` or `ts`) to icon. Needs `showIcons: true`. |

An explicit `icon(…)` always shows, also when `showIcons` is `false`.

```
---
config:
  treeView:
    showIcons: true
    rowIndent: 16
---
```

There is no `classDef`. A class other than `highlight` needs CSS from the host page or `themeCSS`.

## Examples

```mermaid
---
config:
  treeView:
    showIcons: true
---
treeView-beta
  my-app/
    src/
      components/
        Button.tsx
        Modal.tsx
      main.tsx :::highlight ## The entry point
      App.tsx
    public/
      favicon.svg
    package.json ## Scripts and dependencies
    README.md
```

```mermaid
treeView-beta
  accTitle: Help menu
  Help/
    Getting started ## Read this first
    Keyboard shortcuts
    "Release notes" icon(file)
  Settings/ icon()
    Account
    Privacy :::highlight
```

```mermaid
treeView-beta
docs/
├── guide/
│   ├── install.md
│   └── usage.md
└── index.md
```

## Common mistakes

- A `title`, `accTitle`, or `accDescr` line after the first node fails. Put it directly after the header.
- `name:::cls` without a space makes `:::cls` part of the name. Write `name :::cls`.
- Text after `##` is all description. `## note :::cls` puts `:::cls` in the description.
- A trailing `%% comment` becomes part of the name.
- Uneven indentation can attach a node to the wrong parent. Use the same step on every level.
- In box-drawing input, an indented line without `├`, `└`, `┣`, or `┗` fails.
- `icon(mdi:folder)` and other pack icons show only when the host app registers that pack. Use `icon(folder)` or `icon(file)` when you are not sure.

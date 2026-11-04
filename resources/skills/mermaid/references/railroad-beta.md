# Railroad (syntax) diagram

Declaration: `railroad-beta` (function notation), `railroad-ebnf-beta` (EBNF), `railroad-abnf-beta` (ABNF), `railroad-peg-beta` (PEG)

Use this diagram to show a grammar as railroad tracks: one track per rule, with boxes for terminals and rule references. Pick the variant that matches the grammar notation that you already have.

## Syntax

All four variants share this frame:

```
<header>
title <text>
accTitle: <text>
accDescr: <text>
<rule> <definition> ;
<rule> <definition> ;
```

- `title`, `accTitle`, and `accDescr` are optional. They must come before the first rule.
- Every rule ends with `;`. A rule can span several lines.
- Each rule is drawn as its own row: `name =`, a start marker, the track, an end marker.
- The renderer does not draw the `title`.
- The header keyword is lowercase. `RAILROAD-EBNF-BETA` fails.

All variants map to the same visual elements:

| Element | Drawing |
| --- | --- |
| Terminal | Rounded box with the literal text. |
| Nonterminal | Square box with the rule name. |
| Special | Box with the text shown as `? text ?`. |
| Sequence | Boxes in a row. |
| Choice | Parallel tracks. |
| Optional | Track with a bypass. |
| Zero or more | Loop with a bypass. |
| One or more | Loop without a bypass. |

### `railroad-beta`: function notation

Rule form: `name = <expression> ;`. Rule names match `[A-Za-z_][A-Za-z0-9_-]*`.

| Expression | Meaning |
| --- | --- |
| `terminal("text")` | Terminal. |
| `nonterminal("name")` | Rule reference. The name must be a quoted string. |
| `special("text")` | Special box. |
| `sequence(e1, e2, ...)` | Sequence. Needs one or more arguments. |
| `choice(e1, e2, ...)` | Choice. |
| `optional(e)` | Optional. |
| `zeroOrMore(e)` | Zero or more. |
| `oneOrMore(e)` | One or more. |

Strings use `"..."` or `'...'`. Escape with a backslash: `"say \"hi\""`, `\n`, `\t`. Comments: `/* ... */` and `%% ...`. Function names are case-sensitive.

### `railroad-ebnf-beta`: EBNF

Rule form: `name = ... ;` or `name ::= ... ;`. Names match `[A-Za-z_][A-Za-z0-9_-]*` (no spaces).

| Syntax | Meaning |
| --- | --- |
| `"x"` or `'x'` | Terminal. |
| `name` | Rule reference. |
| `a , b` or `a b` | Sequence. The comma is optional. |
| `[ a ]` or `a?` | Optional. |
| `{ a }` or `a*` | Zero or more. |
| `a+` | One or more. |
| `a - b` | Exception. Drawn as `a`, a `-` terminal, then `b`. |
| `? text ?` | Special sequence. |

A choice separates its options with a vertical bar, and a group puts a choice in parentheses:

```
Choice = a | b ;
Group = ( a | b ) , c ;
```

Comments: `(* ... *)`, `/* ... */`, and `%% ...`.

### `railroad-abnf-beta`: ABNF (RFC 5234 style)

Rule form: `name = ... ;`. Names match `[A-Za-z][A-Za-z0-9-]*` (no underscore).

The `;` has two uses. A `;` that is the last character on its line ends the rule. A `;` with any character after it on the same line, also a space, starts a comment to the end of the line. Thus, put comments on their own lines, and do not leave trailing spaces after the rule-end `;`.

| Syntax | Meaning |
| --- | --- |
| `"x"` | Terminal. Double quotes only. |
| `%x41`, `%x30-39`, `%d13.10`, `%b1010` | Numeric value. Drawn as a terminal with this text. |
| `name` | Rule reference. |
| `a b` | Concatenation. |
| `a / b` | Alternative. |
| `( a b )` | Group. |
| `[ a ]` | Optional. |
| `*a` | Zero or more. |
| `1*a` | One or more. |
| `2*5a`, `*3a`, `3a` | Bounded or exact repeat. Drawn as a loop. The counts are not shown. |
| `; text` | Comment to the end of the line. Use its own line. |

### `railroad-peg-beta`: PEG

Rule form: `Name <- ... ;`. Names match `[A-Za-z_][A-Za-z0-9_-]*`.

| Syntax | Meaning |
| --- | --- |
| `'x'` or `"x"` | Literal (terminal). |
| `Name` | Rule reference. |
| `a b` | Sequence. |
| `a / b` | Ordered choice. |
| `a?`, `a*`, `a+` | Optional, zero or more, one or more. |
| `( ... )` | Group. |
| `.` | Any character. Drawn as a special box. |
| `&a`, `!a` | Lookahead. Drawn as a special box with the text `&a` or `!a`. |
| `# text` | Comment. `%%` comments also work. |

## Styling and configuration

All four variants read the `railroad` config key. These keys work in frontmatter:

| Key | Effect |
| --- | --- |
| `padding` | Space inside boxes and around the diagram (default 10). |
| `fontSize`, `fontFamily` | Text of all boxes and rule names. |
| `terminalFill`, `terminalStroke`, `terminalTextColor` | Terminal boxes. |
| `nonTerminalFill`, `nonTerminalStroke`, `nonTerminalTextColor` | Rule reference boxes. |
| `specialFill`, `specialStroke` | Special boxes. |
| `lineColor`, `strokeWidth` | Tracks and box borders. |
| `markerFill` | Start and end markers. |
| `ruleNameColor` | Rule name labels. |
| `useMaxWidth` | Scales the SVG to the container width (default true). |

Colors default to the theme (`secondaryColor` for terminals, `mainBkg` for nonterminals, `lineColor` for tracks). Frontmatter drops `arcRadius`, `verticalSeparation`, `horizontalSeparation`, and `markerRadius` in 12.2.0. `compactMode` and `showMarkers` have no visible effect.

## Examples

Function notation for a JSON subset:

```mermaid
---
config:
  railroad:
    terminalFill: "#FFF4CC"
    fontSize: 13
---
railroad-beta
title JSON value
/* Top-level value */
value = choice(nonterminal("object"), nonterminal("array"), nonterminal("string"), terminal("true"), terminal("false"), terminal("null"));
object = sequence(terminal("{"), optional(sequence(nonterminal("member"), zeroOrMore(sequence(terminal(","), nonterminal("member"))))), terminal("}"));
member = sequence(nonterminal("string"), terminal(":"), nonterminal("value"));
array = sequence(terminal("["), optional(sequence(nonterminal("value"), zeroOrMore(sequence(terminal(","), nonterminal("value"))))), terminal("]"));
string = sequence(terminal('"'), zeroOrMore(special("any char except quote")), terminal('"'));
digits = oneOrMore(nonterminal("digit"));
```

EBNF for arithmetic expressions:

```mermaid
railroad-ebnf-beta
title Arithmetic
(* Expressions with precedence *)
expression ::= term , { ( "+" | "-" ) , term } ;
term = factor { ( "*" | "/" ) factor } ;
factor = [ "-" ] ( number | identifier | "(" expression ")" ) ;
number = digit+ ( "." digit+ )? ;
identifier = letter { letter | digit | "_" } ;
letter = ? any ASCII letter ? ;
digit = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
non-zero = digit - "0" ;
```

ABNF for a URI subset:

```mermaid
railroad-abnf-beta
title URI subset
; Based on RFC 3986
URI = scheme ":" hier-part [ "?" query ] [ "#" fragment ] ;
scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ;
hier-part = "//" authority path-abempty ;
authority = host [ ":" port ] ;
port = *5DIGIT ;
ALPHA = %x41-5A / %x61-7A ;
DIGIT = %x30-39 ;
```

PEG for a key-value list:

```mermaid
railroad-peg-beta
title Key-value list
# Ordered choice tries alternatives from left to right
List <- Pair ( ',' Pair )* EndOfInput ;
Pair <- Key '=' Value ;
Key <- Letter ( Letter / Digit / '_' )* ;
Value <- Number / Quoted ;
Number <- '-'? Digit+ ;
Quoted <- '"' ( !'"' . )* '"' ;
EndOfInput <- !. ;
```

## Common mistakes

- A missing `;` at the end of a rule fails in every variant. EBNF `.` as a terminator fails.
- `title`, `accTitle`, or `accDescr` after the first rule fails.
- A rule name or reference that starts with `title`, `accTitle`, or `accDescr` breaks parsing (for example `titles`). Choose another name.
- `railroad-beta`: `nonterminal(name)` fails. Quote the name: `nonterminal("name")`. Function names such as `sequence` or `choice` cannot be rule names. `sequence()` with no arguments fails.
- EBNF: `a - b` needs spaces. `letter-"x"` reads `letter-` as one name. `//` comments fail.
- ABNF: `r = "a" ; note` reads `; note` as a comment, so the rule has no end. A space after the final `;` has the same result. Single quotes, `=/`, `<prose>`, and `%s"..."` fail. Underscores in names fail.
- PEG: character classes such as `[0-9]` fail. Write `Digit <- '0' / '1' / ... ;` instead. `=` instead of `<-` fails.
- `<` in a terminal shows as its HTML entity text, not as the character, and text that looks like an HTML tag (`"<tag>"`) is removed. Use words such as `"less-than"`, or a special box.

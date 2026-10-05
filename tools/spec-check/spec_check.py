#!/usr/bin/env python3
"""The specification consistency check.

Specification: specifications/infra/SPC-specification-consistency.md

Reads every specification under specifications/, then every citation of a
requirement identifier in the repository, and reports each one that does not
resolve. A specification defines requirements alone: a test scenario is a test,
tagged with the identifiers of the requirements it verifies. Standard library only, so it runs on a bare checkout
before anything is installed (SPC-FR-NUAB).
"""

import os
import re
import sys

# SPC-FR-CALX: codes reserved for test fixtures — identifiers that deliberately
# name a specification that does not exist. No real spec may declare one
# (SPC-FR-GKZK) and citing one is never a violation (SPC-FR-EYLW).
RESERVED_CODES = frozenset({"ABC", "THG"})

SPEC_DIR = "specifications"
TEMPLATE = "TEMPLATE.md"

# SPC-FR-ORAH: the file types a citation may stand in.
CITED_EXTS = (".rs", ".ts", ".tsx", ".md", ".yaml", ".yml", ".json")
# SPC-FR-YRIP: directories the check never descends into.
SKIP_DIRS = frozenset({"node_modules", "dist", "target"})

# SPC-FR-MPSM: exactly one such line per spec, three capital letters.
CODE_RE = re.compile(r"^\*\*Spec code:\*\*\s*`([A-Z]{3})`\s*$", re.M)
# SPC-FR-NZLK: a definition is a numbered list item, anchored at line start.
# Only a requirement is definable; a `-TS-` token is therefore always an
# unresolved citation, which is what retires the identifier kind for good.
DEF_RE = re.compile(r"^[ \t]*\d+\.[ \t]+\*\*([A-Z]{3})-(FR)-([0-9A-Za-z]+)\*\*", re.M)
# SPC-FR-KUSH: a citation, matched on word boundaries with its whole suffix.
CITE_RE = re.compile(r"\b([A-Z]{3})-(FR|TS)-(\d+|[A-Z]{4})\b")
# SPC-FR-VJQT: a specification carries no test-scenario section. Matched at any
# heading level and in any case, a demoted or title-cased heading being the same
# section under another spelling.
TS_SECTION_RE = re.compile(r"^#{2,6}\s+test\s+scenarios\s*$", re.M | re.I)
# SPC-FR-WNZP / SPC-FR-BQLT: the concision caps, in words.
FR_WORD_CAP = 60
WHY_WORD_CAP = 40
# A requirement's statement: the definition line with its bold identifier gone.
FR_LINE_RE = re.compile(
    r"^[ \t]*\d+\.[ \t]+\*\*([A-Z]{3}-FR-[0-9A-Za-z]+)\*\*[ \t]*(.*)$")
# Its optional reason, a sub-bullet directly beneath it.
WHY_LINE_RE = re.compile(r"^[ \t]+-[ \t]+\*Why:\*[ \t]*(.*)$")
# One sentence ends once. A trailing full stop is the sentence's own.
SENTENCE_END_RE = re.compile(r"[.!?](?:\s|$)")
# Full stops that end no sentence: the abbreviations this corpus's prose uses,
# and an ellipsis. Counted before the sentence ends are, so "e.g." is one
# sentence rather than two.
ABBREVIATION_RE = re.compile(r"\b(?:e\.g|i\.e|etc|cf|vs|approx|Fig|No)\.", re.I)
ELLIPSIS_RE = re.compile(r"\.{2,}|\u2026")
# SPC-FR-JHRC: a well-formed suffix is digits or exactly four capitals.
SUFFIX_RE = re.compile(r"^(?:\d+|[A-Z]{4})$")
# SPC-FR-TQVN: an identifier inside inline backticks is mentioned, not cited.
INLINE_CODE_RE = re.compile(r"`[^`]*`")
FENCE_RE = re.compile(r"^\s*(```|~~~)")


class Violation:
    __slots__ = ("kind", "path", "line", "message")

    def __init__(self, kind, path, line, message):
        self.kind = kind
        self.path = path
        self.line = line
        self.message = message

    def sort_key(self):
        return (self.path, self.line or 0, self.kind, self.message)

    def render(self):
        where = self.path if self.line is None else "%s:%d" % (self.path, self.line)
        return "%-15s %s: %s" % (self.kind, where, self.message)


def walk(root, exts):
    """Yield every file under root with one of exts, honouring SPC-FR-YRIP."""
    for base, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS and not d.startswith(".")]
        dirs.sort()
        for name in sorted(files):
            if name.startswith("."):
                continue
            if name.endswith(exts):
                yield os.path.join(base, name)


def read(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        return handle.read()


def fences_balanced(text):
    """SPC-FR-ZMVK: whether every fence this text opens is also closed.

    `strip_fences` toggles a flag, so an unclosed fence blanks the rest of the
    file — and a blanked remainder defines no requirement, carries no heading,
    and reports nothing. Silence is the one failure a check like this must not
    have, so the imbalance is reported in its own right.
    """
    return sum(1 for line in text.split("\n") if FENCE_RE.match(line)) % 2 == 0


def strip_fences(text):
    """Blank every fenced code block, keeping line numbers (SPC-FR-TQVN).

    Inline backticks are left alone here: a spec's own `**Spec code:** `XXX``
    line carries its code inside backticks, so blanking them would erase the
    very thing the corpus pass reads.
    """
    out = []
    in_fence = False
    for line in text.split("\n"):
        if FENCE_RE.match(line):
            in_fence = not in_fence
            out.append("")
            continue
        out.append("" if in_fence else line)
    return "\n".join(out)


def strip_mentions(text):
    """Blank every fenced block and inline-backtick span (SPC-FR-TQVN).

    Replaced with spaces rather than removed so that line and column numbers
    are unchanged for everything that remains.
    """
    return "\n".join(
        INLINE_CODE_RE.sub(lambda m: " " * len(m.group(0)), line)
        for line in strip_fences(text).split("\n")
    )


def load_corpus(root, violations):
    """code -> {'path', 'ids'} for every spec of the corpus."""
    specs = {}
    by_code = {}
    spec_root = os.path.join(root, SPEC_DIR)
    if not os.path.isdir(spec_root):
        violations.append(Violation("no-corpus", SPEC_DIR, None, "no specifications directory"))
        return specs

    for path in walk(spec_root, (".md",)):
        rel = os.path.relpath(path, root)
        # SPC-FR-IFPN / SPC-FR-EQZH: the template and dotfiles are no part of it.
        if os.path.basename(path) == TEMPLATE:
            continue
        # SPC-FR-TQVN applies to the corpus pass as well: a spec code or a
        # definition shown inside a fenced block or backticks is illustration.
        # Reading one as real would register a phantom definition, and a broken
        # citation of it would then resolve — the false negative this whole
        # check exists to prevent.
        raw = read(path)
        if not fences_balanced(raw):
            violations.append(Violation(
                "unclosed-fence", rel, None,
                "a fenced block is opened and never closed, so everything "
                "after it is read as code and checked by nothing"))
        text = strip_fences(raw)
        codes = CODE_RE.findall(text)
        if len(codes) != 1:
            violations.append(Violation(
                "spec-code", rel, None,
                'expected one "**Spec code:**" line, found %d' % len(codes)))
            continue
        code = codes[0]
        if code in RESERVED_CODES:
            violations.append(Violation(
                "reserved-code", rel, None,
                "%s is reserved for test fixtures" % code))
            continue
        if code in by_code:
            violations.append(Violation(
                "code-collision", rel, None,
                "%s is already declared by %s" % (code, by_code[code])))
            continue
        by_code[code] = rel

        # SPC-FR-VJQT: requirements are the acceptance criteria and the tests
        # verify them, so a written scenario section is a third statement of
        # the same thing and no spec carries one.
        match = TS_SECTION_RE.search(text)
        if match:
            violations.append(Violation(
                "ts-section", rel, text.count("\n", 0, match.start()) + 1,
                "a specification carries no Test scenarios section; "
                "tag the tests with the requirement ids they verify"))

        ids = set()
        for match in DEF_RE.finditer(text):
            found, kind, suffix = match.group(1), match.group(2), match.group(3)
            line = text.count("\n", 0, match.start()) + 1
            ident = "%s-%s-%s" % (found, kind, suffix)
            if found != code:
                violations.append(Violation(
                    "foreign-prefix", rel, line,
                    "%s is defined in a spec whose code is %s" % (ident, code)))
                continue
            if not SUFFIX_RE.match(suffix):
                violations.append(Violation(
                    "bad-suffix", rel, line,
                    "%s: a suffix is digits or four capital letters" % ident))
                continue
            if ident in ids:
                violations.append(Violation("duplicate-id", rel, line,
                                            "%s is defined more than once" % ident))
                continue
            ids.add(ident)
        specs[code] = {"path": rel, "ids": ids}
    return specs


def check_citations(root, specs, violations, in_code=None):
    distinct = set()
    total = 0
    for path in walk(root, CITED_EXTS):
        rel = os.path.relpath(path, root)
        # SPC-FR-XKBW: the template's placeholders are not citations.
        if os.path.basename(path) == TEMPLATE and rel.startswith(SPEC_DIR + os.sep):
            continue
        text = read(path)
        if "-FR-" not in text and "-TS-" not in text:
            continue
        for lineno, line in enumerate(strip_mentions(text).split("\n"), 1):
            # A definition declares an identifier rather than citing one, so the
            # leading `N. **XXX-FR-YYY**` is not read here. Without this a
            # foreign-prefix definition is reported twice, once for each pass.
            line = DEF_RE.sub(lambda m: " " * len(m.group(0)), line)
            for match in CITE_RE.finditer(line):
                code, kind, suffix = match.group(1), match.group(2), match.group(3)
                ident = "%s-%s-%s" % (code, kind, suffix)
                if code in RESERVED_CODES:
                    continue
                total += 1
                distinct.add(ident)
                # SPC-FR-QXBM: a citation standing outside the corpus is what
                # makes a requirement verified rather than merely cross-referenced.
                if in_code is not None and not rel.startswith(SPEC_DIR + os.sep):
                    in_code.add(ident)
                spec = specs.get(code)
                if spec is None:
                    violations.append(Violation("unknown-code", rel, lineno,
                                                "%s: no spec declares %s" % (ident, code)))
                elif ident not in spec["ids"]:
                    violations.append(Violation(
                        "unknown-id", rel, lineno,
                        "%s: %s declares %s but defines no such id"
                        % (ident, spec["path"], code)))
    return distinct, total


# SPC-FR-LQTV: the two layer-alignment rules, reported as candidates rather
# than as violations. The corpus holds two naming conventions for one operation
# — with and without its parameter list — so a match is made on the name with
# any trailing parenthesised list removed. What this reports is a list to read,
# not a set of failures, and it never changes the exit code.
OP_QUOTE_RE = re.compile(r'"([a-z][a-z0-9 ()\-,.]*)"')
CORE_SECTIONS = ("### Tauri commands", "### Events (Tauri event bus)", "### Events")


def operation_name(quoted):
    """One operation's comparable name: its text without a trailing (params)."""
    return re.sub(r"\s*\([^)]*\)\s*$", "", quoted).strip()


def section_of(text, heading):
    if heading not in text:
        return ""
    after = text.split(heading, 1)[1]
    return after.split("\n### ", 1)[0].split("\n## ", 1)[0]


def collect_operations(root):
    """UI-delegated names and core-exposed names, each mapped to the specs holding them."""
    ui, core = {}, {}
    spec_root = os.path.join(root, SPEC_DIR)
    for path in walk(spec_root, (".md",)):
        if os.path.basename(path) == TEMPLATE:
            continue
        rel = os.path.relpath(path, root)
        text = read(path)
        if not CODE_RE.search(strip_fences(text)):
            continue
        if "## UI contract boundary" in text and "**Delegated to backend" in text:
            block = text.split("## UI contract boundary", 1)[1].split("\n## ", 1)[0]
            block = block.split("**Delegated to backend", 1)[1]
            for line in block.split("\n"):
                stripped = line.strip()
                # SPC-FR-LQTV: an operation marked inline as a stub is declared
                # to have no core spec yet, which is the escape the rule allows.
                if not stripped.startswith("- ") or "stub" in line:
                    continue
                for quoted in OP_QUOTE_RE.findall(line):
                    ui.setdefault(operation_name(quoted), set()).add(rel)
        for heading in CORE_SECTIONS:
            for line in section_of(text, heading).split("\n"):
                if not line.strip().startswith("- "):
                    continue
                quoted = OP_QUOTE_RE.findall(line)
                if quoted:
                    core.setdefault(operation_name(quoted[0]), set()).add(rel)
    return ui, core


def report_layers(root):
    """SPC-FR-LQTV: print the two candidate lists. Returns nothing the exit code reads."""
    ui, core = collect_operations(root)
    unmatched = sorted(name for name in ui if name not in core)
    orphaned = sorted(name for name in core if name not in ui)
    print("layer alignment: %d UI-delegated names, %d core-exposed names"
          % (len(ui), len(core)))
    for name in unmatched:
        print("%-15s %s: %s is delegated to the backend and no core spec exposes it"
              % ("unmatched-op", sorted(ui[name])[0], name))
    for name in orphaned:
        print("%-15s %s: %s is exposed and no UI spec delegates to it"
              % ("orphaned-op", sorted(core[name])[0], name))
    print("layer alignment: %d unmatched, %d orphaned (candidates, not violations)"
          % (len(unmatched), len(orphaned)))


def words(text):
    """The statement's length, counted the way a reader meets it."""
    return len(text.split())


def sentences(text):
    """How many sentences a reason holds.

    Inline backticks go first: `report_concision` keeps them for the word
    count, a backticked term being a word the reader reads, but a full stop
    inside one ends no sentence. Abbreviations and ellipses go next, each
    carrying a full stop that ends no sentence either.
    """
    body = INLINE_CODE_RE.sub(" ", text)
    body = ELLIPSIS_RE.sub(" ", body)
    body = ABBREVIATION_RE.sub(" ", body)
    return len(SENTENCE_END_RE.findall(body))


def statement_of(lines, index):
    """SPC-FR-WNZP: one requirement's whole statement, and where it ends.

    A definition may wrap onto indented continuation lines, and the corpus
    holds dozens that do. Measuring the first line alone would let exactly the
    longest requirements — the ones most likely to wrap — pass the cap.
    """
    match = FR_LINE_RE.match(lines[index])
    parts = [match.group(2)]
    cursor = index + 1
    while cursor < len(lines):
        line = lines[cursor]
        if not line.strip():
            break
        if not line.startswith((" ", "\t")):
            break
        if WHY_LINE_RE.match(line) or FR_LINE_RE.match(line):
            break
        parts.append(line.strip())
        cursor += 1
    return match.group(1), " ".join(parts), cursor


def report_concision(root, specs):
    """SPC-FR-WNZP / SPC-FR-BQLT: requirements longer than the caps allow.

    A requirement is one testable assertion and a reason is one sentence, so a
    statement past the cap is two requirements, or a requirement with its
    argument still attached. Reported as candidates: the exit code ignores
    them while the corpus is brought under the caps.
    """
    long_frs, long_whys, orphan_whys = [], [], []
    for spec in sorted(specs.values(), key=lambda s: s["path"]):
        # SPC-FR-TQVN reaches this pass too: a requirement shown inside a
        # fenced block illustrates the form and is not one the spec states.
        # Inline backticks stay, a backticked term being a word the reader reads.
        lines = strip_fences(read(os.path.join(root, spec["path"]))).split("\n")
        pending = None
        index = 0
        while index < len(lines):
            line = lines[index]
            if FR_LINE_RE.match(line):
                start = index
                ident, statement, index = statement_of(lines, index)
                count = words(statement)
                if count > FR_WORD_CAP:
                    # The definition's own line, not the end of its wrap.
                    long_frs.append((spec["path"], start + 1, ident, count))
                pending = ident
                continue
            why = WHY_LINE_RE.match(line)
            if why:
                body = why.group(1)
                if pending is None:
                    # SPC-FR-BQLT: a reason under no requirement is reported
                    # rather than dropped, silence being the worse answer.
                    orphan_whys.append((spec["path"], index + 1))
                else:
                    count = words(body)
                    found = sentences(body)
                    if count > WHY_WORD_CAP or found > 1:
                        long_whys.append(
                            (spec["path"], index + 1, pending, count, found))
                index += 1
                continue
            if line.strip() and not line.startswith((" ", "\t")):
                pending = None
            index += 1

    for path, lineno, ident, count in long_frs:
        print("%-15s %s:%d: %s is %d words, over the %d-word cap"
              % ("fr-too-long", path, lineno, ident, count, FR_WORD_CAP))
    for path, lineno, ident, count, found in long_whys:
        why = "%d words" % count if count > WHY_WORD_CAP else "%d sentences" % found
        print("%-15s %s:%d: the reason for %s is %s"
              % ("why-too-long", path, lineno, ident, why))
    for path, lineno in orphan_whys:
        print("%-15s %s:%d: a reason stands under no requirement"
              % ("why-orphaned", path, lineno))
    print("concision: %d requirements over %d words, %d reasons over %d words or one "
          "sentence, %d reasons under no requirement (candidates, not violations)"
          % (len(long_frs), FR_WORD_CAP, len(long_whys), WHY_WORD_CAP,
             len(orphan_whys)))


def report_coverage(specs, in_code):
    """SPC-FR-QXBM: requirements no file outside the corpus cites.

    A requirement is met by code and proved by a test, and both name it. One
    that nothing outside `specifications/` mentions is a requirement nothing
    is known to verify. It is a list to read rather than a failure: a
    requirement may be stated before it is built, and the exit code ignores it.
    """
    uncited = sorted(
        ident
        for spec in specs.values()
        for ident in spec["ids"]
        if ident not in in_code)
    where = {ident: spec["path"]
             for spec in specs.values() for ident in spec["ids"]}
    for ident in uncited:
        print("%-15s %s: %s is cited by no file outside the corpus"
              % ("fr-uncited", where[ident], ident))
    print("requirement coverage: %d of %d cited outside the corpus "
          "(candidates, not violations)"
          % (sum(len(s["ids"]) for s in specs.values()) - len(uncited),
             sum(len(s["ids"]) for s in specs.values())))
    return len(uncited)


def main(argv):
    flags = ("--layers", "--coverage", "--concision")
    unknown = [a for a in argv[1:] if a.startswith("-") and a not in flags]
    if unknown:
        print("unknown option %s; expected any of %s"
              % (", ".join(unknown), ", ".join(flags)), file=sys.stderr)
        return 2
    args = [a for a in argv[1:] if a not in flags]
    if len(args) > 1:
        print("expected at most one root, got %d" % len(args), file=sys.stderr)
        return 2
    layers = "--layers" in argv
    coverage = "--coverage" in argv
    concision = "--concision" in argv
    root = args[0] if args else "."
    violations = []
    in_code = set()
    specs = load_corpus(root, violations)
    distinct, total = check_citations(root, specs, violations, in_code)

    for violation in sorted(violations, key=Violation.sort_key):
        print(violation.render())

    if layers:
        report_layers(root)
    if coverage:
        report_coverage(specs, in_code)
    if concision:
        report_concision(root, specs)

    defined = sum(len(s["ids"]) for s in specs.values())
    uncited = sum(1 for s in specs.values() for i in s["ids"] if i not in in_code)
    print("specs %d  ids defined %d  ids cited %d  citations %d  uncited %d  violations %d"
          % (len(specs), defined, len(distinct), total, uncited, len(violations)))
    # SPC-FR-HBDE
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

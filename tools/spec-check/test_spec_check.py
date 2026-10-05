#!/usr/bin/env python3
"""Tests for the specification consistency check.

Specification: specifications/infra/SPC-specification-consistency.md

Standard library alone, on the same terms as the script under test
(SPC-FR-NUAB): `python3 -m unittest discover -s tools/spec-check` runs them on
a bare checkout, with no toolchain and no install step.

Each test builds a throwaway corpus, runs the real command line against it, and
reads the violation lines and the exit status — the contract SPC-FR-RSIV and
SPC-FR-HBDE define, rather than the internals behind it.
"""

import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "spec_check.py")


class CorpusCase(unittest.TestCase):
    """A temporary repository root holding whatever specs a test needs."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="spec-check-")
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)

    def write(self, relpath, text):
        path = os.path.join(self.root, relpath)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        return path

    def spec(self, code, requirements, extra="", name=None):
        """One well-formed spec: an H1, its code, an intent, its requirements."""
        body = ["# %s" % (name or code), "", "**Spec code:** `%s`" % code, "",
                "## Intent", "Words.", "", "## Functional requirements"]
        body.extend(requirements)
        if extra:
            body.extend(["", extra])
        return self.write("specifications/ui/%s-thing.md" % code,
                          "\n".join(body) + "\n")

    def run_check(self, *flags):
        result = subprocess.run(
            [sys.executable, SCRIPT, self.root, *flags],
            capture_output=True, text=True)
        return result.stdout, result.returncode

    CANDIDATE_KINDS = ("unmatched-op", "orphaned-op", "fr-uncited",
                       "fr-too-long", "why-too-long", "why-orphaned")
    SUMMARY_PREFIXES = ("specs ", "layer alignment", "requirement coverage",
                        "concision:")

    def kinds(self, out):
        """The violation kind of every reported line, in order.

        Candidates are not violations, so they are no part of this. Without
        that filter `assertEqual(self.kinds(out), [])` would stop meaning
        "clean" the moment a test passed --coverage or --concision.
        """
        found = []
        for line in out.strip().split("\n"):
            if not line or line.startswith(self.SUMMARY_PREFIXES):
                continue
            kind = line.split()[0]
            if kind not in self.CANDIDATE_KINDS:
                found.append(kind)
        return found

    def candidates(self, out):
        """The candidate kind of every reported line, in order."""
        return [line.split()[0] for line in out.strip().split("\n")
                if line and line.split()[0] in self.CANDIDATE_KINDS]


class TestDefinitionsAreRequirementsAlone(CorpusCase):
    """SPC-FR-NZLK: a specification defines requirements and nothing else."""

    def test_a_requirement_is_defined_and_resolves(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("src/a.ts", "// ZQX-FR-AAAA\n")
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), [])
        self.assertEqual(code, 0)
        self.assertIn("ids defined 1", out)

    def test_a_test_scenario_definition_registers_nothing(self):
        # The retired kind is no longer definable, so a spec that writes one
        # defines nothing by it and every citation of it fails to resolve.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "2. **ZQX-TS-BBBB** Given a thing, Then it works."])
        out, code = self.run_check()
        self.assertIn("ids defined 1", out)
        # The line is read as a citation of an id nothing defines — that exact
        # mechanism, not merely some failure.
        self.assertEqual(self.kinds(out), ["unknown-id"])
        self.assertEqual(code, 1)

    def test_a_surviving_scenario_citation_is_an_unknown_id(self):
        # SPC-FR-KUSH: leaving -TS- citable is what makes a stale reference
        # surface instead of passing unnoticed.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("src/a.ts", '// ZQX-TS-BBBB: the old tag\n')
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), ["unknown-id"])
        self.assertIn("ZQX-TS-BBBB", out)
        self.assertEqual(code, 1)


class TestNoTestScenarioSection(CorpusCase):
    """SPC-FR-VJQT: a spec carries no Test scenarios heading."""

    def test_the_heading_is_a_violation_naming_its_line(self):
        path = self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."],
                         extra="## Test scenarios\n1. **ZQX-FR-CCCC** placeholder.")
        out, code = self.run_check()
        self.assertIn("ts-section", self.kinds(out))
        # The line it names is the line the heading is actually on.
        reported = [l for l in out.split("\n") if l.startswith("ts-section")][0]
        lineno = int(reported.split(".md:")[1].split(":")[0])
        with open(path, encoding="utf-8") as handle:
            source = handle.read().split("\n")
        self.assertEqual(source[lineno - 1].strip(), "## Test scenarios")
        self.assertEqual(code, 1)

    def test_a_spec_without_the_heading_passes(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        out, code = self.run_check()
        self.assertNotIn("ts-section", out)
        self.assertEqual(code, 0)

    def test_the_heading_inside_a_fenced_block_is_illustration(self):
        # SPC-FR-TQVN governs this pass too: a template shown in a code fence
        # is not a section the spec carries.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."],
                  extra="```\n## Test scenarios\n```")
        out, code = self.run_check()
        self.assertNotIn("ts-section", out)
        self.assertEqual(code, 0)


class TestConcisionCandidates(CorpusCase):
    """SPC-FR-WNZP / SPC-FR-BQLT: the caps, reported without failing."""

    def test_the_cap_is_exclusive_at_sixty_words(self):
        sixty = " ".join(["word"] * 60)
        sixty_one = " ".join(["word"] * 61)
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s." % sixty,
                          "2. **ZQX-FR-BBBB** %s." % sixty_one])
        out, code = self.run_check("--concision")
        self.assertIn("concision: 1 requirements over 60 words", out)
        self.assertIn("ZQX-FR-BBBB is 61 words", out)
        self.assertEqual(code, 0, "a candidate never changes the exit status")

    def test_a_long_reason_is_reported(self):
        forty_one = " ".join(["word"] * 41)
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "   - *Why:* %s" % forty_one])
        out, code = self.run_check("--concision")
        self.assertIn("why-too-long", out)
        self.assertIn("is 41 words", out)
        self.assertEqual(code, 0)

    def test_a_reason_of_two_sentences_is_reported(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "   - *Why:* One reason. And a second."])
        out, code = self.run_check("--concision")
        self.assertIn("why-too-long", out)
        self.assertIn("is 2 sentences", out)

    def test_one_short_sentence_passes(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "   - *Why:* The reason is short."])
        out, _ = self.run_check("--concision")
        self.assertNotIn("why-too-long", out)
        self.assertIn("0 reasons over", out)

    def test_a_requirement_inside_a_fenced_block_is_not_counted(self):
        # SPC-FR-TQVN: a fenced requirement illustrates the form; the spec
        # does not state it, so the caps have nothing to say about it.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."],
                  extra="```\n1. **ZQX-FR-BBBB** %s.\n```" % " ".join(["word"] * 99))
        out, code = self.run_check("--concision")
        self.assertNotIn("fr-too-long", out)
        self.assertIn("0 requirements over 60 words", out)
        self.assertEqual(code, 0)

    def test_the_lists_are_silent_without_the_flag(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s." % " ".join(["word"] * 99)])
        out, code = self.run_check()
        self.assertNotIn("fr-too-long", out)
        self.assertEqual(code, 0)


class TestCoverageCandidates(CorpusCase):
    """SPC-FR-QXBM: requirements nothing outside the corpus cites."""

    def test_a_requirement_no_code_cites_is_a_candidate(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "2. **ZQX-FR-BBBB** It also works."])
        self.write("src/a.ts", "// ZQX-FR-AAAA\n")
        out, code = self.run_check("--coverage")
        self.assertIn("fr-uncited", out)
        self.assertIn("ZQX-FR-BBBB is cited by no file", out)
        self.assertNotIn("ZQX-FR-AAAA is cited by no file", out)
        self.assertEqual(code, 0, "a candidate never changes the exit status")

    def test_a_citation_from_another_spec_does_not_count_as_coverage(self):
        # A cross-reference says a requirement is related to another, never
        # that anything verifies it.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("specifications/ui/YYQ-other.md",
                   "# Other\n\n**Spec code:** `YYQ`\n\n## Intent\nWords.\n\n"
                   "## Functional requirements\n"
                   "1. **YYQ-FR-AAAA** It defers to ZQX-FR-AAAA.\n")
        out, _ = self.run_check("--coverage")
        self.assertIn("ZQX-FR-AAAA is cited by no file", out)

    def test_the_summary_counts_the_uncited(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "2. **ZQX-FR-BBBB** It also works."])
        self.write("src/a.ts", "// ZQX-FR-AAAA\n")
        out, _ = self.run_check("--coverage")
        self.assertIn("uncited 1", out)
        self.assertIn("requirement coverage: 1 of 2 cited outside the corpus", out)


class TestStructuralRulesStillHold(CorpusCase):
    """The rules that predate the change, so the edit did not loosen them."""

    def test_a_missing_spec_code_is_reported(self):
        self.write("specifications/ui/ZQX-thing.md", "# Thing\n\n## Intent\nWords.\n")
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), ["spec-code"])
        self.assertEqual(code, 1)

    def test_two_specs_may_not_share_a_code(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("specifications/core/ZQX-other.md",
                   "# Other\n\n**Spec code:** `ZQX`\n\n## Intent\nWords.\n")
        out, code = self.run_check()
        self.assertIn("code-collision", self.kinds(out))
        self.assertEqual(code, 1)

    def test_a_definition_carrying_another_specs_code_is_reported(self):
        self.spec("ZQX", ["1. **YYQ-FR-AAAA** It works."])
        out, code = self.run_check()
        self.assertIn("foreign-prefix", self.kinds(out))
        self.assertEqual(code, 1)

    def test_a_malformed_suffix_is_reported(self):
        self.spec("ZQX", ["1. **ZQX-FR-Ab** It works."])
        out, code = self.run_check()
        self.assertIn("bad-suffix", self.kinds(out))
        self.assertEqual(code, 1)

    def test_one_identifier_defined_twice_is_reported(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "2. **ZQX-FR-AAAA** It works again."])
        out, code = self.run_check()
        self.assertIn("duplicate-id", self.kinds(out))
        self.assertEqual(code, 1)

    def test_a_citation_of_an_undeclared_code_is_reported(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("src/a.ts", "// QQQ-FR-AAAA\n")
        out, code = self.run_check()
        self.assertIn("unknown-code", self.kinds(out))
        self.assertEqual(code, 1)

    def test_a_reserved_code_is_cited_freely_and_declared_never(self):
        # SPC-FR-CALX / SPC-FR-EYLW: the fixture codes.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("src/a.ts", "// ABC-FR-01 names a spec that does not exist\n")
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), [])
        self.assertEqual(code, 0)

    def test_an_identifier_in_backticks_is_mentioned_not_cited(self):
        # SPC-FR-TQVN.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("src/a.ts", "// see `ZQX-FR-ZZZZ` for the shape\n")
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), [])
        self.assertEqual(code, 0)

    def test_the_template_is_no_part_of_the_corpus(self):
        # SPC-FR-IFPN / SPC-FR-XKBW: it declares placeholders, not requirements.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("specifications/TEMPLATE.md",
                   "# T\n\n**Spec code:** `XXX`\n\n## Functional requirements\n"
                   "1. **XXX-FR-QJZM** placeholder.\n\n## Test scenarios\n")
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), [])
        self.assertEqual(code, 0)

    def snapshot(self):
        """Every file under the root, by path and by content."""
        found = {}
        for dirpath, _, filenames in os.walk(self.root):
            for name in filenames:
                path = os.path.join(dirpath, name)
                with open(path, "rb") as handle:
                    found[path] = hashlib.sha256(handle.read()).hexdigest()
        return found

    def test_the_check_writes_nothing(self):
        # SPC-FR-MDLI. Compared by content and by the file set, so a rewrite
        # inside one filesystem tick and a newly created file are both caught.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "   - *Why:* The reason is short."])
        before = self.snapshot()
        self.run_check("--layers", "--coverage", "--concision")
        self.assertEqual(before, self.snapshot())


class TestTheCheckIsNeverSilent(CorpusCase):
    """The one failure a drift check must not have is saying nothing."""

    def test_an_unclosed_fence_is_reported_rather_than_swallowing_the_file(self):
        # SPC-FR-ZMVK. `strip_fences` toggles a flag, so an odd number of fence
        # markers blanks everything after it — and a blanked remainder defines
        # nothing, carries no heading, and reports nothing.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."],
                  extra="```\n2. **ZQX-FR-BBBB** %s.\n\n## Test scenarios"
                        % " ".join(["word"] * 99))
        out, code = self.run_check("--concision")
        self.assertIn("unclosed-fence", self.kinds(out))
        self.assertEqual(code, 1)

    def test_a_demoted_or_recased_scenarios_heading_is_still_caught(self):
        # A heading is the same section whatever level or case it is written at.
        for heading in ("### Test scenarios", "## Test Scenarios",
                        "###### test scenarios"):
            with self.subTest(heading=heading):
                self.setUp()
                self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."], extra=heading)
                out, code = self.run_check()
                self.assertIn("ts-section", self.kinds(out))
                self.assertEqual(code, 1)

    def test_a_reason_under_no_requirement_is_reported(self):
        # SPC-FR-BQLT: once a heading resets the pending requirement, a reason
        # beneath it belongs to nothing and must not be dropped in silence.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."],
                  extra="## Notes\n   - *Why:* A reason under no requirement.")
        out, code = self.run_check("--concision")
        self.assertIn("why-orphaned", self.candidates(out))
        self.assertIn("1 reasons under no requirement", out)
        self.assertEqual(code, 0)

    def test_an_unknown_option_is_refused_rather_than_ignored(self):
        # A typo that reads as a clean pass is worse than an error.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        out, code = self.run_check("--covrage")
        self.assertEqual(code, 2)
        self.assertNotIn("requirement coverage", out)

    def test_a_second_root_is_refused(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        result = subprocess.run(
            [sys.executable, SCRIPT, self.root, self.root],
            capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)


class TestTheStatementIsTheWholeListItem(CorpusCase):
    """SPC-FR-WNZP: a requirement that wraps is measured whole."""

    def test_an_indented_continuation_counts_toward_the_cap(self):
        # Measuring the definition line alone would let exactly the longest
        # requirements — the ones that wrap — pass the cap.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s" % " ".join(["word"] * 20),
                          "   %s." % " ".join(["word"] * 45)])
        out, code = self.run_check("--concision")
        self.assertIn("ZQX-FR-AAAA is 65 words", out)
        self.assertEqual(code, 0)

    def test_the_reported_line_is_the_definition_not_the_end_of_the_wrap(self):
        path = self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s" % " ".join(["word"] * 61),
                                 "   a continuation line."])
        out, _ = self.run_check("--concision")
        reported = [l for l in out.split("\n") if l.startswith("fr-too-long")][0]
        lineno = int(reported.split(".md:")[1].split(":")[0])
        with open(path, encoding="utf-8") as handle:
            source = handle.read().split("\n")
        self.assertIn("**ZQX-FR-AAAA**", source[lineno - 1])

    def test_a_reason_is_no_part_of_the_statement(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s." % " ".join(["word"] * 55),
                          "   - *Why:* %s." % " ".join(["word"] * 30)])
        out, _ = self.run_check("--concision")
        self.assertIn("concision: 0 requirements over 60 words", out)
        self.assertIn("0 reasons over 40 words", out)


class TestSentenceCounting(CorpusCase):
    """SPC-FR-BQLT: a full stop that ends no sentence must not count."""

    def one_why(self, body):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works.",
                          "   - *Why:* %s" % body])
        return self.run_check("--concision")[0]

    def test_an_abbreviation_is_not_a_sentence_end(self):
        for body in ("It is fast, e.g. faster than before.",
                     "The rule holds, i.e. always.",
                     "Paths, files, etc. are covered."):
            with self.subTest(body=body):
                self.setUp()
                self.assertNotIn("why-too-long", self.one_why(body))

    def test_an_ellipsis_is_not_a_sentence_end(self):
        self.assertNotIn("why-too-long", self.one_why("It waits... then acts."))

    def test_a_full_stop_inside_backticks_is_not_a_sentence_end(self):
        self.assertNotIn("why-too-long",
                         self.one_why("The token `x. y` is one word."))

    def test_a_decimal_is_not_a_sentence_end(self):
        self.assertNotIn("why-too-long", self.one_why("It takes 3.5 seconds."))

    def test_two_real_sentences_are_still_caught(self):
        self.assertIn("is 2 sentences", self.one_why("One reason. And a second."))

    def test_the_word_cap_boundary_is_exclusive(self):
        forty = " ".join(["word"] * 40)
        self.assertNotIn("why-too-long", self.one_why(forty))
        self.setUp()
        self.assertIn("is 41 words", self.one_why(" ".join(["word"] * 41)))


class TestTheRetirementIsProvedForBothSuffixForms(CorpusCase):
    """SPC-FR-KUSH: legacy scenario ids were numbered as well as lettered."""

    def test_a_numbered_scenario_citation_is_an_unknown_id(self):
        # CITE_RE's suffix alternation is (\d+|[A-Z]{4}); a regression
        # narrowing it to four capitals would let every numbered legacy
        # citation through while the rest of the suite still passed.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** It works."])
        self.write("src/a.ts", "// ZQX-TS-12: the old tag\n")
        out, code = self.run_check()
        self.assertEqual(self.kinds(out), ["unknown-id"])
        self.assertIn("ZQX-TS-12", out)
        self.assertEqual(code, 1)


class TestFlagsAndStatusDoNotInterfere(CorpusCase):
    """Candidates neither add failure nor hide it."""

    def test_a_violation_still_fails_with_every_candidate_flag_on(self):
        # The existing tests prove candidates do not ADD failure. This proves
        # they do not SUPPRESS one.
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s." % " ".join(["word"] * 99)])
        self.write("src/a.ts", "// QQQ-FR-AAAA\n")
        out, code = self.run_check("--layers", "--coverage", "--concision")
        self.assertIn("unknown-code", self.kinds(out))
        self.assertEqual(code, 1)

    def test_each_flag_reports_only_its_own_list(self):
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s." % " ".join(["word"] * 99)])
        coverage, _ = self.run_check("--coverage")
        self.assertIn("fr-uncited", coverage)
        self.assertNotIn("fr-too-long", coverage)
        self.setUp()
        self.spec("ZQX", ["1. **ZQX-FR-AAAA** %s." % " ".join(["word"] * 99)])
        concision, _ = self.run_check("--concision")
        self.assertIn("fr-too-long", concision)
        self.assertNotIn("fr-uncited", concision)


class TestCorpusShapesThatMustNotCrash(CorpusCase):
    """Degenerate but legal specs."""

    def test_a_spec_with_no_requirements_section_is_legal(self):
        self.write("specifications/ui/ZQX-thing.md",
                   "# Thing\n\n**Spec code:** `ZQX`\n\n## Intent\nWords.\n")
        out, code = self.run_check("--coverage", "--concision")
        self.assertEqual(self.kinds(out), [])
        self.assertIn("requirement coverage: 0 of 0", out)
        self.assertEqual(code, 0)

    def test_crlf_line_endings_are_read_the_same(self):
        body = ("# Thing\r\n\r\n**Spec code:** `ZQX`\r\n\r\n## Intent\r\nWords.\r\n"
                "\r\n## Functional requirements\r\n"
                "1. **ZQX-FR-AAAA** %s.\r\n"
                "   - *Why:* %s.\r\n"
                "\r\n## Test scenarios\r\n" % (" ".join(["word"] * 61),
                                              " ".join(["word"] * 41)))
        path = os.path.join(self.root, "specifications/ui/ZQX-thing.md")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(body.encode("utf-8"))
        out, code = self.run_check("--concision")
        self.assertIn("ts-section", self.kinds(out))
        self.assertIn("is 61 words", out)
        self.assertIn("is 41 words", out)
        self.assertEqual(code, 1)


if __name__ == "__main__":
    unittest.main()

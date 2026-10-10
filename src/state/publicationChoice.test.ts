import { describe, expect, it } from "vitest";

import {
  describeMismatches,
  describePublicationChoice,
  publicationErrorText,
} from "./publicationChoice";

describe("publication choice wording", () => {
  it("GHP-FR-CDVT, NAW-FR-QEZG: a missing choice and a root choice read as a root issue", () => {
    expect(describePublicationChoice(undefined)).toBe("Root issue");
    expect(describePublicationChoice({ kind: "root" })).toBe("Root issue");
    expect(
      describePublicationChoice({
        kind: "root",
        issueType: "Bug",
        milestoneNumber: 8,
        milestoneTitle: "v1.3",
      }),
    ).toBe("Root issue · Type Bug · Milestone v1.3");
  });

  it("NAW-FR-NHAY: a sub-issue names its parent, repository, Type, and milestone", () => {
    expect(
      describePublicationChoice({
        kind: "sub_issue",
        parentRepositoryOwner: "acme",
        parentRepositoryName: "widgets",
        parentIssueNumber: 412,
        issueType: "Task",
        milestoneNumber: 7,
      }),
    ).toBe("Sub-issue of #412 in acme/widgets · Type Task · Milestone #7");
    expect(describePublicationChoice({ kind: "sub_issue", parentIssueNumber: 3 })).toBe(
      "Sub-issue of #3",
    );
  });

  it("NAW-FR-EOTB: the differences read as a list", () => {
    expect(describeMismatches([])).toBe("");
    expect(describeMismatches(["body"])).toBe("body");
    expect(describeMismatches(["parent", "type"])).toBe("parent and Type");
    expect(describeMismatches(["title", "parent", "milestone"])).toBe(
      "title, parent and milestone",
    );
  });

  it("NAW-FR-HZSW: the new typed errors read as sentences that keep their code; the old ones render as they were", () => {
    expect(publicationErrorText("parent_issue_unavailable")).toMatch(
      /missing, closed, or no longer usable.*\(parent_issue_unavailable\)$/,
    );
    expect(publicationErrorText("Error: sub_issue_link_failed")).toContain("(sub_issue_link_failed)");
    expect(publicationErrorText("github_unreachable")).toBe("github_unreachable");
  });

  it("GHA-FR-LBLM: a host mismatch refusal is stated and keeps its code", () => {
    const text = publicationErrorText("Error: github_host_mismatch");
    expect(text).toContain("The project token belongs to another GitHub host than this remote. Pick or add a token for the host of the remote.");
    expect(text).toContain("(github_host_mismatch)");
  });

  it("AAP-FR-LRTC: a refused certificate names its host and its cause", () => {
    expect(
      publicationErrorText("Error: tls_untrusted:unknown_issuer:api.github.com"),
    ).toBe("The certificate of api.github.com is not trusted: the issuer of the certificate is unknown.");
  });
});

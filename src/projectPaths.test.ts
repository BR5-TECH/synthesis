import { describe, expect, it } from "vitest";

import { isAbsolutePath, pickerStart, projectRelativePath } from "./projectPaths";

describe("projectPaths (SET-FR-22, SET-FR-FPCX)", () => {
  it("SET-FR-22, SET-FR-FPCX: a path inside the project folder is read relative to it", () => {
    expect(projectRelativePath("/work/app", "/work/app/docker/agent.Dockerfile")).toBe(
      "docker/agent.Dockerfile",
    );
    expect(projectRelativePath("/work/app/", "/work/app/ctx")).toBe("ctx");
    expect(projectRelativePath("/work/app", "/work/app")).toBe(".");
  });

  it("SET-FR-22, SET-FR-FPCX: a path outside the project folder has no relative form", () => {
    expect(projectRelativePath("/work/app", "/work/certs")).toBeNull();
    // A sibling whose name starts with the project's name is not inside it.
    expect(projectRelativePath("/work/app", "/work/app-extra/x")).toBeNull();
    expect(projectRelativePath("", "/work/app")).toBeNull();
  });

  it("SET-FR-FPCX: a Windows path compares without case and with either separator", () => {
    expect(projectRelativePath("C:\\Work\\App", "c:\\work\\app\\ctx\\sub")).toBe("ctx/sub");
    expect(projectRelativePath("C:\\Work\\App", "D:\\certs")).toBeNull();
  });

  it("SET-FR-FPCX: a picker opens in the configured folder, or in the project folder", () => {
    expect(pickerStart("/work/app", "")).toBe("/work/app");
    expect(pickerStart("/work/app", "  ")).toBe("/work/app");
    expect(pickerStart("/work/app", "ctx/sub")).toBe("/work/app/ctx/sub");
    expect(pickerStart("/work/app", "./ctx")).toBe("/work/app/ctx");
    expect(pickerStart("/work/app", "/etc/certs")).toBe("/etc/certs");
    expect(pickerStart("", "")).toBeUndefined();
    expect(isAbsolutePath("C:\\certs")).toBe(true);
    expect(isAbsolutePath("../certs")).toBe(false);
  });
});

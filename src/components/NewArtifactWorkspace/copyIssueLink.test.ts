/**
 * The clipboard write of the publication tag's copy part
 * (`../../../specifications/ui/NAW-new-artifact.md` NAW-FR-KVFM, NAW-FR-ZBZH,
 * and `NTF-notifications.md` NTF-FR-EPOA, NTF-FR-RQFW).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { resetToasts, visibleToasts } from "../../state/toasts";
import { resetLogBufferForTest } from "../../logging";
import {
  copyIssueLink,
  linkCopiedBody,
  linkCopyFailedBody,
  LINK_COPIED_KEY,
  LINK_COPIED_TITLE,
  LINK_COPY_FAILED_KEY,
  LINK_COPY_FAILED_TITLE,
} from "./copyIssueLink";

const URL_418 = "https://github.com/acme/widgets/issues/418";

function installClipboard(value: unknown) {
  Object.defineProperty(window.navigator, "clipboard", {
    configurable: true,
    value,
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "append_log_records"
      ? Promise.resolve()
      : Promise.reject(new Error(`unexpected command ${cmd}`)),
  );
  resetToasts();
  resetLogBufferForTest();
});
afterEach(() => {
  Reflect.deleteProperty(window.navigator, "clipboard");
  resetLogBufferForTest();
});

describe("copyIssueLink", () => {
  it("NAW-FR-KVFM, NTF-FR-EPOA: a write that succeeds puts the URL on the clipboard and shows the Info toast", async () => {
    const writeText = vi.fn(async () => {});
    installClipboard({ writeText });

    await expect(copyIssueLink(418, URL_418)).resolves.toBe(true);

    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith(URL_418);
    expect(visibleToasts()).toEqual([
      expect.objectContaining({
        key: LINK_COPIED_KEY,
        level: "Info",
        title: LINK_COPIED_TITLE,
        body: linkCopiedBody(418),
        address: null,
      }),
    ]);
    // NTF-FR-EPOA states the key, the title, and the body, not this module.
    expect(visibleToasts()[0]).toEqual(
      expect.objectContaining({
        key: "draft-issue-link-copied",
        title: "Link copied",
        body: "The link to issue #418 is on the clipboard.",
      }),
    );
  });

  it("NAW-FR-ZBZH, NTF-FR-RQFW: a write the platform refuses shows the Error toast and does not reject", async () => {
    installClipboard({
      writeText: vi.fn(async () => {
        throw new DOMException("Document is not focused.", "NotAllowedError");
      }),
    });

    await expect(copyIssueLink(418, URL_418)).resolves.toBe(false);

    expect(visibleToasts()).toEqual([
      expect.objectContaining({
        key: LINK_COPY_FAILED_KEY,
        level: "Error",
        title: LINK_COPY_FAILED_TITLE,
        body: linkCopyFailedBody(418),
        address: null,
      }),
    ]);
    // NTF-FR-RQFW states the key, the title, and the body, not this module.
    expect(visibleToasts()[0]).toEqual(
      expect.objectContaining({
        key: "draft-issue-link-copy-failed",
        title: "Link not copied",
        body: "The link to issue #418 was not copied to the clipboard.",
      }),
    );
  });

  it.each([
    ["an Error", new Error("x")],
    ["a TypeError", new TypeError("x")],
    ["a string", "x"],
    ["undefined", undefined],
  ])("NTF-FR-RQFW: a write that throws %s fails without a rejection", async (_, thrown) => {
    installClipboard({
      writeText: vi.fn(async () => {
        throw thrown;
      }),
    });

    await expect(copyIssueLink(418, URL_418)).resolves.toBe(false);

    expect(visibleToasts().map((t) => [t.key, t.level])).toEqual([
      [LINK_COPY_FAILED_KEY, "Error"],
    ]);
  });

  it.each([
    ["no clipboard", undefined],
    ["a clipboard with no writeText", {}],
  ])("NTF-FR-RQFW: %s fails without a rejection", async (_, clipboard) => {
    installClipboard(clipboard);

    await expect(copyIssueLink(418, URL_418)).resolves.toBe(false);

    expect(visibleToasts().map((t) => [t.key, t.level])).toEqual([
      [LINK_COPY_FAILED_KEY, "Error"],
    ]);
  });

  it("NTF-FR-EPOA, NTF-FR-RQFW: a second toast of one kind replaces the first, and the two kinds stand apart", async () => {
    installClipboard({ writeText: vi.fn(async () => {}) });
    await copyIssueLink(418, URL_418);
    await copyIssueLink(419, "https://github.com/acme/widgets/issues/419");
    expect(visibleToasts().map((t) => t.body)).toEqual([linkCopiedBody(419)]);

    installClipboard({
      writeText: vi.fn(async () => {
        throw new DOMException("Denied.", "NotAllowedError");
      }),
    });
    await copyIssueLink(418, URL_418);
    await copyIssueLink(419, "https://github.com/acme/widgets/issues/419");
    expect(visibleToasts().map((t) => [t.key, t.body])).toEqual([
      [LINK_COPY_FAILED_KEY, linkCopyFailedBody(419)],
      [LINK_COPIED_KEY, linkCopiedBody(419)],
    ]);
  });
});

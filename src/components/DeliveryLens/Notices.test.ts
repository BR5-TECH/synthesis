/**
 * The time of a read, in words (`../../../specifications/ui/MDL-map-delivery-lens.md`
 * MDL-FR-KKYH, MDL-FR-LSKL).
 */
import { describe, expect, it } from "vitest";

import { readTimeText } from "./Notices";

const NOW = Date.parse("2026-11-14T12:00:00Z");
const before = (ms: number) => new Date(NOW - ms).toISOString();

describe("readTimeText (MDL-FR-KKYH, MDL-FR-LSKL)", () => {
  it("MDL-FR-KKYH: a read less than a minute ago is just now", () => {
    expect(readTimeText(before(0), NOW)).toBe("read just now");
    expect(readTimeText(before(59_999), NOW)).toBe("read just now");
  });

  it("MDL-FR-KKYH: a read less than an hour ago is counted in minutes", () => {
    expect(readTimeText(before(60_000), NOW)).toBe("read 1 min ago");
    expect(readTimeText(before(59 * 60_000), NOW)).toBe("read 59 min ago");
  });

  it("MDL-FR-KKYH: a read less than a day ago is counted in hours", () => {
    expect(readTimeText(before(60 * 60_000), NOW)).toBe("read 1 h ago");
    expect(readTimeText(before(23 * 60 * 60_000 + 59 * 60_000), NOW)).toBe("read 23 h ago");
  });

  it("MDL-FR-LSKL: a read a day ago or more is named by its date", () => {
    expect(readTimeText(before(24 * 60 * 60_000), NOW)).toBe("read 2026-11-13");
    expect(readTimeText("2026-10-02T08:30:00Z", NOW)).toBe("read 2026-10-02");
  });

  it("MDL-FR-LSKL: a time that cannot be read is named as unknown", () => {
    expect(readTimeText("not a time", NOW)).toBe("read at an unknown time");
    expect(readTimeText("", NOW)).toBe("read at an unknown time");
  });
});

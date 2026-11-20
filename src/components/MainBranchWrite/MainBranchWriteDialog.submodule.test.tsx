import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { MainBranchWriteHost } from ".";
import { reasonSentence, runMainBranchWrite } from "../../state/mainBranchWrite";
import {
  isEditGateReading,
  isEditHeld,
  primeEditGate,
  rememberConfirmation,
  resetEditGateForTest,
} from "../../state/mainBranchWrite/editGate";
import { mainGuard, scriptedReads, unconfirmedRefusal } from "../../test/mainBranchWriteFixtures";

afterEach(cleanup);

describe("the main branch write confirmation of a submodule", () => {
  it("MBW-FR-DKOY, MBW-FR-YSMC: a refusal of a submodule names it, and Confirm passes its repository", async () => {
    const guard = mainGuard({ repository: "vendor/kit" });
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    render(<MainBranchWriteHost />);
    const write = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(unconfirmedRefusal(guard))
      .mockResolvedValueOnce("written");
    const run = runMainBranchWrite("commit", write);

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("main is the default branch of the submodule vendor/kit.");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      expect(await run).toBe("written");
    });
    expect(reads.confirms).toEqual(["main"]);
    expect(reads.confirmRepositories).toEqual(["vendor/kit"]);
    expect(write).toHaveBeenCalledTimes(2);
  });

  it("MBW-FR-DKOY: the reason of a protected submodule branch names the submodule", () => {
    expect(reasonSentence(mainGuard({ repository: "vendor/kit", protection: "protected" }))).toBe(
      "main is the default branch of the submodule vendor/kit, and GitHub protects it.",
    );
    expect(
      reasonSentence(
        mainGuard({ repository: "vendor/kit", isDefault: false, protection: "protected", branch: "rel" }),
      ),
    ).toBe("GitHub protects rel in the submodule vendor/kit.");
  });

  it("GSM-FR-KGOP, MBW-FR-DKOY: a submodule confirmation of main does not confirm the main repository's main", async () => {
    resetEditGateForTest();
    scriptedReads(mainGuard());
    primeEditGate();
    await waitFor(() => expect(isEditGateReading()).toBe(false));
    expect(isEditHeld()).toBe(true);
    rememberConfirmation(mainGuard({ repository: "vendor/kit", confirmed: true }));
    expect(isEditHeld()).toBe(true);
    rememberConfirmation(mainGuard({ confirmed: true }));
    expect(isEditHeld()).toBe(false);
    resetEditGateForTest();
  });

  it("MBW-FR-DKOY: the main repository's guard keeps its words", () => {
    expect(reasonSentence(mainGuard())).toBe("main is the default branch of this repository.");
  });

  it("MBW-FR-YSMC, GSM-FR-GHFY: a write refused for two repositories asks for each, then runs", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    render(<MainBranchWriteHost />);
    const write = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(unconfirmedRefusal(mainGuard({ repository: "a" })))
      .mockRejectedValueOnce(unconfirmedRefusal(mainGuard({ repository: "b" })))
      .mockResolvedValueOnce("written");
    const run = runMainBranchWrite("push", write);
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(reads.confirmRepositories).toEqual(["a"]));
    await waitFor(() => expect(write).toHaveBeenCalledTimes(2));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      expect(await run).toBe("written");
    });
    expect(reads.confirmRepositories).toEqual(["a", "b"]);
    expect(write).toHaveBeenCalledTimes(3);
  });

  it("MBW-FR-YSMC: a second refusal for a repository confirmed in this write ends it with the error", async () => {
    scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    render(<MainBranchWriteHost />);
    const refusal = unconfirmedRefusal(mainGuard({ repository: "a" }));
    const write = vi.fn<() => Promise<string>>().mockRejectedValue(refusal);
    const run = runMainBranchWrite("commit", write);
    const settled = run.then(
      () => "resolved",
      (e: unknown) => String(e),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      expect(await settled).toBe(refusal);
    });
    expect(write).toHaveBeenCalledTimes(2);
  });

  it("MBW-FR-DKOY, MBW-FR-UNAN: Check again reads the guard of the submodule", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    render(<MainBranchWriteHost />);
    const guard = mainGuard({
      repository: "vendor/kit",
      protection: "unknown",
      protectionError: "github_unreachable",
    });
    const write = vi.fn<() => Promise<string>>().mockRejectedValueOnce(unconfirmedRefusal(guard));
    void runMainBranchWrite("commit", write);
    reads.answer = guard;
    reads.reads.length = 0;
    const readSpy = vi.spyOn(reads, "read");
    fireEvent.click(await screen.findByRole("button", { name: "Check again" }));
    await waitFor(() => expect(readSpy).toHaveBeenCalled());
    expect(readSpy.mock.calls[readSpy.mock.calls.length - 1]).toEqual(["main", true, "vendor/kit"]);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  });
});


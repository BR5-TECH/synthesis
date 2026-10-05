import { act } from "@testing-library/react";

import { AUTOSAVE_DELAY_MS } from "../state/editSessions";

/**
 * EDT-FR-70 / FLO-FR-27: let the rest after the last edit elapse, so the write
 * the document scheduled for itself lands.
 *
 * This is what replaced the Save click these tests used to make. There is no
 * Save control any more — an artifact writes itself — so "the user saves" is no
 * longer an interaction to fire but a pause to wait out, and a test that wants
 * the bytes on disk waits for the same thing the author does.
 *
 * A real wait rather than a fake-timer advance, deliberately: the rest lives in
 * the session store, which is constructed by the component under test, so a
 * timer faked after the render would never see it.
 */

/**
 * How long past the rest to wait, for the write's own promise and the React
 * commit behind it.
 *
 * Generous rather than tight. Most callers follow this with a `waitFor`, which
 * absorbs a short margin — but the ones asserting that *nothing* was written
 * cannot flake, they can only false-pass, and a margin too small would make them
 * read the same as a helper that returned immediately.
 */
const MARGIN_MS = 200;

export async function letWriteLand(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) =>
      setTimeout(resolve, AUTOSAVE_DELAY_MS + MARGIN_MS),
    );
  });
}

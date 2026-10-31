/**
 * Voice mode at the level of the main window (`VOM-voice-mode.md`): the hotkey,
 * the replies, the transcript, and the provider state. These run once, and they
 * need no surface to be mounted.
 */
import { useEffect } from "react";

import * as api from "../api";
import {
  onAppPreferencesChanged,
  onDiscussionChanged,
  onVoiceActivityChanged,
  onVoiceAiChanged,
  onVoiceTranscriptChanged,
} from "../events";
import { isTextField } from "../dropGuard";
import { logWarn } from "../logging";
import { loadAppPreferences, refreshAppPreferences } from "../state/appPreferences";
import { getVoiceState } from "../state/voiceSession";
import type { AppPreferences, VoiceAiIntegrations } from "../types";
import { DEFAULT_VOICE_HOTKEY, chordOf, mainKeyOf, matchesHotkey } from "../voice/hotkey";
import { errorCode } from "../voice/voiceMessages";
import {
  noteActivity,
  noteDiscussionChanged,
  noteProvider,
  noteTranscript,
  pressRecord,
  releaseRecord,
  setVoiceBehaviour,
  voiceComposerMounted,
} from "../voice/voiceController";

/** VOM-FR-UXKL, VOM-FR-IAVZ. */
export function applyIntegrations(integrations: VoiceAiIntegrations): void {
  const record = integrations.records.find(
    (r) => r.provider === integrations.active && r.verifiedAt !== undefined,
  );
  noteProvider(record !== undefined, record?.speechRoute != null);
}

/** VOM-FR-DXJT: a dialog or a menu that does not hold the composer. */
function foreignOverlayMounted(): boolean {
  const key = getVoiceState().activeKey;
  const overlays = document.querySelectorAll('[role="dialog"], [role="menu"], [role="listbox"]');
  return Array.from(overlays).some(
    (overlay) => key === null || overlay.querySelector(`[data-composer-key="${CSS.escape(key)}"]`) === null,
  );
}

export function useVoiceShell(): void {
  useEffect(() => {
    let hotkey = DEFAULT_VOICE_HOTKEY;
    let held = false;
    const unlisten: (() => void)[] = [];
    let cancelled = false;
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisten.push(fn);
    };
    const applyPreferences = (prefs: AppPreferences) => {
      hotkey = prefs.voiceHotkey ?? DEFAULT_VOICE_HOTKEY;
      setVoiceBehaviour(prefs.voiceHotkeyBehaviour ?? "hold");
    };

    void loadAppPreferences().then(applyPreferences);
    // VAI-FR-AVBR: the Settings window is another webview, so the cached
    // record here must be read again.
    void onAppPreferencesChanged(() => {
      void refreshAppPreferences().then(applyPreferences);
    }).then(keep);
    void api
      .listVoiceAiIntegrations()
      .then(applyIntegrations)
      .catch((error: unknown) => {
        logWarn(["frontend"], "voice ai integrations could not be read", {
          code: errorCode(error),
        });
      });
    void onVoiceAiChanged(applyIntegrations).then(keep);
    void onVoiceTranscriptChanged(({ sessionId, text }) => noteTranscript(sessionId, text)).then(keep);
    void onVoiceActivityChanged(noteActivity).then(keep);
    void onDiscussionChanged(noteDiscussionChanged).then(keep);

    const onKeyDown = (event: KeyboardEvent) => {
      if (!matchesHotkey(event, hotkey) || !voiceComposerMounted()) return;
      // VOM-FR-SAMO: a held key repeats, and the press stands.
      if (event.repeat) {
        if (held) stopHere(event);
        return;
      }
      // VOM-FR-HXLV.
      if (isTextField(event.target) || event.isComposing) return;
      if (foreignOverlayMounted()) return;
      stopHere(event);
      // VOM-FR-WPRW: stopped all the same, or the record control takes the Space.
      if (getVoiceState().inputMode === "vad") return;
      held = true;
      pressRecord();
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (!held) return;
      const main = chordOf({
        code: event.code,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        metaKey: false,
      });
      const modifierLeft = !chordHoldsModifiers(event, hotkey);
      if (main === mainKeyOf(hotkey) || modifierLeft) {
        stopHere(event);
        held = false;
        releaseRecord();
      }
    };
    // VOM-FR-XQUY: a window with no focus gets no `keyup`.
    const onBlur = () => {
      held = false;
      releaseRecord();
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", onBlur);
    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
}

/**
 * The hotkey is handled here and goes no further. The record control holds focus
 * in voice mode, and a Space in the chord would press it a second time.
 */
function stopHere(event: KeyboardEvent): void {
  event.preventDefault();
  event.stopPropagation();
}

/** Each modifier of the chord is still down. */
function chordHoldsModifiers(event: KeyboardEvent, chord: string): boolean {
  const parts = chord.split("+");
  return (
    (!parts.includes("Ctrl") || event.ctrlKey) &&
    (!parts.includes("Alt") || event.altKey) &&
    (!parts.includes("Shift") || event.shiftKey) &&
    (!parts.includes("Cmd") || event.metaKey)
  );
}

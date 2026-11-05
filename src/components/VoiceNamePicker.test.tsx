/**
 * The voice name of the two selectors (`VAI-voice-ai-settings.md`
 * VAI-FR-KWSN, VAI-FR-PMDX; `AGT-agents.md` AGT-FR-HXVQ).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import {
  VOICE_LANGUAGES,
  VoiceNamePicker,
  composeVoiceName,
  parseVoiceName,
} from "./VoiceNamePicker";

afterEach(cleanup);

describe("the voice name", () => {
  it("VAI-FR-PMDX: a name of the form opens in the selectors, and each other name is Other", () => {
    expect(parseVoiceName("")).toEqual({ sex: "", language: "", other: "" });
    expect(parseVoiceName("female")).toEqual({ sex: "female", language: "", other: "" });
    expect(parseVoiceName("male-british-english")).toEqual({
      sex: "male",
      language: "british-english",
      other: "",
    });
    expect(parseVoiceName("male-russian")).toEqual({ sex: "other", language: "", other: "male-russian" });
    expect(parseVoiceName("Female")).toEqual({ sex: "other", language: "", other: "Female" });
    expect(parseVoiceName("cedar")).toEqual({ sex: "other", language: "", other: "cedar" });
  });

  it("VAI-FR-PMDX: the selectors compose the name, and each composed name reads back", () => {
    expect(composeVoiceName({ sex: "", language: "polish", other: "" })).toBe("");
    expect(composeVoiceName({ sex: "male", language: "", other: "" })).toBe("male");
    expect(composeVoiceName({ sex: "other", language: "", other: " cedar " })).toBe("cedar");
    for (const sex of ["female", "male"] as const) {
      for (const language of VOICE_LANGUAGES) {
        const name = composeVoiceName({ sex, language, other: "" });
        expect(name).toBe(`${sex}-${language}`);
        expect(parseVoiceName(name)).toEqual({ sex, language, other: "" });
      }
    }
  });
});

describe("the picker", () => {
  it("AGT-FR-HXVQ: with a live name, each key of Other changes the value", () => {
    const onChange = vi.fn();
    render(
      <VoiceNamePicker value="" defaultLabel="Provider default" liveName testId="p" onChange={onChange} />,
    );
    fireEvent.change(screen.getByTestId("p-sex"), { target: { value: "other" } });
    fireEvent.change(screen.getByLabelText("Voice name"), { target: { value: "c" } });
    expect(onChange).toHaveBeenLastCalledWith("c");
  });

  it("VAI-FR-KWSN: a language is kept through a change of sex, and a new stored value replaces the choice", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <VoiceNamePicker value="female-polish" defaultLabel="Server default" testId="p" onChange={onChange} />,
    );
    fireEvent.change(screen.getByTestId("p-sex"), { target: { value: "male" } });
    expect(onChange).toHaveBeenLastCalledWith("male-polish");
    rerender(
      <VoiceNamePicker value="female" defaultLabel="Server default" testId="p" onChange={onChange} />,
    );
    expect(screen.getByTestId("p-sex")).toHaveValue("female");
    expect(screen.getByTestId("p-language")).toHaveValue("");
  });

  it("VAI-FR-KWSN: a language is not kept through the default voice or Other", () => {
    const onChange = vi.fn();
    render(
      <VoiceNamePicker value="female-polish" defaultLabel="Server default" testId="p" onChange={onChange} />,
    );
    const sex = screen.getByTestId("p-sex");
    fireEvent.change(sex, { target: { value: "" } });
    expect(onChange).toHaveBeenLastCalledWith("");
    fireEvent.change(sex, { target: { value: "female" } });
    expect(onChange).toHaveBeenLastCalledWith("female");
    fireEvent.change(screen.getByTestId("p-language"), { target: { value: "polish" } });
    fireEvent.change(sex, { target: { value: "other" } });
    fireEvent.change(sex, { target: { value: "male" } });
    expect(onChange).toHaveBeenLastCalledWith("male");
  });

  it("VAI-FR-KWSN: Other with no name, or a name of spaces, changes nothing", () => {
    const onChange = vi.fn();
    render(
      <VoiceNamePicker value="female-polish" defaultLabel="Provider default" liveName testId="p" onChange={onChange} />,
    );
    fireEvent.change(screen.getByTestId("p-sex"), { target: { value: "other" } });
    const name = screen.getByLabelText("Voice name");
    fireEvent.change(name, { target: { value: "   " } });
    fireEvent.blur(name);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("VAI-FR-KWSN: a save that fails shows the stored value again", async () => {
    const onChange = vi.fn(() => Promise.reject(new Error("unreachable")));
    render(<VoiceNamePicker value="male" defaultLabel="Server default" testId="p" onChange={onChange} />);
    fireEvent.change(screen.getByTestId("p-sex"), { target: { value: "female" } });
    await waitFor(() => expect(screen.getByTestId("p-sex")).toHaveValue("male"));
  });

  it("VAI-FR-KWSN: a disabled picker disables each control", () => {
    render(
      <VoiceNamePicker value="cedar" defaultLabel="Server default" disabled testId="p" onChange={vi.fn()} />,
    );
    expect(screen.getByTestId("p-sex")).toBeDisabled();
    expect(screen.getByTestId("p-language")).toBeDisabled();
    expect(screen.getByLabelText("Voice name")).toBeDisabled();
  });
});

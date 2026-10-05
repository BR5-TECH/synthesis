/**
 * The shared composer (`CVP-conversation-presentation.md` CVP-FR-40, CVP-FR-41,
 * CVP-FR-47, CVP-FR-55, CVP-FR-64; `CMT-comments.md` CMT-FR-34, CMT-FR-51).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { DiscussionComposer, type DiscussionComposerProps } from "./DiscussionComposer";
import { isImeComposing, isPostAccelerator } from "./composerKeys";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerAttachments,
  setComposerBody,
} from "../../state/discussionSession";
import type { Agent, Discussion, ProjectAgent } from "../../types";

beforeEach(() => clearAllDiscussionSessions());
afterEach(cleanup);

const human = { kind: "human", login: "raver119" } as const;

function discussion(id = "d1"): Discussion {
  return {
    id,
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [
      {
        id: `${id}-c1`,
        author: human,
        body: "first",
        quotes: [],
        attachments: [],
        createdAt: "2026-02-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

const archAgent: ProjectAgent = {
  agent: {
    id: "a1",
    nickname: "arch",
    title: "",
    provider: "openrouter",
    modelId: "m",
    instructions: "",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    reasoning: null,
  } as Agent,
  availability: "ready",
};

function replyProps(
  over: Partial<DiscussionComposerProps> = {},
): DiscussionComposerProps {
  return {
    mode: "reply",
    discussion: discussion(),
    agents: [],
    onReply: vi.fn(async () => undefined),
    ...over,
  } as DiscussionComposerProps;
}

const field = () => screen.getByRole("textbox") as HTMLTextAreaElement;

function type(text: string) {
  fireEvent.change(field(), { target: { value: text } });
}

describe("post accelerator rule", () => {
  it("CVP-FR-40: Enter with Ctrl or Cmd is the accelerator, plain Enter is not", () => {
    const base = { key: "Enter", metaKey: false, ctrlKey: false };
    expect(isPostAccelerator({ ...base, ctrlKey: true })).toBe(true);
    expect(isPostAccelerator({ ...base, metaKey: true })).toBe(true);
    expect(isPostAccelerator(base)).toBe(false);
    expect(isPostAccelerator({ ...base, key: "a", ctrlKey: true })).toBe(false);
  });

  it("CVP-FR-40: an IME composition is never the accelerator", () => {
    const key = { key: "Enter", metaKey: true, ctrlKey: false };
    expect(isImeComposing({ ...key, nativeEvent: { isComposing: true } })).toBe(true);
    expect(isImeComposing({ ...key, keyCode: 229 })).toBe(true);
    expect(isPostAccelerator({ ...key, keyCode: 229 })).toBe(false);
    expect(
      isPostAccelerator({ ...key, nativeEvent: { isComposing: true } }),
    ).toBe(false);
  });
});

describe("DiscussionComposer keyboard", () => {
  it("CVP-FR-40: Ctrl+Enter posts the body and prevents the newline", async () => {
    const props = replyProps();
    render(<DiscussionComposer {...props} />);
    type("hello");
    const notPrevented = fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    expect(notPrevented).toBe(false);
    await act(async () => {});
    expect(props.mode === "reply" && props.onReply).toHaveBeenCalledTimes(1);
    expect(props.mode === "reply" && props.onReply).toHaveBeenCalledWith(
      "d1",
      "hello",
      [],
      [],
    );
  });

  it("CVP-FR-40: Cmd+Enter posts the body", async () => {
    const props = replyProps();
    render(<DiscussionComposer {...props} />);
    type("hello");
    fireEvent.keyDown(field(), { key: "Enter", metaKey: true });
    await act(async () => {});
    expect(props.mode === "reply" && props.onReply).toHaveBeenCalledTimes(1);
  });

  it("CVP-FR-40: a posted body clears the field and the stored text", async () => {
    render(<DiscussionComposer {...replyProps()} />);
    type("hello");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    await act(async () => {});
    expect(field().value).toBe("");
    expect(getDiscussionSession("d1").body).toBe("");
  });

  it("CVP-FR-40: plain Enter inserts a newline and posts nothing", async () => {
    const props = replyProps();
    render(<DiscussionComposer {...props} />);
    type("hello");
    const notPrevented = fireEvent.keyDown(field(), { key: "Enter" });
    expect(notPrevented).toBe(true);
    expect(props.mode === "reply" && props.onReply).not.toHaveBeenCalled();
  });

  it("CVP-FR-40: nothing posts while an IME composition is in progress", async () => {
    const props = replyProps();
    render(<DiscussionComposer {...props} />);
    type("こんにちは");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true, isComposing: true });
    fireEvent.keyDown(field(), { key: "Enter", metaKey: true, keyCode: 229 });
    await act(async () => {});
    expect(props.mode === "reply" && props.onReply).not.toHaveBeenCalled();
    expect(field().value).toBe("こんにちは");
  });

  it("CVP-FR-40: an empty or whitespace body posts nothing", async () => {
    const props = replyProps();
    render(<DiscussionComposer {...props} />);
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    type("   \n ");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    await act(async () => {});
    expect(props.mode === "reply" && props.onReply).not.toHaveBeenCalled();
  });

  it("CVP-FR-40: nothing posts while the composer is disabled for want of an identity", async () => {
    const props = replyProps({ disabled: true });
    render(<DiscussionComposer {...props} />);
    setComposerBody("d1", "typed before");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    await act(async () => {});
    expect(props.mode === "reply" && props.onReply).not.toHaveBeenCalled();
  });

  it("CVP-FR-40: a locked discussion carries no composer", () => {
    render(<DiscussionComposer {...replyProps({ locked: true })} />);
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("CVP-FR-40: a second accelerator press while a post is in flight posts nothing", async () => {
    let resolve: () => void = () => {};
    const onReply = vi.fn(
      () => new Promise<void>((r) => {
        resolve = r;
      }),
    );
    render(<DiscussionComposer {...replyProps({ onReply })} />);
    type("hello");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    expect(onReply).toHaveBeenCalledTimes(1);
    await act(async () => resolve());
  });

  it("CVP-FR-40: the accelerator does not post while the mention picker is open", async () => {
    const props = replyProps({ agents: [archAgent] });
    render(<DiscussionComposer {...props} />);
    const user = userEvent.setup();
    await user.click(field());
    await user.keyboard("hi @a");
    expect(screen.queryByRole("listbox")).not.toBeNull();
    await user.keyboard("{Control>}{Enter}{/Control}");
    expect(props.mode === "reply" && props.onReply).not.toHaveBeenCalled();
  });
});

describe("DiscussionComposer refusal", () => {
  it("CVP-FR-41: a refused accelerator post renders the typed error and keeps the body and attachments", async () => {
    const onReply = vi.fn(async () => {
      throw "discussion_locked";
    });
    render(<DiscussionComposer {...replyProps({ onReply })} />);
    setComposerAttachments("d1", [
      {
        name: "notes.txt",
        input: { kind: "url", url: "https://example.com/n", mediaType: "text/plain" },
      },
    ]);
    type("keep me");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    await act(async () => {});
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(field().value).toBe("keep me");
    expect(getDiscussionSession("d1").attachments).toHaveLength(1);
  });

  it("CMT-FR-34: an error given by the owner renders inline as an alert", () => {
    render(<DiscussionComposer {...replyProps({ error: "discussion_locked" })} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});

describe("DiscussionComposer session state", () => {
  it("CVP-FR-47: unsent text and pending attachments survive an unmount and a remount", () => {
    const first = render(<DiscussionComposer {...replyProps()} />);
    type("half a sentence");
    act(() =>
      setComposerAttachments("d1", [
        {
          name: "spec.pdf",
          input: { kind: "url", url: "https://example.com/s", mediaType: "application/pdf" },
        },
      ]),
    );
    first.unmount();
    render(<DiscussionComposer {...replyProps()} />);
    expect(field().value).toBe("half a sentence");
    expect(screen.getByText("spec.pdf")).toBeInTheDocument();
  });

  it("CVP-FR-47: another owner surface for the same discussion reads the same text", () => {
    const first = render(<DiscussionComposer {...replyProps()} />);
    type("carried");
    first.unmount();
    render(<DiscussionComposer {...replyProps({ placeholder: "elsewhere" })} />);
    expect(field().value).toBe("carried");
  });

  it("CVP-FR-47: text typed in one discussion is not shown in another", () => {
    const first = render(<DiscussionComposer {...replyProps()} />);
    type("only d1");
    first.unmount();
    render(<DiscussionComposer {...replyProps({ discussion: discussion("d2") })} />);
    expect(field().value).toBe("");
  });

  it("CMT-FR-11: Cancel appears only while there is something to discard and clears it", () => {
    render(<DiscussionComposer {...replyProps()} />);
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    type("x");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(field().value).toBe("");
  });
});

describe("DiscussionComposer opening mode", () => {
  const target = { kind: "note", noteId: "n1" } as const;

  function opening(over: Record<string, unknown> = {}): DiscussionComposerProps {
    return {
      mode: "opening",
      target,
      agents: [],
      onOpen: vi.fn(async () => discussion("created")),
      ...over,
    } as DiscussionComposerProps;
  }

  it("CVP-FR-60: the opening post carries the target, the fragment, the body, and the attachments", async () => {
    const fragmentTarget = {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      start: 1,
      end: 4,
      quote: "abc",
    } as const;
    const onOpen = vi.fn(async () => discussion("created"));
    const onOpened = vi.fn();
    render(
      <DiscussionComposer
        {...opening({
          target: { kind: "artifact", artifactId: "a.md" },
          fragmentTarget,
          onOpen,
          onOpened,
        })}
      />,
    );
    type("start here");
    fireEvent.keyDown(field(), { key: "Enter", metaKey: true });
    await act(async () => {});
    expect(onOpen).toHaveBeenCalledWith({
      target: { kind: "artifact", artifactId: "a.md" },
      fragmentTarget,
      body: "start here",
      attachments: [],
    });
    expect(onOpened).toHaveBeenCalledWith(expect.objectContaining({ id: "created" }));
    expect(field().value).toBe("");
  });

  it("CVP-FR-64: an unposted opening composer keeps its text under the target key", () => {
    const first = render(<DiscussionComposer {...opening()} />);
    type("not yet");
    expect(getDiscussionSession("note:n1").body).toBe("not yet");
    first.unmount();
    render(<DiscussionComposer {...opening()} />);
    expect(field().value).toBe("not yet");
  });

  it("CVP-FR-40: a refused opening post keeps the text and shows the typed error", async () => {
    const onOpen = vi.fn(async () => {
      throw "not_supported";
    });
    render(<DiscussionComposer {...opening({ onOpen })} />);
    type("kept");
    fireEvent.keyDown(field(), { key: "Enter", ctrlKey: true });
    await act(async () => {});
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(field().value).toBe("kept");
  });

  it("CVP-FR-55: the discard control is unconditional where the owner asks for it", () => {
    const onCancel = vi.fn();
    render(<DiscussionComposer {...opening({ alwaysShowCancel: true, onCancel })} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});

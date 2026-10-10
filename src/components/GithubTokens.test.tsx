import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GithubTokens, tokenErrorMessage } from "./GithubTokens";
import { GITHUB_TOKEN_ERRORS, type GithubTokenRecord } from "../types";

// The section reaches the backend only through `invoke`; mock the bridge so
// jsdom never needs the Tauri runtime.
const invokeMock = vi.fn();
const promptMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function token(over: Partial<GithubTokenRecord> = {}): GithubTokenRecord {
  return {
    id: "t1",
    label: "work laptop",
    accountLogin: "raver119",
    scopes: ["repo", "workflow"],
    maskedHint: "a3f9",
    host: "github.com",
    addedAt: "2026-03-12T10:00:00Z",
    lastVerifiedAt: "2026-03-12T10:00:00Z",
    state: "valid",
    ...over,
  };
}

/** Wire `invoke` up to a token list plus any per-command overrides. */
function backend(
  tokens: GithubTokenRecord[],
  overrides: Record<string, (args?: Record<string, unknown>) => unknown> = {},
) {
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (overrides[cmd]) return overrides[cmd](args);
      switch (cmd) {
        case "list_github_tokens":
          return tokens;
        case "open_github_token_creation_page":
          return undefined;
        default:
          throw new Error(`unexpected command ${cmd}`);
      }
    },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  promptMock.mockReset();
  vi.stubGlobal("prompt", promptMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("GitHub section — listing and empty state", () => {
  it("GHA-FR-01, GHA-FR-04 renders a first-class empty state and asks the backend for the list", async () => {
    // GHA-FR-01 / GHA-FR-04: an empty registry is not an error.
    backend([]);
    render(<GithubTokens />);

    await waitFor(() =>
      expect(screen.getByTestId("github-tokens-empty")).toBeInTheDocument(),
    );
    expect(invokeMock).toHaveBeenCalledWith("list_github_tokens");
    expect(screen.queryByText(/✗/)).not.toBeInTheDocument();
    // The way out of the empty state is offered right there.
    expect(screen.getByRole("button", { name: /Add token/ })).toBeInTheDocument();
  });

  it("GHA-FR-02, GHA-FR-03 shows every field of a row and never more than the masked hint", async () => {
    // GHA-FR-02 / GHA-FR-03.
    backend([
      token(),
      token({
        id: "t2",
        label: "ci bot",
        accountLogin: "synthesis-ci",
        scopes: ["repo"],
        maskedHint: "7c21",
        state: "invalid",
        lastVerifiedAt: "2026-07-04T09:00:00Z",
      }),
    ]);
    render(<GithubTokens />);

    await waitFor(() =>
      expect(screen.getAllByTestId("github-token-row")).toHaveLength(2),
    );
    const [first, second] = screen.getAllByTestId("github-token-row");

    expect(within(first).getByText("work laptop")).toBeInTheDocument();
    expect(within(first).getByTestId("token-state")).toHaveTextContent("valid");
    expect(within(first).getByTestId("token-identity")).toHaveTextContent(
      "@raver119 · repo, workflow · ••••a3f9",
    );
    expect(within(second).getByTestId("token-state")).toHaveTextContent("invalid");
    // GHA-FR-02: the date the token was added is part of the row, and stays
    // there once the token has also been verified.
    expect(first).toHaveTextContent("added 2026-03-12");
    expect(second).toHaveTextContent("added 2026-03-12");
    expect(second).toHaveTextContent("checked 2026-07-04");

    // GHA-FR-03: the whole rendered section carries four characters of each
    // token and nothing more — there is no path by which a full one could
    // appear, because the backend never sends one.
    const rendered = screen.getByTestId("github-tokens").textContent ?? "";
    expect(rendered).toContain("a3f9");
    expect(rendered).not.toContain("ghp_");
  });

  it("renders a row whose secret has vanished as unavailable rather than dropping it", async () => {
    // GTS-FR-08 surfaced: the author needs a row they can remove deliberately.
    backend([token({ state: "unavailable" })]);
    render(<GithubTokens />);

    await waitFor(() =>
      expect(screen.getByTestId("token-state")).toHaveTextContent("unavailable"),
    );
    expect(screen.getByTestId("github-token-row")).toBeInTheDocument();
  });
});

describe("Add token dialog", () => {
  const openDialog = async () => {
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getByTestId("github-tokens-empty")).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /Add token/ }));
    return screen.getByRole("dialog");
  };

  it("GHA-FR-05, GHA-FR-06 walks to GitHub without closing or disturbing what is typed", async () => {
    // GHA-FR-06: the token comes back by paste, so the dialog must survive the
    // trip to the browser with its inputs intact.
    backend([]);
    const dialog = await openDialog();

    await userEvent.type(screen.getByLabelText(/^Label/), "work laptop");
    await userEvent.click(
      screen.getByRole("button", { name: /Open GitHub token page/ }),
    );

    expect(invokeMock).toHaveBeenCalledWith("open_github_token_creation_page", {
      host: "",
    });
    expect(dialog).toBeInTheDocument();
    expect(screen.getByLabelText(/^Label/)).toHaveValue("work laptop");
  });

  it("GHA-FR-05, GHA-FR-06 is one form: the hand-off to GitHub is always offered and gates nothing", async () => {
    // GHA-FR-05. A generate-versus-paste mode would have been decorative —
    // pasting was already possible without touching the generate step — while
    // hiding the hand-off from the author who has no token yet.
    backend([]);
    await openDialog();

    // The one field that must be filled, and the standing hand-off, are both
    // present at once with no mode to choose first.
    expect(screen.getByLabelText("Token")).toBeInTheDocument();
    expect(screen.getByLabelText(/^Label/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Open GitHub token page/ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("radio", { name: /Paste existing|Generate new/ }),
    ).not.toBeInTheDocument();

    // And a token can be pasted straight away, without any preceding step.
    await userEvent.type(screen.getByLabelText("Token"), "ghp_abc123");
    expect(screen.getByRole("button", { name: "Add" })).toBeEnabled();
  });

  it("GHA-FR-07, GTS-FR-05 requires a token and nothing else — the label is optional", async () => {
    // GHA-FR-07: the backend names an unlabelled token after the account it
    // verified it against (GTS-FR-05), so demanding a label up front would ask
    // the author for something the application is about to learn anyway.
    backend([]);
    await openDialog();
    const add = screen.getByRole("button", { name: "Add" });

    expect(add).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Token"), "ghp_abc123");
    expect(add).toBeEnabled();
    // Whitespace is not a token.
    await userEvent.clear(screen.getByLabelText("Token"));
    await userEvent.type(screen.getByLabelText("Token"), "   ");
    expect(add).toBeDisabled();
  });

  it("submits an empty label and shows the name the backend derived", async () => {
    // The account is the name the author would have typed; it comes back on the
    // created record rather than being guessed client-side.
    backend([], {
      add_github_token: (args) => {
        expect(args).toEqual({ label: "", secret: "ghp_good", host: "" });
        return token({ id: "new", label: "raver119" });
      },
    });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Token"), "ghp_good");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.getByTestId("github-token-row")).toHaveTextContent("raver119"),
    );
  });

  it("GHA-FR-08 stores the token and shows the record the backend resolved", async () => {
    // GHA-FR-08: the account and scopes on the new row come from the backend's
    // verification, not from anything the UI guessed.
    const created = token({ id: "new", label: "work" });
    backend([], { add_github_token: () => created });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Token"), "ghp_good");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(invokeMock).toHaveBeenCalledWith("add_github_token", {
      label: "work",
      secret: "ghp_good",
      host: "",
    });
    const row = screen.getByTestId("github-token-row");
    expect(within(row).getByText("work")).toBeInTheDocument();
    expect(within(row).getByTestId("token-identity")).toHaveTextContent("@raver119");
  });

  it("GHA-FR-PGPY: the domain field sits above the hand-off, shows the github.com placeholder, and is not prefilled", async () => {
    backend([]);
    const dialog = await openDialog();

    const domain = screen.getByLabelText("Domain");
    expect(domain).toHaveAttribute("placeholder", "github.com");
    expect(domain).toHaveValue("");
    const handOff = screen.getByRole("button", { name: /Open GitHub token page/ });
    expect(
      domain.compareDocumentPosition(handOff) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      domain.compareDocumentPosition(screen.getByLabelText("Token")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(dialog).toBeInTheDocument();
  });

  it("GHA-FR-PGPY, GHA-FR-07: an empty domain does not gate Add, and Add stays enabled by the secret alone", async () => {
    backend([]);
    await openDialog();
    expect(screen.getByRole("button", { name: "Add" })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Token"), "ghp_abc123");
    expect(screen.getByLabelText("Domain")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Add" })).toBeEnabled();
  });

  it("GHA-FR-PGPY, GHA-FR-08: submit sends the domain exactly as typed and the new row shows its domain", async () => {
    const created = token({ id: "new", label: "work", host: "company.ghe.com" });
    backend([], { add_github_token: () => created });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Domain"), " Company.GHE.com ");
    await userEvent.type(screen.getByLabelText("Token"), "ghp_good");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(invokeMock).toHaveBeenCalledWith("add_github_token", {
      label: "",
      secret: "ghp_good",
      host: " Company.GHE.com ",
    });
    expect(screen.getByTestId("token-host")).toHaveTextContent("company.ghe.com");
  });

  it("GHA-FR-FUSZ, GHA-FR-06: the hand-off sends the domain as typed and leaves the dialog open", async () => {
    backend([]);
    const dialog = await openDialog();

    await userEvent.type(screen.getByLabelText("Domain"), "company.ghe.com");
    await userEvent.click(
      screen.getByRole("button", { name: /Open GitHub token page/ }),
    );

    expect(invokeMock).toHaveBeenCalledWith("open_github_token_creation_page", {
      host: "company.ghe.com",
    });
    expect(dialog).toBeInTheDocument();
    expect(screen.getByLabelText("Domain")).toHaveValue("company.ghe.com");
  });

  it("GHA-FR-FUSZ, GHA-FR-OGNL: an invalid_host answer to the hand-off renders inline and keeps the dialog open", async () => {
    backend([], {
      open_github_token_creation_page: () => {
        throw GITHUB_TOKEN_ERRORS.invalidHost;
      },
    });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Domain"), "not a host");
    await userEvent.type(screen.getByLabelText("Token"), "ghp_keep");
    await userEvent.click(
      screen.getByRole("button", { name: /Open GitHub token page/ }),
    );

    expect(await screen.findByTestId("add-token-error")).toHaveTextContent(
      "That domain is not a valid host.",
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByLabelText("Domain")).toHaveValue("not a host");
    expect(screen.getByLabelText("Token")).toHaveValue("ghp_keep");
  });

  it("GHA-FR-OGNL: invalid_host on submit renders above the actions, keeps label and domain, and clears the secret", async () => {
    backend([], {
      add_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.invalidHost;
      },
    });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Domain"), "bad host");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.type(screen.getByLabelText("Token"), "ghp_secret");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    const error = await screen.findByTestId("add-token-error");
    expect(error).toHaveTextContent("That domain is not a valid host.");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByLabelText("Domain")).toHaveValue("bad host");
    expect(screen.getByLabelText(/^Label/)).toHaveValue("work");
    expect(screen.getByLabelText("Token")).toHaveValue("");
    expect(screen.queryByTestId("github-token-row")).not.toBeInTheDocument();
    // The error sits in the body, before the action row.
    const actions = screen.getByRole("button", { name: "Cancel" }).parentElement!;
    expect(
      error.compareDocumentPosition(actions) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("GHA-FR-10 keeps the dialog open on a rejection, preserving the label and clearing the token", async () => {
    // GHA-FR-10: a mistyped paste is retried without retyping the label.
    backend([], {
      add_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.invalidToken;
      },
    });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Token"), "ghp_bad");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.getByTestId("add-token-error")).toBeInTheDocument(),
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByTestId("add-token-error")).toHaveTextContent(
      /GitHub rejected that token/,
    );
    expect(screen.getByLabelText(/^Label/)).toHaveValue("work");
    expect(screen.getByLabelText("Token")).toHaveValue("");
    // Nothing was added to the list.
    expect(screen.queryByTestId("github-token-row")).not.toBeInTheDocument();
  });

  it("GHA-FR-09 retains no secret once a submission has resolved", async () => {
    // GHA-FR-09: the secret lives in the dialog's transient state and nowhere
    // else; closing and reopening must not resurrect it.
    backend([], {
      add_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.invalidToken;
      },
    });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Token"), "ghp_secret");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(screen.getByTestId("add-token-error")).toBeInTheDocument(),
    );
    expect(screen.getByLabelText("Token")).toHaveValue("");

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await userEvent.click(screen.getByRole("button", { name: /Add token/ }));
    expect(screen.getByLabelText("Token")).toHaveValue("");
    expect(screen.getByLabelText(/^Label/)).toHaveValue("");
  });

  it("GHA-FR-10 blames the network rather than the token when GitHub is unreachable", async () => {
    // GHA-FR-10: the two call for different responses, so they must not read
    // the same.
    backend([], {
      add_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.githubUnreachable;
      },
    });
    await openDialog();

    await userEvent.type(screen.getByLabelText("Token"), "ghp_x");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.getByTestId("add-token-error")).toHaveTextContent(
        /Could not reach GitHub/,
      ),
    );
    expect(screen.getByTestId("add-token-error")).not.toHaveTextContent(
      /rejected that token/,
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("distinguishes a keychain refusal from both of the above", async () => {
    backend([], {
      add_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.keychainUnavailable;
      },
    });
    await openDialog();
    await userEvent.type(screen.getByLabelText("Token"), "ghp_x");
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.getByTestId("add-token-error")).toHaveTextContent(
        /keychain is unavailable/,
      ),
    );
  });
});

describe("Token row domain", () => {
  it("GHA-FR-02: a github.com token shows no domain line", async () => {
    backend([token()]);
    render(<GithubTokens />);
    await screen.findByTestId("github-token-row");
    expect(screen.queryByTestId("token-host")).not.toBeInTheDocument();
  });

  it("GHA-FR-02: a missing or empty host reads as github.com and shows no domain line", async () => {
    backend([
      token({ id: "a", label: "empty", host: "" }),
      token({ id: "b", label: "missing", host: undefined as unknown as string }),
    ]);
    render(<GithubTokens />);
    await screen.findAllByTestId("github-token-row");
    expect(screen.queryByTestId("token-host")).not.toBeInTheDocument();
  });

  it("GHA-FR-02: a GitHub Enterprise token shows its domain on its own line below the identity line", async () => {
    backend([token({ host: "company.ghe.com" })]);
    render(<GithubTokens />);
    const row = await screen.findByTestId("github-token-row");
    const host = within(row).getByTestId("token-host");
    expect(host).toHaveTextContent("company.ghe.com");
    const identity = within(row).getByTestId("token-identity");
    expect(identity).not.toHaveTextContent("company.ghe.com");
    expect(
      identity.compareDocumentPosition(host) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

describe("Row actions", () => {
  it("GHA-FR-11 verifies a row and updates it in place", async () => {
    // GHA-FR-11: the row's state changes without the list being rebuilt.
    backend([token(), token({ id: "t2", label: "ci bot" })], {
      validate_github_token: () => token({ state: "invalid" }),
    });
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getAllByTestId("github-token-row")).toHaveLength(2),
    );

    await userEvent.click(screen.getAllByRole("button", { name: "Verify" })[0]);

    await waitFor(() =>
      expect(screen.getAllByTestId("token-state")[0]).toHaveTextContent("invalid"),
    );
    expect(invokeMock).toHaveBeenCalledWith("validate_github_token", { id: "t1" });
    // The other row is untouched and the list did not reorder.
    expect(screen.getAllByTestId("github-token-row")).toHaveLength(2);
    expect(screen.getAllByTestId("token-state")[1]).toHaveTextContent("valid");
  });

  it("GHA-FR-12 surfaces a colliding label and leaves the row's name alone", async () => {
    // GHA-FR-12: uniqueness is the backend's ruling (GTS-FR-05); the UI shows it.
    backend([token(), token({ id: "t2", label: "personal" })], {
      rename_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.duplicateLabel;
      },
    });
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getAllByTestId("github-token-row")).toHaveLength(2),
    );

    promptMock.mockReturnValue("work laptop");
    await userEvent.click(screen.getAllByRole("button", { name: "Rename" })[1]);

    await waitFor(() =>
      expect(screen.getByText(/Another token already uses that name/)).toBeInTheDocument(),
    );
    expect(screen.getAllByTestId("github-token-row")[1]).toHaveTextContent("personal");
  });

  it("GHA-FR-13 asks for confirmation before removing, and says what it costs", async () => {
    // GHA-FR-13.
    backend([token()], { remove_github_token: () => undefined });
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getByTestId("github-token-row")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: /Remove work laptop/ }));

    // Nothing has been removed yet — the confirmation comes first.
    expect(invokeMock).not.toHaveBeenCalledWith("remove_github_token", expect.anything());
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(/ask again which token to use/);

    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));

    await waitFor(() =>
      expect(screen.queryByTestId("github-token-row")).not.toBeInTheDocument(),
    );
    expect(invokeMock).toHaveBeenCalledWith("remove_github_token", { id: "t1" });
  });

  it("cancelling the removal confirmation removes nothing", async () => {
    backend([token()]);
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getByTestId("github-token-row")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: /Remove work laptop/ }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }),
    );

    expect(invokeMock).not.toHaveBeenCalledWith("remove_github_token", expect.anything());
    expect(screen.getByTestId("github-token-row")).toBeInTheDocument();
  });
});

describe("the secret never reaches the DOM (GHA-FR-03 / GHA-FR-09)", () => {
  // The fixtures elsewhere in this file contain no full secret at all, so an
  // assertion that the DOM lacks one cannot fail. These two drive a REAL secret
  // through the one component that ever holds one and check the rendered
  // document against it.
  const SECRET = "ghp_REAL_SECRET_VALUE_a3f9";

  const submitSecret = async () => {
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getByTestId("github-tokens-empty")).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /Add token/ }));
    await userEvent.type(screen.getByLabelText("Token"), SECRET);
    await userEvent.type(screen.getByLabelText(/^Label/), "work");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
  };

  it("is absent from the document after a successful add", async () => {
    backend([], { add_github_token: () => token({ label: "work" }) });
    await submitSecret();

    await waitFor(() =>
      expect(screen.getByTestId("github-token-row")).toBeInTheDocument(),
    );
    expect(document.body.innerHTML).not.toContain(SECRET);
    expect(document.body.innerHTML).not.toContain("ghp_REAL_SECRET_VALUE");
    // Only the masked hint survives.
    expect(screen.getByTestId("token-identity")).toHaveTextContent("••••a3f9");
  });

  it("is absent from the document after a rejected add", async () => {
    backend([], {
      add_github_token: () => {
        throw GITHUB_TOKEN_ERRORS.invalidToken;
      },
    });
    await submitSecret();

    await waitFor(() =>
      expect(screen.getByTestId("add-token-error")).toBeInTheDocument(),
    );
    // The dialog is still open — this is the moment the secret would linger.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(document.body.innerHTML).not.toContain(SECRET);
    expect(document.body.innerHTML).not.toContain("ghp_REAL_SECRET_VALUE");
  });

  it("is masked in the field while it is being typed", async () => {
    // Losing `type="password"` would render the secret in plaintext on screen
    // and in any screenshot or screen-share, and nothing else pins it.
    backend([]);
    render(<GithubTokens />);
    await waitFor(() =>
      expect(screen.getByTestId("github-tokens-empty")).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: /Add token/ }));

    expect(screen.getByLabelText("Token")).toHaveAttribute(
      "type",
      "password",
    );
  });
});

describe("tokenErrorMessage host errors", () => {
  it("GHA-FR-OGNL: invalid_host has its own text", () => {
    expect(tokenErrorMessage(GITHUB_TOKEN_ERRORS.invalidHost)).toBe(
      "That domain is not a valid host.",
    );
  });

  it("GHA-FR-LBLM: github_host_mismatch says the token belongs to another host and names the fix", () => {
    const text = tokenErrorMessage(GITHUB_TOKEN_ERRORS.hostMismatch);
    expect(text).toBe(
      "The project token belongs to another GitHub host than this remote. Pick or add a token for the host of the remote.",
    );
  });
});

describe("tokenErrorMessage", () => {
  it("maps every typed backend error to its own distinguishable text", () => {
    // GHA-FR-10 hinges on these not collapsing into one another.
    const messages = Object.values(GITHUB_TOKEN_ERRORS).map(tokenErrorMessage);
    expect(new Set(messages).size).toBe(messages.length);
    for (const m of messages) expect(m).not.toMatch(/^[a-z_]+$/);
  });

  it("passes an unrecognised error through rather than swallowing it", () => {
    expect(tokenErrorMessage("disk on fire")).toBe("disk on fire");
    expect(tokenErrorMessage(new Error("boom"))).toBe("boom");
    expect(tokenErrorMessage(undefined)).toBe("operation failed");
  });

  it("AAP-FR-LRTC: a refused certificate names the host and the cause", () => {
    const wire = "tls_untrusted:unknown_issuer:api.github.com";
    for (const rejection of [wire, `Error: ${wire}`, new Error(wire)]) {
      const text = tokenErrorMessage(rejection);
      expect(text).toContain("api.github.com");
      expect(text).toMatch(/issuer.*unknown/);
    }
  });
});

import { render, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Page from "./+page.svelte";
import type { Capture } from "$lib/captures/types";
import type { UnlistenFn } from "@tauri-apps/api/event";

function note(id: string, text: string): Capture {
  return {
    id,
    kind: "Note",
    created_at: new Date().toISOString(),
    payload: { text },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

describe("inbox page", () => {
  it("prepends a new Capture when captures.changed fires", async () => {
    const initial: Capture[] = [
      note("01H000000000000000000000A1", "older"),
    ];
    const listFn = vi.fn(async (_cursor: string | null, _limit: number) => initial);

    // Capture the listen handler so the test can fire a synthetic event.
    let fire: ((payload: Capture) => void) | null = null;
    const listenFn = vi.fn(
      async (
        _event: string,
        handler: (payload: Capture) => void,
      ): Promise<UnlistenFn> => {
        fire = handler;
        return () => {};
      },
    );

    const { findByText, queryByText } = render(Page, {
      props: { listFn, listenFn },
    });

    // First page loaded.
    expect(await findByText("older")).toBeTruthy();
    expect(queryByText("freshly captured")).toBeNull();

    // Wait for the listener to be registered before firing.
    await waitFor(() => expect(fire).not.toBeNull());
    fire!(note("01H000000000000000000000Z9", "freshly captured"));

    // New row appears.
    expect(await findByText("freshly captured")).toBeTruthy();
  });

  it("de-dups when the same id fires twice", async () => {
    const initial: Capture[] = [note("01H000000000000000000000A1", "older")];
    const listFn = vi.fn(async () => initial);
    let fire: ((payload: Capture) => void) | null = null;
    const listenFn = vi.fn(
      async (
        _event: string,
        handler: (payload: Capture) => void,
      ): Promise<UnlistenFn> => {
        fire = handler;
        return () => {};
      },
    );

    const { findAllByRole, findByText } = render(Page, {
      props: { listFn, listenFn },
    });

    await findByText("older");
    await waitFor(() => expect(fire).not.toBeNull());

    const dup = note("01H000000000000000000000Z9", "fresh");
    fire!(dup);
    await findByText("fresh");
    fire!(dup);

    const rows = await findAllByRole("option");
    expect(rows.length).toBe(2);
  });
});

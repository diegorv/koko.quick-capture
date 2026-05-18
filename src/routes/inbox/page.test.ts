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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
  };
}

describe("inbox page", () => {
  it("prepends a new Capture when captures.changed fires", async () => {
    const initial: Capture[] = [
      note("01H000000000000000000000A1", "older"),
    ];
    const listFn = vi.fn(async (_cursor: string | null, _limit: number) => initial);

    // Capture the captures:changed handler specifically — the page
    // also registers a navigation listener on `view:open_archive`
    // which we ignore here.
    let fire: ((payload: Capture) => void) | null = null;
    const listenFn = vi.fn(
      async (
        event: string,
        handler: (payload: Capture) => void,
      ): Promise<UnlistenFn> => {
        if (event === "captures:changed") fire = handler;
        return () => {};
      },
    );

    const invokeFn = vi.fn(async () => 0);
    const { findByText, queryByText } = render(Page, {
      props: { listFn, listenFn, invokeFn },
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
        event: string,
        handler: (payload: Capture) => void,
      ): Promise<UnlistenFn> => {
        if (event === "captures:changed") fire = handler;
        return () => {};
      },
    );

    const invokeFn = vi.fn(async () => 0);
    const { findAllByRole, findByText } = render(Page, {
      props: { listFn, listenFn, invokeFn },
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

  it("renders the 'Inbox zero. Nice.' empty state when totalCount > 0 but list is empty", async () => {
    const listFn = vi.fn(async () => [] as Capture[]);
    const listenFn = vi.fn(
      async (
        _event: string,
        _handler: (...args: never[]) => void,
      ): Promise<UnlistenFn> => () => {},
    );
    // total_count > 0 means there are routed/deleted Captures somewhere
    // — the Inbox is just drained.
    const invokeFn = vi.fn(async (cmd: string) =>
      cmd === "total_count" ? 5 : 0,
    );

    const { findByText } = render(Page, {
      props: { listFn, listenFn, invokeFn },
    });

    expect(await findByText("Inbox zero. Nice.")).toBeTruthy();
  });

  it("renders the cold-start empty state when totalCount is 0", async () => {
    const listFn = vi.fn(async () => [] as Capture[]);
    const listenFn = vi.fn(
      async (
        _event: string,
        _handler: (...args: never[]) => void,
      ): Promise<UnlistenFn> => () => {},
    );
    const invokeFn = vi.fn(async () => 0);

    const { findByText } = render(Page, {
      props: { listFn, listenFn, invokeFn },
    });

    expect(await findByText("No captures yet")).toBeTruthy();
  });

  it("navigates to /archive when view:open_archive fires (ADR-0010)", async () => {
    const gotoMod = await import("$app/navigation");
    const gotoSpy = vi.spyOn(gotoMod, "goto");

    const listFn = vi.fn(async () => [] as Capture[]);
    let fire: (() => void) | null = null;
    const listenFn = vi.fn(
      async (
        event: string,
        handler: (...args: never[]) => void,
      ): Promise<UnlistenFn> => {
        if (event === "view:open_archive") fire = () => handler();
        return () => {};
      },
    );
    const invokeFn = vi.fn(async () => 0);

    render(Page, { props: { listFn, listenFn, invokeFn } });

    await new Promise((r) => setTimeout(r, 0));
    expect(fire).not.toBeNull();
    fire!();
    expect(gotoSpy).toHaveBeenCalledWith("/archive");
  });
});

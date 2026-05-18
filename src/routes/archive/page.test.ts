import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import type { UnlistenFn } from "@tauri-apps/api/event";
import Page from "./+page.svelte";
import type { Capture, Destination } from "$lib/captures/types";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `tauri-fake://${p}`,
  invoke: vi.fn(),
}));
vi.mock("$app/navigation", () => ({
  goto: vi.fn(),
}));

function mkCapture(id: string, text: string, destId: string | null): Capture {
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
    destination_id: destId,
    routed_at: destId ? new Date().toISOString() : null,
  };
}

function mkDest(id: string, name: string, color: string | null = null): Destination {
  return {
    id,
    name,
    color,
    created_at: new Date().toISOString(),
    deleted_at: null,
  };
}

function makeInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  return vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    const handler = handlers[cmd];
    if (!handler) return null;
    return handler(args ?? {});
  });
}

function noopListen() {
  return vi.fn(
    async (_event: string, _handler: (payload: unknown) => void): Promise<UnlistenFn> => () => {},
  );
}

describe("archive page", () => {
  it("shows the empty state when no captures are routed", async () => {
    const invokeFn = makeInvoke({
      list_archive: () => [],
      list_destinations: () => [],
      inbox_count: () => 0,
    });

    const { findByText } = render(Page, {
      props: {
        invokeFn,
        listenFn: noopListen(),
        hideFn: vi.fn(async () => {}),
      },
    });

    expect(await findByText("Nothing routed yet")).toBeTruthy();
  });

  it("renders Routed captures and Destination chips with counts", async () => {
    const dests = [mkDest("D1", "Todoist", "red"), mkDest("D2", "Readwise")];
    const capt = [
      mkCapture("C1", "alpha", "D1"),
      mkCapture("C2", "bravo", "D1"),
      mkCapture("C3", "charlie", "D2"),
    ];
    const invokeFn = makeInvoke({
      list_archive: () => capt,
      list_destinations: () => dests,
      inbox_count: () => 0,
    });

    const { findAllByTestId, getByTestId } = render(Page, {
      props: {
        invokeFn,
        listenFn: noopListen(),
        hideFn: vi.fn(async () => {}),
      },
    });

    await findAllByTestId("filter-chip");
    const all = getByTestId("filter-all");
    expect(all.textContent).toContain("3");
    const chips = await findAllByTestId("filter-chip");
    expect(chips.length).toBe(2);
    expect(chips[0].textContent).toContain("Todoist");
    expect(chips[0].textContent).toContain("2");
    expect(chips[1].textContent).toContain("Readwise");
    expect(chips[1].textContent).toContain("1");
  });

  it("filters the visible list when a destination chip is clicked", async () => {
    const dests = [mkDest("D1", "Todoist"), mkDest("D2", "Readwise")];
    const capt = [
      mkCapture("C1", "alpha", "D1"),
      mkCapture("C2", "bravo", "D2"),
    ];
    const invokeFn = makeInvoke({
      list_archive: () => capt,
      list_destinations: () => dests,
      inbox_count: () => 0,
    });

    const { findAllByRole, findAllByTestId } = render(Page, {
      props: {
        invokeFn,
        listenFn: noopListen(),
        hideFn: vi.fn(async () => {}),
      },
    });

    // Both rows visible by default
    const initial = await findAllByRole("option");
    expect(initial.length).toBe(2);

    const chips = await findAllByTestId("filter-chip");
    // Click the Todoist chip (index 0 alpha).
    await fireEvent.click(chips[0]);

    await waitFor(async () => {
      const after = await findAllByRole("option");
      expect(after.length).toBe(1);
      expect(after[0].textContent).toContain("alpha");
    });
  });

  it("surfaces the soft-deleted-destination hint when an orphaned capture is present", async () => {
    const dests: Destination[] = []; // No live destinations.
    const capt = [mkCapture("C1", "orphan", "DGHOST")];
    const invokeFn = makeInvoke({
      list_archive: () => capt,
      list_destinations: () => dests,
      inbox_count: () => 0,
    });

    const { findByTestId } = render(Page, {
      props: {
        invokeFn,
        listenFn: noopListen(),
        hideFn: vi.fn(async () => {}),
      },
    });

    expect(await findByTestId("filter-deleted-hint")).toBeTruthy();
  });
});

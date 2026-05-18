import { describe, expect, it, vi } from "vitest";
import { flushSync } from "svelte";
import { createPaginatedList } from "./paginated-list.svelte";
import type { Capture } from "./types";

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

describe("createPaginatedList", () => {
  it("loadNext appends pages until exhausted (page < pageSize)", async () => {
    const pages: Capture[][] = [
      [note("A", "a"), note("B", "b"), note("C", "c")],
      [note("D", "d")],
    ];
    let call = 0;
    const pageFn = vi.fn(async () => pages[call++] ?? []);

    const pager = createPaginatedList({
      pageFn,
      cursorOf: (c) => c.id,
      pageSize: 3,
    });

    await pager.loadNext();
    expect(pager.items.map((c) => c.id)).toEqual(["A", "B", "C"]);
    expect(pager.exhausted).toBe(false);

    await pager.loadNext();
    expect(pager.items.map((c) => c.id)).toEqual(["A", "B", "C", "D"]);
    expect(pager.exhausted).toBe(true);

    // Further calls are no-ops once exhausted.
    await pager.loadNext();
    expect(pageFn).toHaveBeenCalledTimes(2);
  });

  it("passes the cursorOf result of the last item to pageFn", async () => {
    const pageFn = vi
      .fn<(c: string | null, n: number) => Promise<Capture[]>>()
      .mockResolvedValueOnce([note("X1", "x"), note("X2", "y")])
      .mockResolvedValueOnce([]);

    const pager = createPaginatedList({
      pageFn,
      cursorOf: (c) => `cursor-${c.id}`,
      pageSize: 2,
    });

    await pager.loadNext();
    expect(pageFn).toHaveBeenLastCalledWith(null, 2);
    await pager.loadNext();
    expect(pageFn).toHaveBeenLastCalledWith("cursor-X2", 2);
  });

  it("refetchFirst resets the list with a fresh first page", async () => {
    const pageFn = vi
      .fn<(c: string | null, n: number) => Promise<Capture[]>>()
      .mockResolvedValueOnce([note("A", "a"), note("B", "b")])
      .mockResolvedValueOnce([note("Z", "z")]);

    const pager = createPaginatedList({
      pageFn,
      cursorOf: (c) => c.id,
      pageSize: 5,
    });

    await pager.loadNext();
    expect(pager.items.map((c) => c.id)).toEqual(["A", "B"]);

    await pager.refetchFirst();
    expect(pager.items.map((c) => c.id)).toEqual(["Z"]);
    // < pageSize triggers exhausted on refetch too.
    expect(pager.exhausted).toBe(true);
  });

  it("prepend de-dups by id", async () => {
    const pager = createPaginatedList({
      pageFn: vi.fn(async () => []),
      cursorOf: (c) => c.id,
      pageSize: 5,
    });
    pager.prepend(note("A", "a"));
    pager.prepend(note("A", "dup"));
    pager.prepend(note("B", "b"));
    expect(pager.items.map((c) => c.id)).toEqual(["B", "A"]);
  });

  it("remove drops the item by id", async () => {
    const pager = createPaginatedList({
      pageFn: vi.fn(async () => []),
      cursorOf: (c) => c.id,
      pageSize: 5,
    });
    pager.setItems([note("A", "a"), note("B", "b"), note("C", "c")]);
    pager.remove("B");
    expect(pager.items.map((c) => c.id)).toEqual(["A", "C"]);
  });

  it("onScroll triggers loadNext when near the bottom", async () => {
    const pageFn = vi.fn(async () => [] as Capture[]);
    const pager = createPaginatedList({
      pageFn,
      cursorOf: (c) => c.id,
      pageSize: 5,
      scrollThresholdPx: 50,
    });

    const near = {
      currentTarget: { scrollHeight: 1000, scrollTop: 951, clientHeight: 0 },
    } as unknown as Event;
    pager.onScroll(near);
    flushSync();
    expect(pageFn).toHaveBeenCalled();
  });

  it("onScroll is a no-op far from the bottom", () => {
    const pageFn = vi.fn(async () => [] as Capture[]);
    const pager = createPaginatedList({
      pageFn,
      cursorOf: (c) => c.id,
      pageSize: 5,
      scrollThresholdPx: 50,
    });

    const far = {
      currentTarget: { scrollHeight: 1000, scrollTop: 100, clientHeight: 200 },
    } as unknown as Event;
    pager.onScroll(far);
    expect(pageFn).not.toHaveBeenCalled();
  });
});

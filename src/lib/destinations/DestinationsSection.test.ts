import { fireEvent, render, waitFor, within } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import type { UnlistenFn } from "@tauri-apps/api/event";
import DestinationsSection from "./DestinationsSection.svelte";
import type { Destination } from "$lib/captures/types";

function mkDest(
  id: string,
  name: string,
  color: string | null = null,
  deleted = false,
): Destination {
  return {
    id,
    name,
    color,
    created_at: new Date().toISOString(),
    deleted_at: deleted ? new Date().toISOString() : null,
  };
}

function makeInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  return vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unmocked invoke: ${cmd}`);
    return handler(args ?? {});
  });
}

function noopListen(): (event: string, handler: () => void) => Promise<UnlistenFn> {
  return vi.fn(async () => () => {});
}

describe("DestinationsSection", () => {
  it("renders live destinations in the order the backend returned them", async () => {
    // The Rust list_destinations command returns rows alpha-sorted. The
    // component preserves that order; the test mocks the backend by
    // handing rows back pre-sorted.
    const live: Destination[] = [
      mkDest("01H0001", "Readwise", "teal"),
      mkDest("01H0003", "Reference", null),
      mkDest("01H0002", "Todoist", "red"),
    ];
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      list_deleted_destinations: () => [],
    });

    const { findAllByTestId } = render(DestinationsSection, {
      props: { invokeFn, listenFn: noopListen() },
    });

    const rows = await findAllByTestId("destination-row");
    expect(rows.map((r) => r.textContent?.trim().split(/\s+/)[0])).toEqual([
      "Readwise",
      "Reference",
      "Todoist",
    ]);
  });

  it("creates a new destination via the inline form", async () => {
    const live: Destination[] = [];
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      list_deleted_destinations: () => [],
      create_destination: (args) => {
        const created = mkDest(
          "01H0009",
          (args.name as string),
          (args.color as string | null) ?? null,
        );
        live.push(created);
        return created;
      },
    });

    const { getByTestId, findAllByTestId } = render(DestinationsSection, {
      props: { invokeFn, listenFn: noopListen() },
    });

    // Open the form
    await fireEvent.click(getByTestId("new-destination-btn"));
    const form = getByTestId("create-form");
    const input = within(form).getByTestId("create-name-input") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "Todoist" } });

    // Pick the "red" swatch
    const red = within(form).getByLabelText("red");
    await fireEvent.click(red);

    // Submit
    await fireEvent.click(within(form).getByText("Save"));

    const rows = await findAllByTestId("destination-row");
    expect(rows.length).toBe(1);
    expect(rows[0].textContent).toContain("Todoist");
    expect(invokeFn).toHaveBeenCalledWith("create_destination", {
      name: "Todoist",
      color: "red",
    });
  });

  it("surfaces the create error and keeps the form open on conflict", async () => {
    const invokeFn = makeInvoke({
      list_destinations: () => [],
      list_deleted_destinations: () => [],
      create_destination: () => {
        throw "destination name already in use: Todoist";
      },
    });

    const { getByTestId, findByTestId } = render(DestinationsSection, {
      props: { invokeFn, listenFn: noopListen() },
    });

    await fireEvent.click(getByTestId("new-destination-btn"));
    const form = getByTestId("create-form");
    const input = within(form).getByTestId("create-name-input") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "Todoist" } });
    await fireEvent.click(within(form).getByText("Save"));

    const errorBox = await findByTestId("destinations-error");
    expect(errorBox.textContent).toContain("already in use");
    // Form is still open so the user can retry.
    expect(getByTestId("create-form")).toBeTruthy();
  });

  it("asks for confirmation before delete and only fires invoke on confirm", async () => {
    const live: Destination[] = [mkDest("01H0001", "Todoist", "red")];
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      list_deleted_destinations: () => [],
      soft_delete_destination: () => {
        live.length = 0;
        return null;
      },
    });

    const { findAllByTestId, findByTestId, queryByTestId } = render(
      DestinationsSection,
      { props: { invokeFn, listenFn: noopListen() } },
    );

    const rows = await findAllByTestId("destination-row");
    const deleteBtn = within(rows[0]).getByTestId("delete-btn");
    await fireEvent.click(deleteBtn);

    // Confirm bar is visible; invoke has not been called for delete yet.
    expect(await findByTestId("delete-confirm")).toBeTruthy();
    expect(invokeFn).not.toHaveBeenCalledWith(
      "soft_delete_destination",
      expect.anything(),
    );

    await fireEvent.click(
      within(await findByTestId("delete-confirm")).getByTestId(
        "delete-confirm-btn",
      ),
    );

    expect(invokeFn).toHaveBeenCalledWith("soft_delete_destination", {
      id: "01H0001",
    });
    await waitFor(() =>
      expect(queryByTestId("destination-row")).toBeNull(),
    );
  });

  it("renames a destination via the inline edit form", async () => {
    const live: Destination[] = [mkDest("01H0001", "Old", "red")];
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      list_deleted_destinations: () => [],
      update_destination: (args) => {
        const target = live.find((d) => d.id === args.id);
        if (target) {
          target.name = args.name as string;
          target.color = (args.color as string | null) ?? null;
        }
        return null;
      },
    });

    const { findAllByTestId, findByText } = render(DestinationsSection, {
      props: { invokeFn, listenFn: noopListen() },
    });

    const rows = await findAllByTestId("destination-row");
    await fireEvent.click(within(rows[0]).getByTestId("edit-btn"));

    const input = rows[0].querySelector("input.name-input") as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "New" } });
    await fireEvent.click(within(rows[0]).getByText("Save"));

    expect(await findByText("New")).toBeTruthy();
    expect(invokeFn).toHaveBeenCalledWith("update_destination", {
      id: "01H0001",
      name: "New",
      color: "red",
    });
  });

  it("lists soft-deleted destinations under a toggle and restores them", async () => {
    const live: Destination[] = [];
    const deleted: Destination[] = [mkDest("01H9999", "Old", null, true)];
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      list_deleted_destinations: () => deleted,
      restore_destination: (args) => {
        const idx = deleted.findIndex((d) => d.id === args.id);
        if (idx >= 0) {
          const [row] = deleted.splice(idx, 1);
          row.deleted_at = null;
          live.push(row);
        }
        return null;
      },
    });

    const { findByTestId, queryByTestId } = render(DestinationsSection, {
      props: { invokeFn, listenFn: noopListen() },
    });

    const block = await findByTestId("deleted-block");
    await fireEvent.click(within(block).getByText(/Soft-deleted/));
    const deletedRow = await findByTestId("deleted-row");
    await fireEvent.click(within(deletedRow).getByTestId("restore-btn"));

    await waitFor(() => expect(queryByTestId("deleted-row")).toBeNull());
    expect(invokeFn).toHaveBeenCalledWith("restore_destination", {
      id: "01H9999",
    });
  });

  it("refetches when destinations:changed event fires", async () => {
    const state: Destination[] = [];
    const invokeFn = makeInvoke({
      list_destinations: () => [...state],
      list_deleted_destinations: () => [],
    });

    let fire: (() => void) | null = null;
    const listenFn = vi.fn(async (_event: string, handler: () => void) => {
      fire = handler;
      return () => {};
    });

    const { queryAllByTestId, findAllByTestId } = render(DestinationsSection, {
      props: { invokeFn, listenFn },
    });

    await waitFor(() => expect(invokeFn).toHaveBeenCalledWith("list_destinations"));
    expect(queryAllByTestId("destination-row").length).toBe(0);

    state.push(mkDest("01H0001", "Todoist", "red"));
    await waitFor(() => expect(fire).not.toBeNull());
    fire!();

    const rows = await findAllByTestId("destination-row");
    expect(rows.length).toBe(1);
  });
});

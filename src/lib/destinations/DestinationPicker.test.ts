import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import DestinationPicker from "./DestinationPicker.svelte";
import type { Destination } from "$lib/captures/types";

function mkDest(id: string, name: string, color: string | null = null): Destination {
  return {
    id,
    name,
    color,
    created_at: new Date().toISOString(),
    deleted_at: null,
    kind: "label",
    config: null,
  };
}

function mkKokoDest(id: string, name: string, vault: string): Destination {
  return {
    id,
    name,
    color: null,
    created_at: new Date().toISOString(),
    deleted_at: null,
    kind: "kokobrain",
    config: JSON.stringify({ vault }),
  };
}

function makeInvoke(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  return vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unmocked invoke: ${cmd}`);
    return handler(args ?? {});
  });
}

describe("DestinationPicker", () => {
  it("loads destinations on open and assigns on Enter", async () => {
    const live = [mkDest("01H001", "Todoist", "red"), mkDest("01H002", "Readwise", "teal")];
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
    });
    const onClose = vi.fn();
    const onAssigned = vi.fn();

    const { getByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose,
        onAssigned,
      },
    });

    const input = await waitFor(() => getByTestId("picker-search"));
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(routeSpy).toHaveBeenCalled());
    expect(routeSpy).toHaveBeenCalledWith({
      id: "01HCAP",
      destinationId: "01H001",
    });
    expect(onAssigned).toHaveBeenCalledWith("01H001");
    expect(onClose).toHaveBeenCalled();
  });

  it("ArrowDown then Enter assigns the second destination", async () => {
    const live = [mkDest("01H001", "Todoist"), mkDest("01H002", "Readwise")];
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
    });

    const { getByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });
    const input = await waitFor(() => getByTestId("picker-search"));
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(routeSpy).toHaveBeenCalledWith({
        id: "01HCAP",
        destinationId: "01H002",
      }),
    );
  });

  it("auto-opens in create mode when zero live destinations exist", async () => {
    const invokeFn = makeInvoke({
      list_destinations: () => [],
      create_destination: (args) =>
        mkDest("01HNEW", args.name as string, (args.color as string | null) ?? null),
      route_capture: () => null,
    });
    const onAssigned = vi.fn();

    const { getByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned,
      },
    });

    const createInput = await waitFor(() => getByTestId("picker-create-input"));
    await fireEvent.input(createInput, { target: { value: "Todoist" } });
    await fireEvent.keyDown(createInput, { key: "Enter" });

    await waitFor(() => expect(onAssigned).toHaveBeenCalledWith("01HNEW"));
    expect(invokeFn).toHaveBeenCalledWith("create_destination", {
      name: "Todoist",
      color: null,
    });
    expect(invokeFn).toHaveBeenCalledWith("route_capture", {
      id: "01HCAP",
      destinationId: "01HNEW",
    });
  });

  it("Escape closes without assigning", async () => {
    const onClose = vi.fn();
    const onAssigned = vi.fn();
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => [mkDest("01H001", "Todoist")],
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
    });

    const { getByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose,
        onAssigned,
      },
    });
    const input = await waitFor(() => getByTestId("picker-search"));
    await fireEvent.keyDown(input, { key: "Escape" });

    expect(onClose).toHaveBeenCalled();
    expect(routeSpy).not.toHaveBeenCalled();
    expect(onAssigned).not.toHaveBeenCalled();
  });

  it("Cmd+N switches list mode to create mode", async () => {
    const invokeFn = makeInvoke({
      list_destinations: () => [mkDest("01H001", "Todoist")],
    });

    const { getByTestId, findByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });

    const input = await waitFor(() => getByTestId("picker-search"));
    await fireEvent.keyDown(input, { key: "n", metaKey: true });

    expect(await findByTestId("picker-create")).toBeTruthy();
  });

  it("typing filters the destination list", async () => {
    const live = [
      mkDest("01H001", "Todoist"),
      mkDest("01H002", "Readwise"),
      mkDest("01H003", "Notes"),
    ];
    const invokeFn = makeInvoke({
      list_destinations: () => live,
    });

    const { getByTestId, findAllByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });

    const input = (await waitFor(() => getByTestId("picker-search"))) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "read" } });

    const results = await findAllByTestId("picker-result");
    expect(results.length).toBe(1);
    expect(results[0].textContent).toContain("Readwise");
  });

  it("pre-selects the current destination on re-route", async () => {
    const live = [
      mkDest("01H001", "Todoist"),
      mkDest("01H002", "Readwise"),
      mkDest("01H003", "Notes"),
    ];
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
    });

    const { getByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        currentDestinationId: "01H002",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });

    const input = await waitFor(() => getByTestId("picker-search"));
    // Without arrow nav, Enter should hit the pre-selected dest.
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(routeSpy).toHaveBeenCalledWith({
        id: "01HCAP",
        destinationId: "01H002",
      }),
    );
  });

  it("routes Shot captures to KokoBrain via route_to_kokobrain", async () => {
    const live = [mkKokoDest("01HKB", "Personal Brain", "Personal")];
    const kokobrainSpy = vi.fn();
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
      route_to_kokobrain: (args) => {
        kokobrainSpy(args);
        return null;
      },
    });

    const { findAllByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });

    const rows = await findAllByTestId("picker-result");
    await fireEvent.click(rows[0]);
    await waitFor(() =>
      expect(kokobrainSpy).toHaveBeenCalledWith({
        id: "01HCAP",
        destinationId: "01HKB",
      }),
    );
    expect(routeSpy).not.toHaveBeenCalled();
  });

  it("routes Note captures to KokoBrain via route_to_kokobrain", async () => {
    const live = [mkKokoDest("01HKB", "Personal Brain", "Personal")];
    const kokobrainSpy = vi.fn();
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
      route_to_kokobrain: (args) => {
        kokobrainSpy(args);
        return null;
      },
    });

    const { findAllByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });

    const rows = await findAllByTestId("picker-result");
    expect(rows[0].getAttribute("data-disabled")).toBeNull();
    await fireEvent.click(rows[0]);
    await waitFor(() =>
      expect(kokobrainSpy).toHaveBeenCalledWith({
        id: "01HCAP",
        destinationId: "01HKB",
      }),
    );
    expect(routeSpy).not.toHaveBeenCalled();
  });

  it("routes to a label destination via route_capture", async () => {
    const live = [mkDest("01HLBL", "Todoist", "red")];
    const kokobrainSpy = vi.fn();
    const routeSpy = vi.fn();
    const invokeFn = makeInvoke({
      list_destinations: () => live,
      route_capture: (args) => {
        routeSpy(args);
        return null;
      },
      route_to_kokobrain: (args) => {
        kokobrainSpy(args);
        return null;
      },
    });

    const { findAllByTestId } = render(DestinationPicker, {
      props: {
        open: true,
        captureId: "01HCAP",
        invokeFn,
        onClose: () => {},
        onAssigned: () => {},
      },
    });

    const rows = await findAllByTestId("picker-result");
    await fireEvent.click(rows[0]);
    await waitFor(() =>
      expect(routeSpy).toHaveBeenCalledWith({
        id: "01HCAP",
        destinationId: "01HLBL",
      }),
    );
    expect(kokobrainSpy).not.toHaveBeenCalled();
  });
});

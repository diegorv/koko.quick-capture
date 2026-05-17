import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import { describe, it, expect, vi } from "vitest";
import Dock from "./Dock.svelte";

describe("Dock", () => {
  it("clicking the body calls onComposer", async () => {
    const onComposer = vi.fn();
    const onContextMenu = vi.fn();
    const { getByLabelText } = render(Dock, {
      props: { onComposer, onContextMenu },
    });

    const dock = getByLabelText("Open Composer");
    await fireEvent.click(dock);

    expect(onComposer).toHaveBeenCalledTimes(1);
    expect(onContextMenu).not.toHaveBeenCalled();
  });

  it("right-click calls onContextMenu with the event coords and suppresses the native menu", async () => {
    const onComposer = vi.fn();
    const onContextMenu = vi.fn();
    const { getByLabelText } = render(Dock, {
      props: { onComposer, onContextMenu },
    });

    const dock = getByLabelText("Open Composer");

    // jsdom doesn't synthesize a real MouseEvent default action, but
    // `fireEvent.contextMenu` does dispatch a cancelable event so we
    // can assert defaultPrevented on the returned boolean.
    const dispatched = await fireEvent.contextMenu(dock, {
      clientX: 123,
      clientY: 456,
    });

    expect(onContextMenu).toHaveBeenCalledTimes(1);
    expect(onContextMenu).toHaveBeenCalledWith(123, 456);
    expect(onComposer).not.toHaveBeenCalled();
    // fireEvent returns false when preventDefault was called on a
    // cancelable event — i.e. the native menu would be suppressed.
    expect(dispatched).toBe(false);
  });

  it("renders the drag-active class when dragActive is true", () => {
    const { getByLabelText } = render(Dock, {
      props: {
        onComposer: vi.fn(),
        onContextMenu: vi.fn(),
        dragActive: true,
      },
    });

    const dock = getByLabelText("Open Composer");
    expect(dock.classList.contains("drag-active")).toBe(true);
  });

  it("omits the drag-active class when dragActive is false", () => {
    const { getByLabelText } = render(Dock, {
      props: {
        onComposer: vi.fn(),
        onContextMenu: vi.fn(),
        dragActive: false,
      },
    });

    const dock = getByLabelText("Open Composer");
    expect(dock.classList.contains("drag-active")).toBe(false);
  });

  it("renders the unread count when unread > 0", () => {
    const { getByTestId } = render(Dock, {
      props: {
        onComposer: vi.fn(),
        onContextMenu: vi.fn(),
        unread: 5,
      },
    });

    const badge = getByTestId("dock-badge");
    expect(badge.textContent).toBe("5");
  });

  it("hides the badge when unread is zero", () => {
    const { queryByTestId } = render(Dock, {
      props: {
        onComposer: vi.fn(),
        onContextMenu: vi.fn(),
        unread: 0,
      },
    });

    expect(queryByTestId("dock-badge")).toBeNull();
  });

  it("renders 99+ when unread exceeds 99", () => {
    const { getByTestId } = render(Dock, {
      props: {
        onComposer: vi.fn(),
        onContextMenu: vi.fn(),
        unread: 150,
      },
    });

    expect(getByTestId("dock-badge").textContent).toBe("99+");
  });

  it("applies the pulse class on pulseKey change", async () => {
    const { getByLabelText, rerender } = render(Dock, {
      props: {
        onComposer: vi.fn(),
        onContextMenu: vi.fn(),
        pulseKey: 0,
      },
    });

    const dock = getByLabelText("Open Composer");
    // Initial render must not pulse — the user only sees the
    // animation when a capture lands, not on mount.
    expect(dock.classList.contains("pulse")).toBe(false);

    await rerender({
      onComposer: vi.fn(),
      onContextMenu: vi.fn(),
      pulseKey: 1,
    });
    // Effect schedules toggle across a microtask; wait two ticks so
    // the class has settled into its post-bump state.
    await tick();
    await tick();
    expect(dock.classList.contains("pulse")).toBe(true);
  });
});

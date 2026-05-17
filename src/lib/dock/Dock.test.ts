import { render, fireEvent } from "@testing-library/svelte";
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
});

import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import Composer from "./Composer.svelte";

describe("Composer", () => {
  it("autofocuses the textarea on mount", () => {
    const { getByLabelText } = render(Composer, {
      props: { save: vi.fn(), onclose: vi.fn() },
    });
    const textarea = getByLabelText("Note text");
    expect(document.activeElement).toBe(textarea);
  });

  it("ESC emits a close event without calling save", async () => {
    const save = vi.fn();
    const onclose = vi.fn();
    const { getByLabelText } = render(Composer, {
      props: { save, onclose },
    });
    const textarea = getByLabelText("Note text");
    await fireEvent.keyDown(textarea, { key: "Escape" });

    expect(save).not.toHaveBeenCalled();
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("Cmd+Enter calls save with the current text and emits close", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const onclose = vi.fn();
    const { getByLabelText } = render(Composer, {
      props: { save, onclose },
    });
    const textarea = getByLabelText("Note text") as HTMLTextAreaElement;

    await fireEvent.input(textarea, { target: { value: "hello capture" } });
    await fireEvent.keyDown(textarea, { key: "Enter", metaKey: true });

    // `save` is awaited synchronously inside the handler, so it lands
    // by the next microtask. `onclose` fires after the ~180ms save
    // flash; wait for it instead of asserting synchronously.
    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith("hello capture");
    await waitFor(() => expect(onclose).toHaveBeenCalledTimes(1));
  });

  it("a fresh mount starts with an empty textarea (no leakage)", () => {
    const first = render(Composer, {
      props: { save: vi.fn(), onclose: vi.fn() },
    });
    const firstTextarea = first.getByLabelText("Note text") as HTMLTextAreaElement;
    fireEvent.input(firstTextarea, { target: { value: "leftover" } });
    expect(firstTextarea.value).toBe("leftover");
    first.unmount();

    const second = render(Composer, {
      props: { save: vi.fn(), onclose: vi.fn() },
    });
    const secondTextarea = second.getByLabelText("Note text") as HTMLTextAreaElement;
    expect(secondTextarea.value).toBe("");
  });
});

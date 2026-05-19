import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import type { EditorView } from "@codemirror/view";
import Composer from "./Composer.svelte";

// CM exposes `view.contentDOM` (the contenteditable) as the focus +
// keydown target. Tests dispatch through that element so they exercise
// the same path the user does.

async function renderWithView(props: {
  save: (text: string) => void | Promise<void>;
  onclose?: () => void;
  focusKey?: number;
}) {
  let captured: EditorView | undefined;
  const result = render(Composer, {
    props: {
      ...props,
      oneditorReady: (v: EditorView) => {
        captured = v;
      },
    },
  });
  await waitFor(() => {
    if (!captured) throw new Error("editor not ready");
  });
  return { ...result, view: captured! };
}

describe("Composer", () => {
  it("autofocuses the editor on mount", async () => {
    const { view } = await renderWithView({
      save: vi.fn(),
      onclose: vi.fn(),
    });
    expect(view.hasFocus).toBe(true);
  });

  it("ESC emits close without calling save", async () => {
    const save = vi.fn();
    const onclose = vi.fn();
    const { view } = await renderWithView({ save, onclose });

    await fireEvent.keyDown(view.contentDOM, { key: "Escape" });

    expect(save).not.toHaveBeenCalled();
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("Cmd+Enter calls save with the current text and emits close", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const onclose = vi.fn();
    const { view } = await renderWithView({ save, onclose });

    view.dispatch({ changes: { from: 0, insert: "hello capture" } });
    // CM's `Mod-Enter` resolves to Meta-Enter on Mac and Ctrl-Enter
    // elsewhere; jsdom's userAgent puts CM into non-Mac mode so the
    // test event must carry ctrlKey (alone — sending both flags
    // does not match Mod's exact-one-modifier rule).
    await fireEvent.keyDown(view.contentDOM, {
      key: "Enter",
      ctrlKey: true,
    });

    expect(save).toHaveBeenCalledTimes(1);
    expect(save).toHaveBeenCalledWith("hello capture");
    // `onclose` fires after the ~180ms save flash; wait for it.
    await waitFor(() => expect(onclose).toHaveBeenCalledTimes(1));
  });

  it("a fresh mount starts with an empty doc (no leakage)", async () => {
    const first = await renderWithView({ save: vi.fn(), onclose: vi.fn() });
    first.view.dispatch({ changes: { from: 0, insert: "leftover" } });
    expect(first.view.state.doc.toString()).toBe("leftover");
    first.unmount();

    const second = await renderWithView({ save: vi.fn(), onclose: vi.fn() });
    expect(second.view.state.doc.toString()).toBe("");
  });

  it("bumping focusKey clears the doc and refocuses", async () => {
    const { view, rerender } = await renderWithView({
      save: vi.fn(),
      onclose: vi.fn(),
      focusKey: 0,
    });
    view.dispatch({ changes: { from: 0, insert: "stale draft" } });
    expect(view.state.doc.toString()).toBe("stale draft");

    await rerender({
      save: vi.fn(),
      onclose: vi.fn(),
      focusKey: 1,
    });

    expect(view.state.doc.toString()).toBe("");
    expect(view.hasFocus).toBe(true);
  });
});

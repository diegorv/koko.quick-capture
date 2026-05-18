import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import WikilinkFolderSection from "./WikilinkFolderSection.svelte";

function makeInvoke(
  handlers: Record<string, (args: Record<string, unknown>) => unknown>,
) {
  return vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unmocked invoke: ${cmd}`);
    return handler(args ?? {});
  });
}

describe("WikilinkFolderSection", () => {
  it("renders 'Not set' when the folder is unset", async () => {
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => null,
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() => {
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "Not set",
      );
    });
    // No Clear button when the value is unset.
    expect(() => getByTestId("wikilink-clear-btn")).toThrow();
  });

  it("renders the configured path and offers Clear when set", async () => {
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => "/Users/me/people",
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() => {
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "/Users/me/people",
      );
    });
    expect(getByTestId("wikilink-clear-btn")).toBeTruthy();
  });

  it("opens the picker, persists the chosen path, and refreshes the display", async () => {
    let stored: string | null = null;
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => stored,
      pick_wikilink_source_folder: () => "/Users/me/people",
      set_wikilink_source_folder: (args) => {
        stored = (args.path as string | null) ?? null;
        return null;
      },
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "Not set",
      ),
    );

    await fireEvent.click(getByTestId("wikilink-choose-btn"));

    await waitFor(() => {
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "/Users/me/people",
      );
    });
    // Verify the set_ call carried the picked path.
    expect(invokeFn).toHaveBeenCalledWith("set_wikilink_source_folder", {
      path: "/Users/me/people",
    });
  });

  it("leaves the existing value alone when the user cancels the picker", async () => {
    let setCalls = 0;
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => "/Users/me/people",
      pick_wikilink_source_folder: () => null, // user cancelled
      set_wikilink_source_folder: () => {
        setCalls += 1;
        return null;
      },
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "/Users/me/people",
      ),
    );

    await fireEvent.click(getByTestId("wikilink-choose-btn"));

    // Path unchanged, no set_ call.
    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "/Users/me/people",
      ),
    );
    expect(setCalls).toBe(0);
  });

  it("clears the path when Clear is clicked", async () => {
    let stored: string | null = "/Users/me/people";
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => stored,
      set_wikilink_source_folder: (args) => {
        stored = (args.path as string | null) ?? null;
        return null;
      },
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "/Users/me/people",
      ),
    );

    await fireEvent.click(getByTestId("wikilink-clear-btn"));

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "Not set",
      ),
    );
    expect(invokeFn).toHaveBeenCalledWith("set_wikilink_source_folder", {
      path: null,
    });
  });

  it("invokes reveal_wikilink_source_folder when Reveal in Finder is clicked", async () => {
    let revealCalls = 0;
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => "/Users/me/people",
      reveal_wikilink_source_folder: () => {
        revealCalls += 1;
        return null;
      },
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "/Users/me/people",
      ),
    );

    await fireEvent.click(getByTestId("wikilink-reveal-btn"));
    await waitFor(() => expect(revealCalls).toBe(1));
  });

  it("does not render the Reveal button when the folder is unset", async () => {
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => null,
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "Not set",
      ),
    );
    expect(() => getByTestId("wikilink-reveal-btn")).toThrow();
  });

  it("surfaces a validation error from the Rust setter", async () => {
    let stored: string | null = null;
    const invokeFn = makeInvoke({
      get_wikilink_source_folder: () => stored,
      pick_wikilink_source_folder: () => "/garbage",
      set_wikilink_source_folder: () => {
        throw "folder does not exist";
      },
    });

    const { getByTestId } = render(WikilinkFolderSection, {
      props: { invokeFn },
    });

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
        "Not set",
      ),
    );

    await fireEvent.click(getByTestId("wikilink-choose-btn"));

    await waitFor(() =>
      expect(getByTestId("wikilink-folder-error").textContent).toContain(
        "folder does not exist",
      ),
    );
    // Path remained unset.
    expect(getByTestId("wikilink-folder-path").textContent?.trim()).toBe(
      "Not set",
    );
  });
});

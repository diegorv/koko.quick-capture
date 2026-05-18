import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import type { UnlistenFn } from "@tauri-apps/api/event";
import MainNav from "./MainNav.svelte";

function makeInvoke(handlers: Record<string, () => unknown>) {
  return vi.fn(async (cmd: string) => {
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unmocked invoke: ${cmd}`);
    return handler();
  });
}

function noopListen(): (event: string, handler: () => void) => Promise<UnlistenFn> {
  return vi.fn(async () => () => {});
}

describe("MainNav", () => {
  it("highlights the active tab", () => {
    const { getByTestId } = render(MainNav, {
      props: {
        active: "archive",
        invokeFn: makeInvoke({ inbox_count: () => 0 }),
        listenFn: noopListen(),
      },
    });
    expect(getByTestId("nav-archive").getAttribute("aria-current")).toBe("page");
    expect(getByTestId("nav-inbox").getAttribute("aria-current")).toBeNull();
  });

  it("calls onNavigate on tab click", async () => {
    const onNavigate = vi.fn();
    const { getByTestId } = render(MainNav, {
      props: {
        active: "inbox",
        onNavigate,
        invokeFn: makeInvoke({ inbox_count: () => 0 }),
        listenFn: noopListen(),
      },
    });
    await fireEvent.click(getByTestId("nav-archive"));
    expect(onNavigate).toHaveBeenCalledWith("archive");
  });

  it("does not navigate when clicking the already-active tab", async () => {
    const onNavigate = vi.fn();
    const { getByTestId } = render(MainNav, {
      props: {
        active: "inbox",
        onNavigate,
        invokeFn: makeInvoke({ inbox_count: () => 0 }),
        listenFn: noopListen(),
      },
    });
    await fireEvent.click(getByTestId("nav-inbox"));
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("renders the inbox-count badge when count > 0", async () => {
    const { findByTestId } = render(MainNav, {
      props: {
        active: "inbox",
        invokeFn: makeInvoke({ inbox_count: () => 12 }),
        listenFn: noopListen(),
      },
    });
    const badge = await findByTestId("inbox-badge");
    expect(badge.textContent).toBe("12");
  });

  it("omits the badge when count is 0", async () => {
    const invokeFn = makeInvoke({ inbox_count: () => 0 });
    const { queryByTestId } = render(MainNav, {
      props: { active: "inbox", invokeFn, listenFn: noopListen() },
    });
    await waitFor(() => expect(invokeFn).toHaveBeenCalled());
    expect(queryByTestId("inbox-badge")).toBeNull();
  });

  it("refreshes the badge on captures:changed", async () => {
    let n = 1;
    const invokeFn = makeInvoke({ inbox_count: () => n });
    let fire: (() => void) | null = null;
    const listenFn = vi.fn(async (_e: string, h: () => void) => {
      fire = h;
      return () => {};
    });
    const { findByTestId } = render(MainNav, {
      props: { active: "inbox", invokeFn, listenFn },
    });

    let badge = await findByTestId("inbox-badge");
    expect(badge.textContent).toBe("1");
    await waitFor(() => expect(fire).not.toBeNull());

    n = 5;
    fire!();
    await waitFor(() => {
      badge = (badge.ownerDocument!.querySelector(
        '[data-testid="inbox-badge"]',
      ) as HTMLElement)!;
      expect(badge.textContent).toBe("5");
    });
  });

  it("Cmd+2 navigates to archive when inbox is active", async () => {
    const onNavigate = vi.fn();
    render(MainNav, {
      props: {
        active: "inbox",
        onNavigate,
        invokeFn: makeInvoke({ inbox_count: () => 0 }),
        listenFn: noopListen(),
      },
    });

    const event = new KeyboardEvent("keydown", { key: "2", metaKey: true, cancelable: true });
    window.dispatchEvent(event);

    expect(onNavigate).toHaveBeenCalledWith("archive");
  });
});

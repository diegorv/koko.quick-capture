import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import InboxList from "./InboxList.svelte";
import type { Capture } from "$lib/captures/types";

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

function link(id: string, url: string): Capture {
  return {
    id,
    kind: "Link",
    created_at: new Date().toISOString(),
    payload: { url, raw_text: url, title: null },
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

function file(id: string, original_name: string): Capture {
  return {
    id,
    kind: "File",
    created_at: new Date().toISOString(),
    payload: {
      source_path: `/tmp/${original_name}`,
      mime: "application/pdf",
      original_name,
    },
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

describe("InboxList", () => {
  it("renders one row per capture with the right payload preview", () => {
    const captures: Capture[] = [
      note("01H000000000000000000000A1", "first thought"),
      link("01H000000000000000000000B2", "https://example.com/page"),
      file("01H000000000000000000000C3", "notes.pdf"),
    ];
    const { getAllByRole, getByText } = render(InboxList, {
      props: {
        captures,
        selectedId: null,
        onSelect: vi.fn(),
        onStarToggle: vi.fn(),
        onDelete: vi.fn(),
      },
    });

    const rows = getAllByRole("option");
    expect(rows.length).toBe(3);
    expect(getByText("first thought")).toBeTruthy();
    expect(getByText("https://example.com/page")).toBeTruthy();
    expect(getByText("notes.pdf")).toBeTruthy();
  });

  it("clicking a row calls onSelect with that row's id", async () => {
    const onSelect = vi.fn();
    const captures = [
      note("01H000000000000000000000A1", "first"),
      note("01H000000000000000000000B2", "second"),
    ];
    const { getAllByRole } = render(InboxList, {
      props: {
        captures,
        selectedId: null,
        onSelect,
        onStarToggle: vi.fn(),
        onDelete: vi.fn(),
      },
    });

    const rows = getAllByRole("option");
    await fireEvent.click(rows[1]);
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith("01H000000000000000000000B2");
  });

  it("clicking the star icon calls onStarToggle with (id, !starred)", async () => {
    const onStarToggle = vi.fn();
    const captures = [note("01H000000000000000000000A1", "first")];
    const { getByLabelText } = render(InboxList, {
      props: {
        captures,
        selectedId: null,
        onSelect: vi.fn(),
        onStarToggle,
        onDelete: vi.fn(),
      },
    });

    const starBtn = getByLabelText("Star capture");
    await fireEvent.click(starBtn);
    expect(onStarToggle).toHaveBeenCalledTimes(1);
    expect(onStarToggle).toHaveBeenCalledWith(
      "01H000000000000000000000A1",
      true,
    );
  });

  it("clicking the delete icon calls onDelete with the id", async () => {
    const onDelete = vi.fn();
    const captures = [note("01H000000000000000000000A1", "first")];
    const { getByLabelText } = render(InboxList, {
      props: {
        captures,
        selectedId: null,
        onSelect: vi.fn(),
        onStarToggle: vi.fn(),
        onDelete,
      },
    });

    const deleteBtn = getByLabelText("Delete capture");
    await fireEvent.click(deleteBtn);
    expect(onDelete).toHaveBeenCalledTimes(1);
    expect(onDelete).toHaveBeenCalledWith("01H000000000000000000000A1");
  });

  it("clicking star does not also call onSelect (no row-click bubble)", async () => {
    const onSelect = vi.fn();
    const onStarToggle = vi.fn();
    const captures = [note("01H000000000000000000000A1", "first")];
    const { getByLabelText } = render(InboxList, {
      props: {
        captures,
        selectedId: null,
        onSelect,
        onStarToggle,
        onDelete: vi.fn(),
      },
    });

    await fireEvent.click(getByLabelText("Star capture"));
    expect(onStarToggle).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();
  });

  describe("keyboard navigation", () => {
    const ids = [
      "01H000000000000000000000A1",
      "01H000000000000000000000B2",
      "01H000000000000000000000C3",
    ];

    function twoNotes(): Capture[] {
      return [note(ids[0], "first"), note(ids[1], "second")];
    }

    it("ArrowDown moves selection from row 0 to row 1", async () => {
      const onSelect = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[0],
          onSelect,
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
        },
      });

      const listbox = getByRole("listbox");
      await fireEvent.keyDown(listbox, { key: "ArrowDown" });
      expect(onSelect).toHaveBeenCalledTimes(1);
      expect(onSelect).toHaveBeenCalledWith(ids[1]);
    });

    it("ArrowDown clamps at the last row", async () => {
      const onSelect = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[1],
          onSelect,
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "ArrowDown" });
      expect(onSelect).toHaveBeenCalledTimes(1);
      expect(onSelect).toHaveBeenCalledWith(ids[1]);
    });

    it("ArrowUp clamps at the first row", async () => {
      const onSelect = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[0],
          onSelect,
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "ArrowUp" });
      expect(onSelect).toHaveBeenCalledTimes(1);
      expect(onSelect).toHaveBeenCalledWith(ids[0]);
    });

    it("Enter on selection calls onOpen with the selected Capture", async () => {
      const onOpen = vi.fn();
      const captures = twoNotes();
      const { getByRole } = render(InboxList, {
        props: {
          captures,
          selectedId: ids[1],
          onSelect: vi.fn(),
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
          onOpen,
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "Enter" });
      expect(onOpen).toHaveBeenCalledTimes(1);
      expect(onOpen).toHaveBeenCalledWith(captures[1]);
    });

    it("S toggles star on the selected row", async () => {
      const onStarToggle = vi.fn();
      const captures = twoNotes();
      const { getByRole } = render(InboxList, {
        props: {
          captures,
          selectedId: ids[0],
          onSelect: vi.fn(),
          onStarToggle,
          onDelete: vi.fn(),
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "s" });
      expect(onStarToggle).toHaveBeenCalledTimes(1);
      expect(onStarToggle).toHaveBeenCalledWith(ids[0], true);
    });

    it("Cmd+Backspace calls onDelete on the selected row", async () => {
      const onDelete = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[1],
          onSelect: vi.fn(),
          onStarToggle: vi.fn(),
          onDelete,
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), {
        key: "Backspace",
        metaKey: true,
      });
      expect(onDelete).toHaveBeenCalledTimes(1);
      expect(onDelete).toHaveBeenCalledWith(ids[1]);
    });

    it("Escape calls onClose", async () => {
      const onClose = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[0],
          onSelect: vi.fn(),
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
          onClose,
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "Escape" });
      expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("Cmd+W calls onClose", async () => {
      const onClose = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[0],
          onSelect: vi.fn(),
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
          onClose,
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), {
        key: "w",
        metaKey: true,
      });
      expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("bare R triggers onRoute with the selected id (ADR-0010)", async () => {
      const onRoute = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[1],
          onSelect: vi.fn(),
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
          onRoute,
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "R" });
      expect(onRoute).toHaveBeenCalledTimes(1);
      expect(onRoute).toHaveBeenCalledWith(ids[1]);
    });

    it("Cmd+R does NOT trigger onRoute (modifier filters it out)", async () => {
      const onRoute = vi.fn();
      const { getByRole } = render(InboxList, {
        props: {
          captures: twoNotes(),
          selectedId: ids[0],
          onSelect: vi.fn(),
          onStarToggle: vi.fn(),
          onDelete: vi.fn(),
          onRoute,
        },
      });

      await fireEvent.keyDown(getByRole("listbox"), { key: "r", metaKey: true });
      expect(onRoute).not.toHaveBeenCalled();
    });
  });
});

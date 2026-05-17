import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import InboxDetail from "./InboxDetail.svelte";
import type { Capture } from "$lib/captures/types";

// `convertFileSrc` is a thin path -> webview URL helper provided by
// Tauri's runtime; in jsdom we just need it to return something so the
// `<img src>` lands somewhere we can assert against. Vitest hoists
// `vi.mock` calls to the top of the file, so this mock is in place
// before InboxDetail.svelte's import resolves.
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `tauri-fake://${p}`,
}));

function link(id: string, url: string): Capture {
  return {
    id,
    kind: "Link",
    created_at: new Date().toISOString(),
    payload: { url, raw_text: url, title: null },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

function clip(id: string, text: string): Capture {
  return {
    id,
    kind: "Clip",
    created_at: new Date().toISOString(),
    payload: { text },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

function note(id: string, text: string): Capture {
  return {
    id,
    kind: "Note",
    created_at: new Date().toISOString(),
    payload: { text },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

function pathShot(id: string, sourcePath: string): Capture {
  return {
    id,
    kind: "Shot",
    created_at: new Date().toISOString(),
    payload: {
      source_path: sourcePath,
      mime: "image/png",
      width: null,
      height: null,
    },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

function bytesShot(id: string, blobPath: string): Capture {
  return {
    id,
    kind: "Shot",
    created_at: new Date().toISOString(),
    payload: {
      blob_path: blobPath,
      mime: "image/png",
      width: null,
      height: null,
    },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

function file(id: string, name: string, sourcePath: string): Capture {
  return {
    id,
    kind: "File",
    created_at: new Date().toISOString(),
    payload: {
      source_path: sourcePath,
      mime: "application/pdf",
      original_name: name,
    },
    source_app: null,
    starred: false,
    deleted_at: null,
  };
}

describe("InboxDetail", () => {
  it("renders the placeholder when capture is null", () => {
    const { getByText } = render(InboxDetail, {
      props: { capture: null, onOpenLink: vi.fn(), onReveal: vi.fn() },
    });
    expect(getByText("Select a Capture")).toBeTruthy();
  });

  describe("Link", () => {
    it("shows the URL and an Open in Browser button", () => {
      const cap = link("01H000000000000000000000A1", "https://example.com/page");
      const { getByText, getByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("https://example.com/page")).toBeTruthy();
      expect(getByRole("button", { name: "Open in Browser" })).toBeTruthy();
    });

    it("clicking the button calls onOpenLink with the URL", async () => {
      const cap = link("01H000000000000000000000A1", "https://example.com/page");
      const onOpenLink = vi.fn();
      const { getByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink, onReveal: vi.fn() },
      });
      await fireEvent.click(getByRole("button", { name: "Open in Browser" }));
      expect(onOpenLink).toHaveBeenCalledTimes(1);
      expect(onOpenLink).toHaveBeenCalledWith("https://example.com/page");
    });
  });

  describe("Clip", () => {
    it("shows the full text and no action button", () => {
      const cap = clip("01H000000000000000000000B2", "some clipped text");
      const { getByText, queryByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("some clipped text")).toBeTruthy();
      expect(queryByRole("button")).toBeNull();
    });
  });

  describe("Note", () => {
    it("shows the full text and no action button", () => {
      const cap = note("01H000000000000000000000C3", "a free-text note");
      const { getByText, queryByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("a free-text note")).toBeTruthy();
      expect(queryByRole("button")).toBeNull();
    });
  });

  describe("Shot (path)", () => {
    it("shows the source path, an image preview, and a Reveal in Finder button", () => {
      const cap = pathShot(
        "01H000000000000000000000D4",
        "/tmp/screenshot.png",
      );
      const { getByText, getByRole, getByAltText } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("/tmp/screenshot.png")).toBeTruthy();
      const img = getByAltText("Shot preview") as HTMLImageElement;
      expect(img.src).toContain("/tmp/screenshot.png");
      expect(getByRole("button", { name: "Reveal in Finder" })).toBeTruthy();
    });

    it("clicking the button calls onReveal with the id", async () => {
      const cap = pathShot(
        "01H000000000000000000000D4",
        "/tmp/screenshot.png",
      );
      const onReveal = vi.fn();
      const { getByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal },
      });
      await fireEvent.click(getByRole("button", { name: "Reveal in Finder" }));
      expect(onReveal).toHaveBeenCalledTimes(1);
      expect(onReveal).toHaveBeenCalledWith("01H000000000000000000000D4");
    });
  });

  describe("Shot (bytes)", () => {
    it("shows the blob path, an image preview, and an Open Image button", () => {
      const cap = bytesShot(
        "01H000000000000000000000E5",
        "/var/blobs/01H000000000000000000000E5.png",
      );
      const { getByText, getByRole, getByAltText } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("/var/blobs/01H000000000000000000000E5.png")).toBeTruthy();
      const img = getByAltText("Shot preview") as HTMLImageElement;
      expect(img.src).toContain("/var/blobs/01H000000000000000000000E5.png");
      expect(getByRole("button", { name: "Open Image" })).toBeTruthy();
    });

    it("clicking the button calls onReveal with the id", async () => {
      const cap = bytesShot(
        "01H000000000000000000000E5",
        "/var/blobs/01H000000000000000000000E5.png",
      );
      const onReveal = vi.fn();
      const { getByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal },
      });
      await fireEvent.click(getByRole("button", { name: "Open Image" }));
      expect(onReveal).toHaveBeenCalledTimes(1);
      expect(onReveal).toHaveBeenCalledWith("01H000000000000000000000E5");
    });
  });

  describe("File", () => {
    it("shows the original name, mime, source path, and a Reveal in Finder button", () => {
      const cap = file(
        "01H000000000000000000000F6",
        "notes.pdf",
        "/tmp/notes.pdf",
      );
      const { getByText, getByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("notes.pdf")).toBeTruthy();
      expect(getByText("application/pdf")).toBeTruthy();
      expect(getByText("/tmp/notes.pdf")).toBeTruthy();
      expect(getByRole("button", { name: "Reveal in Finder" })).toBeTruthy();
    });

    it("clicking the button calls onReveal with the id", async () => {
      const cap = file(
        "01H000000000000000000000F6",
        "notes.pdf",
        "/tmp/notes.pdf",
      );
      const onReveal = vi.fn();
      const { getByRole } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal },
      });
      await fireEvent.click(getByRole("button", { name: "Reveal in Finder" }));
      expect(onReveal).toHaveBeenCalledTimes(1);
      expect(onReveal).toHaveBeenCalledWith("01H000000000000000000000F6");
    });
  });
});

import { render, fireEvent } from "@testing-library/svelte";
import { describe, it, expect, vi } from "vitest";
import InboxDetail from "./InboxDetail.svelte";
import type { Capture, Destination } from "$lib/captures/types";

function destination(
  id: string,
  name: string,
  overrides: Partial<Destination> = {},
): Destination {
  return {
    id,
    name,
    color: null,
    created_at: new Date().toISOString(),
    deleted_at: null,
    kind: "label",
    config: null,
    ...overrides,
  };
}

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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
  };
}

function transcription(id: string, text: string, audioPath: string): Capture {
  return {
    id,
    kind: "Transcription",
    created_at: new Date().toISOString(),
    payload: {
      text,
      audio_path: audioPath,
      duration_secs: 42.5,
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
    read_at: null,
    source_title: null,
    source_url: null,
    destination_id: null,
    routed_at: null,
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

  describe("Route action (ADR-0010)", () => {
    it("renders a Route button when onRoute is provided", () => {
      const cap = note("01H000000000000000000000R1", "to route");
      const { getByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onRoute: vi.fn(),
        },
      });
      expect(getByTestId("detail-route-btn")).toBeTruthy();
    });

    it("omits the Route button when onRoute is not provided", () => {
      const cap = note("01H000000000000000000000R2", "no route");
      const { queryByTestId } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(queryByTestId("detail-route-btn")).toBeNull();
    });

    it("clicking Route calls onRoute with the capture id", async () => {
      const cap = note("01H000000000000000000000R3", "click me");
      const onRoute = vi.fn();
      const { getByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onRoute,
        },
      });
      await fireEvent.click(getByTestId("detail-route-btn"));
      expect(onRoute).toHaveBeenCalledTimes(1);
      expect(onRoute).toHaveBeenCalledWith("01H000000000000000000000R3");
    });

    it("renders Move-to-Inbox button when onUnroute is provided (Archive)", () => {
      const cap = note("01H000000000000000000000U1", "routed");
      const { getByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onRoute: vi.fn(),
          onUnroute: vi.fn(),
        },
      });
      expect(getByTestId("detail-unroute-btn")).toBeTruthy();
    });

    it("Route button reads 'Re-route' when onUnroute is also provided", () => {
      const cap = note("01H000000000000000000000U2", "routed");
      const { getByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onRoute: vi.fn(),
          onUnroute: vi.fn(),
        },
      });
      expect(getByTestId("detail-route-btn").textContent?.trim()).toBe("Re-route");
    });

    it("clicking Move-to-Inbox calls onUnroute with the id", async () => {
      const cap = note("01H000000000000000000000U3", "routed");
      const onUnroute = vi.fn();
      const { getByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onRoute: vi.fn(),
          onUnroute,
        },
      });
      await fireEvent.click(getByTestId("detail-unroute-btn"));
      expect(onUnroute).toHaveBeenCalledWith("01H000000000000000000000U3");
    });
  });

  describe("mentions inside Note / Clip payloads", () => {
    it("renders [[Name]] tokens as clickable chips when onMentionClick is provided", async () => {
      const cap = note(
        "01H000000000000000000000M1",
        "ping [[Diego]] and [[Ana]] later",
      );
      const onMentionClick = vi.fn();
      const { getAllByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onMentionClick,
        },
      });
      const chips = getAllByTestId("mention-chip");
      expect(chips.map((c) => c.getAttribute("data-mention"))).toEqual([
        "Diego",
        "Ana",
      ]);

      await fireEvent.click(chips[0]);
      expect(onMentionClick).toHaveBeenCalledTimes(1);
      expect(onMentionClick).toHaveBeenCalledWith("Diego");
    });

    it("renders mentions as plain text when onMentionClick is omitted", () => {
      const cap = note("01H000000000000000000000M2", "see [[Diego]]");
      const { getByTestId, queryAllByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
        },
      });
      expect(queryAllByTestId("mention-chip")).toHaveLength(0);
      // The literal `[[Diego]]` still appears in the pre.
      expect(getByTestId("payload-text").textContent).toContain("[[Diego]]");
    });

    it("does not render mention chips for Link kind", () => {
      const cap = link("01H000000000000000000000M3", "https://example.com/[[Diego]]");
      const { queryAllByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
          onMentionClick: vi.fn(),
        },
      });
      expect(queryAllByTestId("mention-chip")).toHaveLength(0);
    });
  });

  describe("destination chip", () => {
    function routedClip(destId: string | null): Capture {
      const cap = clip("01H000000000000000000000D1", "routed text");
      cap.destination_id = destId;
      cap.routed_at = "2026-05-18T12:00:00Z";
      return cap;
    }

    it("renders a To row with the destination name when supplied", () => {
      const cap = routedClip("01H000000000000000000000DEST");
      const dest = destination("01H000000000000000000000DEST", "Personal Brain");
      const { getByTestId, getByText } = render(InboxDetail, {
        props: {
          capture: cap,
          destination: dest,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
        },
      });
      // Label "To" lives in the <dt>; the value sits in the
      // <dd data-testid="detail-destination"> sibling.
      expect(getByText("To")).toBeTruthy();
      expect(getByTestId("detail-destination").textContent).toContain(
        "Personal Brain",
      );
    });

    it("hides the chip when destination is null even if routed_at is set", () => {
      // Covers the soft-deleted-destination case: the Archive page
      // passes `null` because the dest dropped out of the live map.
      const cap = routedClip("01H000000000000000000000DEST");
      const { queryByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          destination: null,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
        },
      });
      expect(queryByTestId("detail-destination")).toBeNull();
    });

    it("hides the chip on un-routed captures", () => {
      // Inbox path: capture has no destination_id; the prop default
      // (`null`) suppresses the chip.
      const cap = clip("01H000000000000000000000D2", "unrouted text");
      const { queryByTestId } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(queryByTestId("detail-destination")).toBeNull();
    });

    it("annotates soft-deleted destinations", () => {
      const cap = routedClip("01H000000000000000000000DEST");
      const dest = destination("01H000000000000000000000DEST", "Old Brain", {
        deleted_at: "2026-05-18T12:00:00Z",
      });
      const { getByTestId } = render(InboxDetail, {
        props: {
          capture: cap,
          destination: dest,
          onOpenLink: vi.fn(),
          onReveal: vi.fn(),
        },
      });
      expect(getByTestId("detail-destination").textContent).toContain("(deleted)");
    });
  });

  describe("Transcription", () => {
    it("shows the transcript text", () => {
      const cap = transcription(
        "01H000000000000000000000T1",
        "Hello world from voice",
        "/tmp/test.wav",
      );
      const { getByTestId } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByTestId("payload-text").textContent).toContain(
        "Hello world from voice",
      );
    });

    it("renders an audio element for playback", () => {
      const cap = transcription(
        "01H000000000000000000000T2",
        "Some transcript",
        "/path/to/audio.wav",
      );
      const { container } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      const audio = container.querySelector("audio");
      expect(audio).toBeTruthy();
    });

    it("shows duration metadata", () => {
      const cap = transcription(
        "01H000000000000000000000T3",
        "Short note",
        "/tmp/audio.wav",
      );
      const { getByText } = render(InboxDetail, {
        props: { capture: cap, onOpenLink: vi.fn(), onReveal: vi.fn() },
      });
      expect(getByText("0:42")).toBeTruthy();
    });
  });
});

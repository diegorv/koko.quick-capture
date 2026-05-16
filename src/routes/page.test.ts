import { render, screen } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import Page from "./+page.svelte";

describe("main page", () => {
  it("renders the app title", () => {
    render(Page);
    expect(screen.getByText("quick-capture")).toBeTruthy();
  });
});

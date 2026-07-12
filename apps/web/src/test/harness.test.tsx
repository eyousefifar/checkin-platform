import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { whepUrl } from "@/lib/whep";

describe("test harness", () => {
  it("resolves @/ alias, renders TSX in jsdom, and exposes accessible link href", () => {
    const href = whepUrl("http://localhost:8889/", "cam_in");
    render(
      <a href={href} aria-label="WHEP stream">
        stream
      </a>,
    );
    const link = screen.getByRole("link", { name: "WHEP stream" });
    expect(link.getAttribute("href")).toBe("http://localhost:8889/cam_in/whep");
  });
});

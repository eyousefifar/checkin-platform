import { describe, expect, it } from "vitest";
import { whepUrl } from "./whep";

describe("whepUrl", () => {
  it("trims one trailing base slash, one leading path slash, and builds /<path>/whep", () => {
    expect(whepUrl("http://localhost:8889/", "/demo")).toBe(
      "http://localhost:8889/demo/whep",
    );
    expect(whepUrl("http://localhost:8889", "demo")).toBe(
      "http://localhost:8889/demo/whep",
    );
  });
});

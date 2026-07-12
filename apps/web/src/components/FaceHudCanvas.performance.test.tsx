import { render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FaceDet } from "@/lib/types";
import { FaceHudCanvas } from "./FaceHudCanvas";

function face(label: string): FaceDet {
  return {
    track_id: 1,
    bbox: [0.1, 0.1, 0.3, 0.4],
    label,
    score: 0.9,
    quality_ok: true,
    state: "tracked",
    employee_id: 1,
  };
}

describe("FaceHudCanvas performance", () => {
  let widthWrites = 0;
  let heightWrites = 0;
  let strokeRectCalls = 0;
  let clearRectCalls = 0;

  beforeEach(() => {
    widthWrites = 0;
    heightWrites = 0;
    strokeRectCalls = 0;
    clearRectCalls = 0;

    const ctx = {
      setTransform: vi.fn(),
      clearRect: vi.fn(() => {
        clearRectCalls += 1;
      }),
      strokeRect: vi.fn(() => {
        strokeRectCalls += 1;
      }),
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      measureText: vi.fn(() => ({ width: 40 })),
      fillRect: vi.fn(),
      fillText: vi.fn(),
      strokeStyle: "",
      fillStyle: "",
      lineWidth: 1,
      font: "",
    };

    // jsdom canvas is incomplete; stub only the 2d path used by the HUD.
    (HTMLCanvasElement.prototype as unknown as { getContext: unknown }).getContext =
      vi.fn(() => ctx);

    Object.defineProperty(HTMLCanvasElement.prototype, "width", {
      configurable: true,
      get() {
        return (this as unknown as { __w?: number }).__w ?? 0;
      },
      set(v: number) {
        widthWrites += 1;
        (this as unknown as { __w?: number }).__w = v;
      },
    });
    Object.defineProperty(HTMLCanvasElement.prototype, "height", {
      configurable: true,
      get() {
        return (this as unknown as { __h?: number }).__h ?? 0;
      },
      set(v: number) {
        heightWrites += 1;
        (this as unknown as { __h?: number }).__h = v;
      },
    });

    vi.stubGlobal("devicePixelRatio", 1);
    Object.defineProperty(window, "devicePixelRatio", {
      configurable: true,
      get: () => 1,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("does not reallocate backing store when faces change at fixed size", () => {
    const { rerender } = render(
      <FaceHudCanvas faces={[face("A")]} width={640} height={360} />,
    );
    expect(widthWrites).toBe(1);
    expect(heightWrites).toBe(1);
    expect(strokeRectCalls).toBe(1);
    expect(clearRectCalls).toBe(1);

    const wAfterFirst = widthWrites;
    const hAfterFirst = heightWrites;

    rerender(<FaceHudCanvas faces={[face("B")]} width={640} height={360} />);
    expect(widthWrites).toBe(wAfterFirst);
    expect(heightWrites).toBe(hAfterFirst);
    // Still clears and draws on every faces update.
    expect(clearRectCalls).toBe(2);
    expect(strokeRectCalls).toBe(2);
  });

  it("writes backing size once when CSS size or DPR changes and still draws", () => {
    const { rerender } = render(
      <FaceHudCanvas faces={[face("A")]} width={320} height={180} />,
    );
    expect(widthWrites).toBe(1);
    expect(heightWrites).toBe(1);

    rerender(<FaceHudCanvas faces={[face("A")]} width={640} height={360} />);
    expect(widthWrites).toBe(2);
    expect(heightWrites).toBe(2);
    expect(clearRectCalls).toBe(2);
    expect(strokeRectCalls).toBe(2);

    Object.defineProperty(window, "devicePixelRatio", {
      configurable: true,
      get: () => 2,
    });
    // Same CSS size, higher DPR → physical size changes → one write.
    rerender(<FaceHudCanvas faces={[face("A")]} width={640} height={360} />);
    expect(widthWrites).toBe(3);
    expect(heightWrites).toBe(3);
    expect(strokeRectCalls).toBe(3);
  });
});

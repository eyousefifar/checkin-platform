import { describe, expect, it } from "vitest";
import {
  advanceStability,
  faceGuideScore,
  idleStability,
  shouldUpdateStatus,
  smoothBbox,
  smoothYaw,
  STABLE_HOLD_MS,
  yawMatchesSlot,
} from "./guidedCaptureMath";

describe("smoothBbox", () => {
  it("seeds from next when prev is null", () => {
    expect(smoothBbox(null, [0.1, 0.2, 0.5, 0.6], 0.35)).toEqual([
      0.1, 0.2, 0.5, 0.6,
    ]);
  });

  it("returns null when face is lost", () => {
    expect(smoothBbox([0.1, 0.2, 0.5, 0.6], null)).toBeNull();
  });

  it("moves toward next without jumping fully", () => {
    const prev: [number, number, number, number] = [0, 0, 0.4, 0.4];
    const next: [number, number, number, number] = [0.2, 0.2, 0.6, 0.6];
    const out = smoothBbox(prev, next, 0.5)!;
    expect(out[0]).toBeCloseTo(0.1);
    expect(out[1]).toBeCloseTo(0.1);
    expect(out[2]).toBeCloseTo(0.5);
    expect(out[3]).toBeCloseTo(0.5);
  });
});

describe("smoothYaw", () => {
  it("resets when next is null", () => {
    expect(smoothYaw(5, null)).toBeNull();
  });

  it("lerps toward next", () => {
    expect(smoothYaw(0, 10, 0.4)).toBeCloseTo(4);
  });
});

describe("yawMatchesSlot hysteresis", () => {
  const center = { id: "center", yawMin: -8, yawMax: 8 };
  const slightLeft = { id: "slight_left", yawMin: 8, yawMax: 18 };

  it("accepts center without yaw (landmarks missing)", () => {
    expect(yawMatchesSlot(null, center)).toBe(true);
    expect(yawMatchesSlot(null, slightLeft)).toBe(false);
  });

  it("uses hard edges for entry", () => {
    expect(yawMatchesSlot(7.5, center, { sticky: false })).toBe(true);
    expect(yawMatchesSlot(8.5, center, { sticky: false })).toBe(false);
    expect(yawMatchesSlot(8.5, slightLeft, { sticky: false })).toBe(true);
  });

  it("expands range when sticky so noise does not drop lock", () => {
    // Just outside hard edge but inside hold pad
    expect(yawMatchesSlot(9.5, center, { sticky: true, holdPad: 2.5 })).toBe(
      true,
    );
    expect(yawMatchesSlot(9.5, center, { sticky: false })).toBe(false);
  });
});

describe("faceGuideScore", () => {
  it("accepts a well-framed centered face", () => {
    // Face roughly matching default 42%×55% guide
    const score = faceGuideScore([0.25, 0.2, 0.75, 0.8]);
    expect(score.ok).toBe(true);
    expect(score.hint).toBeNull();
  });

  it("hints when face is too small", () => {
    const score = faceGuideScore([0.4, 0.4, 0.55, 0.55]);
    expect(score.ok).toBe(false);
    expect(score.hint).toMatch(/closer/i);
  });

  it("hints direction when off-center", () => {
    // Large enough face shifted left
    const score = faceGuideScore([0.0, 0.2, 0.45, 0.85]);
    expect(score.ok).toBe(false);
    expect(score.hint).toMatch(/right|left|up|down/i);
  });
});

describe("advanceStability", () => {
  it("resets on bad frames", () => {
    const started = advanceStability(idleStability(), true, 1000);
    const mid = advanceStability(started, true, 1400);
    expect(mid.progress).toBeGreaterThan(0);
    const bad = advanceStability(mid, false, 1500);
    expect(bad).toEqual(idleStability());
  });

  it("locks after continuous hold window", () => {
    const t0 = 10_000;
    let s = advanceStability(idleStability(), true, t0);
    expect(s.locked).toBe(false);
    s = advanceStability(s, true, t0 + STABLE_HOLD_MS / 2);
    expect(s.progress).toBeCloseTo(0.5);
    expect(s.locked).toBe(false);
    s = advanceStability(s, true, t0 + STABLE_HOLD_MS);
    expect(s.locked).toBe(true);
    expect(s.progress).toBe(1);
  });
});

describe("shouldUpdateStatus", () => {
  it("suppresses same-category thrash within sticky window", () => {
    expect(
      shouldUpdateStatus("Move left a little", "Move right a little", 1000, 1100, 300),
    ).toBe(false);
  });

  it("allows category change immediately", () => {
    expect(
      shouldUpdateStatus("Move left a little", "Hold steady · locking", 1000, 1050, 300),
    ).toBe(true);
  });
});

import { test } from "node:test";
import assert from "node:assert/strict";

import {
    computeSizeContract,
    MIN_RENDER_SCALE,
} from "./sizeContract.ts";

const CAP = 8192;

test("DPR 1 at 100% zoom: physical equals CSS, scale stays 1", () => {
    const c = computeSizeContract(1280, 800, 1, CAP);
    assert.equal(c.cssWidth, 1280);
    assert.equal(c.cssHeight, 800);
    assert.equal(c.scale, 1);
    assert.equal(c.physicalWidth, 1280);
    assert.equal(c.physicalHeight, 800);
});

test("integer HiDPI: physical = CSS x DPR exactly", () => {
    const c = computeSizeContract(1280, 800, 2, CAP);
    assert.equal(c.scale, 2);
    assert.equal(c.physicalWidth, 2560);
    assert.equal(c.physicalHeight, 1600);
});

test("fractional DPR (125% browser zoom) passes through unfloored", () => {
    const c = computeSizeContract(1097, 743, 1.25, CAP);
    // Scale must NOT be floored/truncated — that mismatch is what made
    // the backing store disagree with the chrome layout.
    assert.equal(c.scale, 1.25);
    // Physical = floor(css * scale), matching the wasm-side
    // `(width_px as f32 * scale) as u32` truncation.
    assert.equal(c.physicalWidth, Math.floor(1097 * 1.25));
    assert.equal(c.physicalHeight, Math.floor(743 * 1.25));
});

test("fractional DPR 1.5 keeps exact scale", () => {
    const c = computeSizeContract(1366.6, 768.4, 1.5, CAP);
    // CSS dims floor to integers (style + chrome layout units)...
    assert.equal(c.cssWidth, 1366);
    assert.equal(c.cssHeight, 768);
    // ...while the scale stays exactly 1.5.
    assert.equal(c.scale, 1.5);
    assert.equal(c.physicalWidth, 2049);
    assert.equal(c.physicalHeight, 1152);
});

test("zoom-out DPR below 1 is honored (no forced upscale)", () => {
    const c = computeSizeContract(2000, 1000, 0.8, CAP);
    assert.equal(c.scale, 0.8);
    assert.equal(c.physicalWidth, 1600);
    assert.equal(c.physicalHeight, 800);
});

test("texture cap clamps the scale, never the layout", () => {
    // 1600x900 CSS at DPR 2 wants 3200x1800; a 2048 cap affords at most
    // 2048/1600 = 1.28.
    const c = computeSizeContract(1600, 900, 2, 2048);
    assert.equal(c.cssWidth, 1600, "layout dims must stay the CSS rect");
    assert.equal(c.cssHeight, 900);
    assert.ok(c.scale < 2, "scale must drop below raw DPR");
    assert.ok(c.physicalWidth <= 2048);
    assert.ok(c.physicalHeight <= 2048);
    // The defining invariant: the physical surface is css x scale, so
    // chrome (css layout x scale at draw time) exactly fills it.
    assert.equal(c.physicalWidth, Math.floor(c.cssWidth * c.scale));
    assert.equal(c.physicalHeight, Math.floor(c.cssHeight * c.scale));
});

test("viewport wider than the cap drops scale below 1 instead of cropping", () => {
    const c = computeSizeContract(4096, 1000, 1, 2048);
    assert.equal(c.scale, 0.5);
    assert.equal(c.physicalWidth, 2048);
    assert.equal(c.physicalHeight, 500);
});

test("physical never exceeds the cap across a sweep of sizes and DPRs", () => {
    for (const cap of [2048, 4096, 8192]) {
        for (const w of [320, 1097, 1920, 2560, 5120]) {
            for (const h of [240, 743, 1080, 1440, 2880]) {
                for (const dpr of [0.8, 1, 1.25, 1.5, 2, 3]) {
                    const c = computeSizeContract(w, h, dpr, cap);
                    assert.ok(
                        c.physicalWidth <= cap && c.physicalHeight <= cap,
                        `physical ${c.physicalWidth}x${c.physicalHeight} ` +
                        `exceeds cap ${cap} for ${w}x${h}@${dpr}`,
                    );
                    assert.ok(
                        c.scale > 0 && Number.isFinite(c.scale),
                        `bad scale ${c.scale} for ${w}x${h}@${dpr}`,
                    );
                }
            }
        }
    }
});

test("degenerate inputs collapse to a sane 1x1 @ 1 contract", () => {
    const zero = computeSizeContract(0, 0, 0, 8192);
    assert.equal(zero.cssWidth, 1);
    assert.equal(zero.cssHeight, 1);
    assert.equal(zero.scale, 1);
    assert.equal(zero.physicalWidth, 1);
    assert.equal(zero.physicalHeight, 1);

    const nan = computeSizeContract(Number.NaN, -5, Number.NaN, 8192);
    assert.equal(nan.cssWidth, 1);
    assert.equal(nan.cssHeight, 1);
    assert.equal(nan.scale, 1);

    // Pathological cap: the scale floor keeps the surface alive.
    const tiny = computeSizeContract(10_000, 10_000, 2, 64);
    assert.equal(tiny.scale, MIN_RENDER_SCALE);
    assert.ok(tiny.physicalWidth <= 64 && tiny.physicalHeight <= 64);
});

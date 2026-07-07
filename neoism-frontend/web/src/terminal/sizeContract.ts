/**
 * The one rendering size contract for the sugarloaf-backed web canvas.
 *
 * Every size source in the pipeline derives from this single pure
 * computation so they can never disagree:
 *
 *   - `canvas.style.{width,height}`   = `cssWidth` x `cssHeight`
 *     (the CSS-pixel rect the panel measured — layout truth).
 *   - chrome layout viewport (wasm `ChromeBridge::resize`)
 *                                     = `cssWidth` x `cssHeight`
 *     (chrome panel math runs in CSS pixels).
 *   - effective render scale (`Sugarloaf::rescale`, glyph raster
 *     density, chrome `UiEvent::Resize.scale`)
 *                                     = `scale`
 *     (devicePixelRatio clamped so the physical surface never exceeds
 *     the GPU texture cap on either axis — fractional DPRs like 1.25 /
 *     1.5 from browser zoom pass through untouched when they fit).
 *   - swapchain / canvas backing store (set by wgpu on
 *     `Surface::configure`)          = `physicalWidth` x `physicalHeight`
 *     = floor(css x scale), matching the wasm-side
 *     `(width_px as f32 * scale) as u32` truncation.
 *
 * When the viewport is so large that even scale=1 would exceed the
 * texture cap, the scale drops below 1 (down to `MIN_RENDER_SCALE`)
 * instead of letting the swapchain get silently clamped: a slightly
 * soft-but-complete frame beats chrome overflowing a cropped surface.
 */
export interface SizeContract {
    /** Integer CSS-pixel width — canvas.style.width and chrome layout. */
    cssWidth: number;
    /** Integer CSS-pixel height — canvas.style.height and chrome layout. */
    cssHeight: number;
    /** Effective device-pixel-ratio: raster density + chrome scale. */
    scale: number;
    /** Physical backing-store width: floor(cssWidth * scale). */
    physicalWidth: number;
    /** Physical backing-store height: floor(cssHeight * scale). */
    physicalHeight: number;
}

/**
 * Hard floor for the render scale. Below this the UI is unreadable
 * anyway; the floor keeps degenerate texture caps (or bogus zero/NaN
 * DPR inputs) from collapsing the swapchain to nothing.
 */
export const MIN_RENDER_SCALE = 0.25;

/**
 * Compute the size contract from raw measurements.
 *
 * @param rawCssWidth  measured CSS width (may be fractional / zero).
 * @param rawCssHeight measured CSS height (may be fractional / zero).
 * @param rawDpr       `window.devicePixelRatio` (fractional under
 *                     browser zoom: 1.25, 1.5, 0.8, ...).
 * @param textureCap   GPU max-texture-size for the swapchain.
 */
export function computeSizeContract(
    rawCssWidth: number,
    rawCssHeight: number,
    rawDpr: number,
    textureCap: number,
): SizeContract {
    const cssWidth = Number.isFinite(rawCssWidth)
        ? Math.max(1, Math.floor(rawCssWidth))
        : 1;
    const cssHeight = Number.isFinite(rawCssHeight)
        ? Math.max(1, Math.floor(rawCssHeight))
        : 1;
    const dpr = Number.isFinite(rawDpr) && rawDpr > 0 ? rawDpr : 1;
    const cap = Number.isFinite(textureCap)
        ? Math.max(1, Math.floor(textureCap))
        : 1;

    // Largest scale that keeps BOTH physical axes within the cap.
    const maxAffordable = Math.min(cap / cssWidth, cap / cssHeight);
    const scale = Math.max(MIN_RENDER_SCALE, Math.min(dpr, maxAffordable));

    const physicalWidth = Math.min(
        cap,
        Math.max(1, Math.floor(cssWidth * scale)),
    );
    const physicalHeight = Math.min(
        cap,
        Math.max(1, Math.floor(cssHeight * scale)),
    );
    return { cssWidth, cssHeight, scale, physicalWidth, physicalHeight };
}

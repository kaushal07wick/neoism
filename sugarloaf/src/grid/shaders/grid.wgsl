// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

// WGSL grid shader. Peer of `grid.metal`.
//
// Ported from `ghostty/src/renderer/shaders/shaders.metal`:
// - full_screen_vertex (line 191 in upstream)
// - cell_bg_fragment (line 451)
//
// Phase 1b scope: bg pass only. Same simplifications as the Metal
// port — no full Display P3 / linear-blending chain yet (the colors
// come in already sRGB-encoded from the CPU).
//
// Bindings:
// @group(0) @binding(0) Uniforms (256 bytes)
// @group(0) @binding(1) CellBg[] (cols * rows entries)
//
// Must match `WgpuGridRenderer`'s bind group layout in
// `sugarloaf/src/grid/webgpu.rs`.

struct Uniforms {
    projection:       mat4x4<f32>,   // offset   0
    grid_padding:     vec4<f32>,     //        64
    clip_rect:        vec4<f32>,     //        80
    cursor_color:     vec4<f32>,     //        96
    cursor_bg_color:  vec4<f32>,     //       112
    cell_size:        vec2<f32>,     //       128
    grid_size:        vec2<u32>,     //       136
    cursor_pos:       vec2<u32>,     //       144
    _pad_cursor:      vec2<u32>,     //       152
    min_contrast:     f32,           //       160
    flags:            u32,           //       164
    padding_extend:   u32,           //       168
    input_colorspace: u32,           //       172
    editor_pixel_offset_y: f32,      //       176 — uniform smooth-scroll offset
    // 3 separate u32s (vec3<u32> has 16-byte alignment in WGSL,
    // which would bloat the struct to 208 bytes vs the CPU's 192).
    _pad_editor_0:    u32,           //       180
    _pad_editor_1:    u32,           //       184
    _pad_editor_2:    u32,           //       188
};

// Color space / transfer curve helpers. Matrices match the Metal
// peer (`grid.metal`) and `sugarloaf/src/renderer/renderer.metal`,
// so grid + quad pipelines produce byte-identical framebuffer values.
// WGSL has no `select` with a `bool3` mask returning vec3, so we use
// the scalar `select` per-component via a helper.
fn grid_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(lo, hi, c > vec3<f32>(0.04045));
}

fn grid_linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(1.0 / 2.4)) * vec3<f32>(1.055) - vec3<f32>(0.055);
    return select(lo, hi, c > vec3<f32>(0.0031308));
}

fn grid_srgb_to_p3(linear_srgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(linear_srgb, vec3<f32>(0.82246197, 0.17753803, 0.0)),
        dot(linear_srgb, vec3<f32>(0.03319420, 0.96680580, 0.0)),
        dot(linear_srgb, vec3<f32>(0.01708263, 0.07239744, 0.91051993))
    );
}

fn grid_rec2020_to_p3(linear_r2020: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(linear_r2020, vec3<f32>( 1.34357825, -0.28217967, -0.06139858)),
        dot(linear_r2020, vec3<f32>(-0.06529745,  1.08782226, -0.02252481)),
        dot(linear_r2020, vec3<f32>( 0.00282179, -0.02598807,  1.02316628))
    );
}

fn grid_prepare_output_rgb(srgb: vec3<f32>, input_colorspace: u32) -> vec3<f32> {
    var lin = grid_srgb_to_linear(srgb);
    if (input_colorspace == 0u) {
        lin = grid_srgb_to_p3(lin);
    } else if (input_colorspace == 2u) {
        lin = grid_rec2020_to_p3(lin);
    }
    return grid_linear_to_srgb(lin);
}

const PAD_EXTEND_LEFT:  u32 = 1u;
const PAD_EXTEND_RIGHT: u32 = 2u;
const PAD_EXTEND_UP:    u32 = 4u;
const PAD_EXTEND_DOWN:  u32 = 8u;

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// CellBg is `#[repr(C)] struct { rgba: [u8; 4], pixel_offset_y: i32 }`.
// WGSL has no u8, so rgba is read as a packed little-endian u32.
struct CellBg {
    rgba: u32,
    pixel_offset_y: i32,
};

@group(0) @binding(1) var<storage, read> cells: array<CellBg>;

struct VsOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn grid_bg_vertex(@builtin(vertex_index) vid: u32) -> VsOut {
 // Fullscreen triangle (same trick as the Metal port).
 // vid 0: (-1, -3)
 // vid 1: (-1, 1)
 // vid 2: ( 3, 1)
    var x = -1.0;
    var y =  1.0;
    if (vid == 2u) { x =  3.0; }
    if (vid == 0u) { y = -3.0; }

    var out: VsOut;
    out.position = vec4<f32>(x, y, 1.0, 1.0);
    return out;
}

fn load_cell_bg(cell: CellBg) -> vec4<f32> {
 // One u32 per cell; unpack RGBA little-endian bytes.
    let word = cell.rgba;
    let r = f32((word >>  0u) & 0xFFu) / 255.0;
    let g = f32((word >>  8u) & 0xFFu) / 255.0;
    let b = f32((word >> 16u) & 0xFFu) / 255.0;
    let a = f32((word >> 24u) & 0xFFu) / 255.0;
 // Premultiply.
    return vec4<f32>(r * a, g * a, b * a, a);
}

@fragment
fn grid_bg_fragment(in: VsOut) -> @location(0) vec4<f32> {
    if (uniforms.clip_rect.z > 0.0 && uniforms.clip_rect.w > 0.0) {
        let px = in.position.x;
        let py = in.position.y;
        if (px < uniforms.clip_rect.x
            || px >= uniforms.clip_rect.x + uniforms.clip_rect.z
            || py < uniforms.clip_rect.y
            || py >= uniforms.clip_rect.y + uniforms.clip_rect.w) {
            return vec4<f32>(0.0);
        }
    }

 // `grid_padding` is (top, right, bottom, left).
 // Use .w (left) + .x (top) to find the grid origin, same as Metal port.
    let cell_fx = (in.position.xy - vec2<f32>(uniforms.grid_padding.w, uniforms.grid_padding.x))
                    / uniforms.cell_size;
    let orig_grid_pos = vec2<i32>(floor(cell_fx));
    var grid_pos = orig_grid_pos;

 // Horizontal padding.
    let cols = i32(uniforms.grid_size.x);
    if (grid_pos.x < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_LEFT) != 0u) {
            grid_pos.x = 0;
        } else {
            return vec4<f32>(0.0);
        }
    } else if (grid_pos.x > cols - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_RIGHT) != 0u) {
            grid_pos.x = cols - 1;
        } else {
            return vec4<f32>(0.0);
        }
    }

 // Vertical padding.
    let rows = i32(uniforms.grid_size.y);
    if (grid_pos.y < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_UP) != 0u) {
            grid_pos.y = 0;
        } else {
            return vec4<f32>(0.0);
        }
    } else if (grid_pos.y > rows - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_DOWN) != 0u) {
            grid_pos.y = rows - 1;
        } else {
            return vec4<f32>(0.0);
        }
    }

 // Cursor overlay at in-bounds cursor cell only (skip
 // padding-extended fragments so an edge cursor doesn't bleed
 // into the window margin).
    if (uniforms.cursor_bg_color.a > 0.0
        && orig_grid_pos.x == i32(uniforms.cursor_pos.x)
        && orig_grid_pos.y == i32(uniforms.cursor_pos.y)) {
        let rgb = grid_prepare_output_rgb(
            uniforms.cursor_bg_color.rgb,
            uniforms.input_colorspace,
        );
        let a = uniforms.cursor_bg_color.a;
        return vec4<f32>(rgb * a, a);
    }

 // Load cell, convert to output color space, then premultiply.
 // Same pipeline as the quad fill in `sugarloaf/src/renderer/renderer.metal`
 // so the grid and window-fill paths produce identical framebuffer
 // values.
    var idx = u32(grid_pos.y) * uniforms.grid_size.x + u32(grid_pos.x);
    var cell = cells[idx];
 // Smooth-scroll offset uses a uniform for whole-pane scroll and an
 // optional per-cell addend.
    var cell_pixel_offset_y = f32(cell.pixel_offset_y);
    // Raw float offset (NOT round()): the bg slides at sub-pixel
    // granularity so text snapping to integer pixels (via combined
    // round in the text vertex shader) stays aligned with the bg
    // cell underneath it. Rounding the offset alone here caused the
    // bg to jump in 1-pixel steps every half-pixel of spring decay,
    // while text rounding to nearest pixel followed a different
    // schedule — text appeared to swim inside its cell during long
    // held-arrow scroll. Matches Ghostty's `cell_bg.f.glsl`.
    let editor_pixel_offset_y = uniforms.editor_pixel_offset_y + cell_pixel_offset_y;
    if (editor_pixel_offset_y != 0.0) {
        let adjusted = in.position.xy - vec2<f32>(0.0, editor_pixel_offset_y);
        let shifted = vec2<i32>(floor((adjusted - vec2<f32>(uniforms.grid_padding.w, uniforms.grid_padding.x)) / uniforms.cell_size));
        if (shifted.x >= 0 && shifted.x < cols && shifted.y >= 0 && shifted.y < rows) {
            grid_pos = shifted;
            idx = u32(grid_pos.y) * uniforms.grid_size.x + u32(grid_pos.x);
            cell = cells[idx];
        } else {
            return vec4<f32>(0.0);
        }
    }
    let word = cell.rgba;
    let r = f32((word >>  0u) & 0xFFu) / 255.0;
    let g = f32((word >>  8u) & 0xFFu) / 255.0;
    let b = f32((word >> 16u) & 0xFFu) / 255.0;
    let a = f32((word >> 24u) & 0xFFu) / 255.0;
    let rgb = grid_prepare_output_rgb(vec3<f32>(r, g, b), uniforms.input_colorspace);
    return vec4<f32>(rgb * a, a);
}

// -------------------------------------------------------------------
// Cell Text Shader
//
// WGSL twin of `grid_text_vertex` / `grid_text_fragment` in grid.metal.
// Same simplifications: no full Display P3 / linear-blending chain,
// no min-contrast, single-cell cursor only.
// -------------------------------------------------------------------

const ATLAS_GRAYSCALE: u32 = 0u;
const ATLAS_COLOR:     u32 = 1u;

const BOOL_NO_MIN_CONTRAST: u32 = 1u;
const BOOL_IS_CURSOR_GLYPH: u32 = 2u;

struct CellTextVertexIn {
 // Per-instance attributes (attribute locations match the wgpu
 // vertex buffer layout in grid/webgpu.rs).
    @location(0) glyph_pos:  vec2<u32>,
    @location(1) glyph_size: vec2<u32>,
    @location(2) bearings:   vec2<i32>,
    @location(3) grid_pos:   vec2<u32>,
    @location(4) color:      vec4<f32>,   // UNorm8x4 input, 0..1
    @location(5) atlas:      u32,
    @location(6) bools:      u32,
    @location(7) pixel_offset_y: i32,
};

struct TextVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) atlas: u32,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) tex_coord: vec2<f32>,
    @location(3) @interpolate(flat) clip_rect: vec4<f32>,
};

// Atlases. Group(1) keeps them separate from the bg bind group so
// the bg pipeline (which doesn't need atlases) uses a smaller bind
// group layout. We use `textureLoad` (no sampler) to match Metal's
// `coord::pixel + filter::nearest` — integer pixel fetch.
@group(1) @binding(0) var atlas_grayscale: texture_2d<f32>;
@group(1) @binding(1) var atlas_color:     texture_2d<f32>;

@vertex
fn grid_text_vertex(
    @builtin(vertex_index) vid: u32,
    in: CellTextVertexIn,
) -> TextVsOut {
 // Cell origin in pixel space.
    let cell_pos = uniforms.cell_size * vec2<f32>(in.grid_pos);

 // Quad corner (0..1) from vertex id — 4-vertex triangle strip.
 // 0 --> 1
 // | .'|
 // | / |
 // | L |
 // 2 --> 3
    var corner: vec2<f32>;
    corner.x = select(0.0, 1.0, vid == 1u || vid == 3u);
    corner.y = select(0.0, 1.0, vid == 2u || vid == 3u);

 // Glyph bbox inside cell: bearings.x from left, bearings.y from
 // bottom (font convention).
    let size = vec2<f32>(in.glyph_size);
    var offset = vec2<f32>(in.bearings);
    offset.y = uniforms.cell_size.y - offset.y;

    var quad = cell_pos + size * corner + offset;

 // Shift by grid_padding (top/left).
    quad.x += uniforms.grid_padding.w;
    quad.y += uniforms.grid_padding.x;
    quad.x = round(quad.x);

    var glyph_pixel_offset_y = f32(in.pixel_offset_y);
    var clip_rect = uniforms.clip_rect;
    // Round the COMBINED sum (quad.y + offset + per-cell offset)
    // as a single operation. Matches Ghostty's `cell_text.v.glsl`
    // and the bg shader's now-raw offset shift, so the half-pixel
    // pivots line up: when the spring's float position crosses any
    // half-pixel, BOTH bg and text re-evaluate their integer
    // snap at the same moment.
    //
    // The earlier `round(quad.y) + round(po)` form had bg and text
    // rounding at DIFFERENT po thresholds (bg jumped at po=±0.5,
    // text jumped wherever `round(quad.y+po)` crossed an integer,
    // which depended on quad.y's fractional part). During long
    // held-arrow scroll the text appeared to swim 1 px inside its
    // bg cell every couple frames.
    quad.y = round(quad.y + uniforms.editor_pixel_offset_y + glyph_pixel_offset_y);

    var out: TextVsOut;
    out.position = uniforms.projection * vec4<f32>(quad, 0.0, 1.0);

 // Atlas tex coords in PIXEL space — sampler is set to nearest,
 // unnormalized coords equivalent via textureLoad below.
    out.tex_coord = vec2<f32>(in.glyph_pos) + vec2<f32>(in.glyph_size) * corner;
    out.atlas = in.atlas;
    out.clip_rect = clip_rect;

 // Foreground color — `in.color` arrives normalized via UNorm8x4.
 // Convert to output color space first, then premultiply. Same
 // pipeline as `grid_bg_fragment` and the quad fill so glyph/cell
 // bg/window bg agree.
    var color = in.color;
    color = vec4<f32>(
        grid_prepare_output_rgb(color.rgb, uniforms.input_colorspace) * color.a,
        color.a,
    );

 // Cursor-pos fg swap. Skip when cursor_color.a == 0 — that's the
 // hollow / unfocused path where text colour stays untouched.
    let is_cursor_pos = in.grid_pos.x == uniforms.cursor_pos.x
                     && in.grid_pos.y == uniforms.cursor_pos.y;
    if ((in.bools & BOOL_IS_CURSOR_GLYPH) == 0u
        && is_cursor_pos
        && uniforms.cursor_color.a > 0.0) {
        let c = uniforms.cursor_color;
        color = vec4<f32>(
            grid_prepare_output_rgb(c.rgb, uniforms.input_colorspace) * c.a,
            c.a,
        );
    }

    out.color = color;
    return out;
}

@fragment
fn grid_text_fragment(in: TextVsOut) -> @location(0) vec4<f32> {
    if (in.clip_rect.z > 0.0 && in.clip_rect.w > 0.0) {
        let px = in.position.x;
        let py = in.position.y;
        if (px < in.clip_rect.x
            || px >= in.clip_rect.x + in.clip_rect.z
            || py < in.clip_rect.y
            || py >= in.clip_rect.y + in.clip_rect.w) {
            discard;
        }
    }

 // Pixel-space tex_coord → integer sample via textureLoad (no
 // sampler filter; matches Metal's `coord::pixel` + `filter::nearest`).
    let ic = vec2<i32>(in.tex_coord);
    if (in.atlas == ATLAS_GRAYSCALE) {
        let a = textureLoad(atlas_grayscale, ic, 0).r;
        return in.color * a;
    } else {
        return textureLoad(atlas_color, ic, 0);
    }
}

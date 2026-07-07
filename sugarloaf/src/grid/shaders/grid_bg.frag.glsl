#version 450

// Per-cell background fragment shader. One sample per framebuffer pixel:
// look up the owning grid cell, apply `padding_extend` clamping at the
// edges, paint the cursor color where applicable, otherwise return the
// stored CellBg colour through the sRGB↔DisplayP3 chain.
//
// Ported 1:1 from `grid_bg_fragment` in
// `sugarloaf/src/grid/shaders/grid.metal`. Field order in `Uniforms`
// must match `GridUniforms` in `sugarloaf/src/grid/cell.rs` and the
// Metal `Uniforms` struct in grid.metal — std140 layout naturally
// matches the hand-packed byte layout used over there.

// `Uniforms` mirrors `GridUniforms` (256B = 16 × vec4 blocks under
// std140). See cell.rs for offsets / sizes; the fields here are listed
// in the same order.
layout(set = 0, binding = 0, std140) uniform Uniforms {
    mat4 projection;        // offset   0
    vec4 grid_padding;      // offset  64  (top, right, bottom, left)
    vec4 clip_rect;         // offset  80  (x, y, w, h), 0 size disables
    vec4 cursor_color;      // offset  96
    vec4 cursor_bg_color;   // offset 112
    vec2 cell_size;         // offset 128
    uvec2 grid_size;        // offset 136
    uvec2 cursor_pos;       // offset 144
    uvec2 _pad_cursor;      // offset 152
    float min_contrast;     // offset 160
    uint flags;             // offset 164
    uint padding_extend;    // offset 168
    uint input_colorspace;  // offset 172
    float editor_pixel_offset_y; // offset 176 — uniform smooth-scroll offset
    uint _pad_editor_0;     // offset 180
    uint _pad_editor_1;     // offset 184
    uint _pad_editor_2;     // offset 188
} uniforms;

// `CellBg` is { packed rgba, pixel_offset_y } in cell.rs.
struct CellBgData {
    uint rgba;
    int pixel_offset_y;
};

layout(set = 0, binding = 1, std430) readonly buffer Cells {
    CellBgData cells[];
};

layout(location = 0) out vec4 out_color;

const uint PAD_EXTEND_LEFT  = 1u << 0;
const uint PAD_EXTEND_RIGHT = 1u << 1;
const uint PAD_EXTEND_UP    = 1u << 2;
const uint PAD_EXTEND_DOWN  = 1u << 3;

// ----- colorspace helpers (mirror `grid.metal` 1:1) -----

vec3 grid_srgb_to_linear(vec3 c) {
    vec3 lo = c / 12.92;
    vec3 hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return mix(lo, hi, greaterThan(c, vec3(0.04045)));
}

vec3 grid_linear_to_srgb(vec3 c) {
    vec3 lo = c * 12.92;
    vec3 hi = pow(c, vec3(1.0 / 2.4)) * 1.055 - 0.055;
    return mix(lo, hi, greaterThan(c, vec3(0.0031308)));
}

vec3 grid_srgb_to_p3(vec3 linear_srgb) {
    return vec3(
        dot(linear_srgb, vec3(0.82246197, 0.17753803, 0.0)),
        dot(linear_srgb, vec3(0.03319420, 0.96680580, 0.0)),
        dot(linear_srgb, vec3(0.01708263, 0.07239744, 0.91051993))
    );
}

vec3 grid_rec2020_to_p3(vec3 linear_r2020) {
    return vec3(
        dot(linear_r2020, vec3( 1.34357825, -0.28217967, -0.06139858)),
        dot(linear_r2020, vec3(-0.06529745,  1.08782226, -0.02252481)),
        dot(linear_r2020, vec3( 0.00282179, -0.02598807,  1.02316628))
    );
}

vec3 grid_prepare_output_rgb(vec3 srgb, uint input_colorspace) {
    vec3 lin = grid_srgb_to_linear(srgb);
    if (input_colorspace == 0u) {
        lin = grid_srgb_to_p3(lin);
    } else if (input_colorspace == 2u) {
        lin = grid_rec2020_to_p3(lin);
    }
    return grid_linear_to_srgb(lin);
}

void main() {
    if (uniforms.clip_rect.z > 0.0 && uniforms.clip_rect.w > 0.0) {
        if (gl_FragCoord.x < uniforms.clip_rect.x
            || gl_FragCoord.x >= uniforms.clip_rect.x + uniforms.clip_rect.z
            || gl_FragCoord.y < uniforms.clip_rect.y
            || gl_FragCoord.y >= uniforms.clip_rect.y + uniforms.clip_rect.w) {
            out_color = vec4(0.0);
            return;
        }
    }

    // `gl_FragCoord.xy` is the pixel center in framebuffer pixels.
    // Locate the owning grid cell relative to the grid origin
    // (top-left = grid_padding.w / .x).
    ivec2 orig_grid_pos = ivec2(
        floor((gl_FragCoord.xy - uniforms.grid_padding.wx) / uniforms.cell_size)
    );
    ivec2 grid_pos = orig_grid_pos;

    // Horizontal padding extend / discard.
    if (grid_pos.x < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_LEFT) != 0u) {
            grid_pos.x = 0;
        } else {
            out_color = vec4(0.0);
            return;
        }
    } else if (grid_pos.x > int(uniforms.grid_size.x) - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_RIGHT) != 0u) {
            grid_pos.x = int(uniforms.grid_size.x) - 1;
        } else {
            out_color = vec4(0.0);
            return;
        }
    }

    // Vertical padding extend / discard.
    if (grid_pos.y < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_UP) != 0u) {
            grid_pos.y = 0;
        } else {
            out_color = vec4(0.0);
            return;
        }
    } else if (grid_pos.y > int(uniforms.grid_size.y) - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_DOWN) != 0u) {
            grid_pos.y = int(uniforms.grid_size.y) - 1;
        } else {
            out_color = vec4(0.0);
            return;
        }
    }

    // Cursor block fill: only when this fragment's *original* grid_pos
    // (pre-clamp) matches the cursor cell. Bypassing the clamp here
    // keeps the cursor from leaking into the margin on edge rows.
    if (uniforms.cursor_bg_color.a > 0.0
        && orig_grid_pos.x == int(uniforms.cursor_pos.x)
        && orig_grid_pos.y == int(uniforms.cursor_pos.y))
    {
        vec4 c = uniforms.cursor_bg_color;
        c.rgb = grid_prepare_output_rgb(c.rgb, uniforms.input_colorspace);
        c.rgb *= c.a;
        out_color = c;
        return;
    }

    // Apply smooth-scroll offset by sampling the source cell that moved
    // under this fragment, while the grid origin itself stays fixed.
    //
    // Use the raw float offset (NOT round()): this lets the bg cell
    // boundary slide at sub-pixel granularity as the spring decays.
    // The text vertex shader already rounds the SUM of quad.y + offset
    // — so text snaps to integer pixels, and the bg cell underneath it
    // slides smoothly. Ghostty's `cell_bg.f.glsl` does the same; we
    // were previously rounding the offset in isolation here, which
    // made the bg shift in 1-pixel jumps every time the offset
    // crossed a half-pixel boundary while the text rounding stayed
    // on the same integer pixel. Result: text appeared to "swim"
    // inside its cell during long held-arrow scroll. With raw offset
    // here, every half-pixel of spring motion smoothly moves the
    // bg under the text — no per-frame integer-step desync.
    uint idx = uint(grid_pos.y) * uniforms.grid_size.x + uint(grid_pos.x);
    CellBgData cell = cells[idx];
    float cell_pixel_offset_y = float(cell.pixel_offset_y);
    float editor_pixel_offset_y = uniforms.editor_pixel_offset_y + cell_pixel_offset_y;
    if (editor_pixel_offset_y != 0.0) {
        vec2 adjusted = gl_FragCoord.xy - vec2(0.0, editor_pixel_offset_y);
        ivec2 shifted = ivec2(
            floor((adjusted - uniforms.grid_padding.wx) / uniforms.cell_size)
        );
        if (shifted.x >= 0 && shifted.x < int(uniforms.grid_size.x) &&
            shifted.y >= 0 && shifted.y < int(uniforms.grid_size.y)) {
            grid_pos = shifted;
            idx = uint(grid_pos.y) * uniforms.grid_size.x + uint(grid_pos.x);
            cell = cells[idx];
        } else {
            out_color = vec4(0.0);
            return;
        }
    }

    // Load + decode the CellBg.
    vec4 color = unpackUnorm4x8(cell.rgba);
    color.rgb = grid_prepare_output_rgb(color.rgb, uniforms.input_colorspace);
    color.rgb *= color.a;

    out_color = color;
}

//! Conversion glue between `neoism_terminal_core::graphics` and
//! `sugarloaf::graphics`.
//!
//! Phase 3b: the ANSI graphics state machines (sixel decoder, kitty
//! graphics protocol, iTerm2 inline image protocol) all moved into
//! `neoism-terminal-core`. To keep the engine wasm-clean, the
//! `GraphicData` payload it produces is the plain-data mirror defined
//! in `neoism_terminal_core::graphics`. The native renderer in
//! sugarloaf still operates on `sugarloaf::GraphicData`. This module
//! holds the conversion helpers that bridge the two — both
//! `neoism_terminal_core::graphics::Foo` and `sugarloaf::Foo` are
//! external to the backend crate, so the orphan rule precludes
//! `impl From<...> for ...` here; instead we expose plain `fn`s.

use neoism_terminal_core::graphics as core;

#[inline]
pub fn color_type_to_sugarloaf(value: core::ColorType) -> sugarloaf::ColorType {
    match value {
        core::ColorType::Rgb => sugarloaf::ColorType::Rgb,
        core::ColorType::Rgba => sugarloaf::ColorType::Rgba,
    }
}

#[inline]
pub fn color_type_from_sugarloaf(value: sugarloaf::ColorType) -> core::ColorType {
    match value {
        sugarloaf::ColorType::Rgb => core::ColorType::Rgb,
        sugarloaf::ColorType::Rgba => core::ColorType::Rgba,
    }
}

#[inline]
pub fn graphic_id_to_sugarloaf(value: core::GraphicId) -> sugarloaf::GraphicId {
    sugarloaf::GraphicId::new(value.0)
}

#[inline]
pub fn graphic_id_from_sugarloaf(value: sugarloaf::GraphicId) -> core::GraphicId {
    core::GraphicId::new(value.get())
}

#[inline]
pub fn resize_parameter_to_sugarloaf(
    value: core::ResizeParameter,
) -> sugarloaf::ResizeParameter {
    match value {
        core::ResizeParameter::Auto => sugarloaf::ResizeParameter::Auto,
        core::ResizeParameter::Cells(n) => sugarloaf::ResizeParameter::Cells(n),
        core::ResizeParameter::Pixels(n) => sugarloaf::ResizeParameter::Pixels(n),
        core::ResizeParameter::WindowPercent(n) => {
            sugarloaf::ResizeParameter::WindowPercent(n)
        }
    }
}

#[inline]
pub fn resize_parameter_from_sugarloaf(
    value: sugarloaf::ResizeParameter,
) -> core::ResizeParameter {
    match value {
        sugarloaf::ResizeParameter::Auto => core::ResizeParameter::Auto,
        sugarloaf::ResizeParameter::Cells(n) => core::ResizeParameter::Cells(n),
        sugarloaf::ResizeParameter::Pixels(n) => core::ResizeParameter::Pixels(n),
        sugarloaf::ResizeParameter::WindowPercent(n) => {
            core::ResizeParameter::WindowPercent(n)
        }
    }
}

#[inline]
pub fn resize_command_to_sugarloaf(
    value: core::ResizeCommand,
) -> sugarloaf::ResizeCommand {
    sugarloaf::ResizeCommand {
        width: resize_parameter_to_sugarloaf(value.width),
        height: resize_parameter_to_sugarloaf(value.height),
        preserve_aspect_ratio: value.preserve_aspect_ratio,
    }
}

#[inline]
pub fn resize_command_from_sugarloaf(
    value: sugarloaf::ResizeCommand,
) -> core::ResizeCommand {
    core::ResizeCommand {
        width: resize_parameter_from_sugarloaf(value.width),
        height: resize_parameter_from_sugarloaf(value.height),
        preserve_aspect_ratio: value.preserve_aspect_ratio,
    }
}

#[inline]
pub fn graphic_data_to_sugarloaf(value: core::GraphicData) -> sugarloaf::GraphicData {
    sugarloaf::GraphicData {
        id: graphic_id_to_sugarloaf(value.id),
        width: value.width,
        height: value.height,
        color_type: color_type_to_sugarloaf(value.color_type),
        pixels: value.pixels,
        is_opaque: value.is_opaque,
        resize: value.resize.map(resize_command_to_sugarloaf),
        display_width: value.display_width,
        display_height: value.display_height,
        transmit_time: value.transmit_time,
    }
}

#[inline]
pub fn graphic_data_from_sugarloaf(value: sugarloaf::GraphicData) -> core::GraphicData {
    core::GraphicData {
        id: graphic_id_from_sugarloaf(value.id),
        width: value.width,
        height: value.height,
        color_type: color_type_from_sugarloaf(value.color_type),
        pixels: value.pixels,
        is_opaque: value.is_opaque,
        resize: value.resize.map(resize_command_from_sugarloaf),
        display_width: value.display_width,
        display_height: value.display_height,
        transmit_time: value.transmit_time,
    }
}

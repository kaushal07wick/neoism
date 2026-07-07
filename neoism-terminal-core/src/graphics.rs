//! Plain-data mirrors of `sugarloaf::graphics` types.
//!
//! `neoism-terminal-core` must stay wasm-clean — no sugarloaf, no wgpu.
//! The ANSI graphics state machines (sixel decoder, kitty graphics
//! protocol, iTerm2 inline images) all need a `GraphicData`-shaped
//! payload to communicate decoded pixel buffers back to the host
//! renderer. We define a structurally identical mirror here. The
//! native adapter in `neoism-backend` converts between
//! `crate::graphics::GraphicData` and
//! `sugarloaf::GraphicData` at the renderer boundary via `From` impls.
//!
//! Fields and method semantics are byte-for-byte equivalent to
//! sugarloaf's definitions so the conversion is a trivial field-by-
//! field move.
//!
//! `image_rs` is intentionally retained as a dependency here because
//! the iTerm2 inline image protocol decodes base64-encoded PNG/JPEG
//! into a `DynamicImage` and immediately surfaces it as `GraphicData`.
//! `image_rs` is wasm-compatible with the codec features the workspace
//! uses (`gif`, `jpeg`, `ico`, `png`, `pnm`, `webp`, `bmp`).

use image_rs::DynamicImage;
use std::cmp;

/// Maximum width and height (in pixels) allowed for a graphic.
pub const MAX_GRAPHIC_DIMENSIONS: [usize; 2] = [4096, 4096];

/// Unique identifier for every graphic added to a grid.
///
/// An id of 0 represents a temporary, non-referenceable image
/// (matching kitty's behavior).
#[derive(Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord)]
pub struct GraphicId(pub u64);

impl GraphicId {
    /// Create a new GraphicId from a u64 value.
    #[inline]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the inner u64 value.
    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Specifies the format of the pixel data.
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,

    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
}

/// Unit to specify a dimension to resize the graphic.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum ResizeParameter {
    /// Dimension is computed from the original graphic dimensions.
    Auto,

    /// Size is specified in number of grid cells.
    Cells(u32),

    /// Size is specified in number pixels.
    Pixels(u32),

    /// Size is specified in a percent of the window.
    WindowPercent(u32),
}

/// Dimensions to resize a graphic.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub struct ResizeCommand {
    pub width: ResizeParameter,

    pub height: ResizeParameter,

    pub preserve_aspect_ratio: bool,
}

/// Defines a single graphic read from the PTY.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct GraphicData {
    /// Graphics identifier.
    pub id: GraphicId,

    /// Width, in pixels, of the graphic.
    pub width: usize,

    /// Height, in pixels, of the graphic.
    pub height: usize,

    /// Color type of the pixels.
    pub color_type: ColorType,

    /// Pixels data.
    pub pixels: Vec<u8>,

    /// Indicate if there are no transparent pixels.
    pub is_opaque: bool,

    /// Render graphic in a different size.
    pub resize: Option<ResizeCommand>,

    /// Display width in pixels (set when GPU scaling is used instead of
    /// CPU resize). If None, display at the original pixel width.
    pub display_width: Option<usize>,

    /// Display height in pixels (set when GPU scaling is used instead
    /// of CPU resize). If None, display at the original pixel height.
    pub display_height: Option<usize>,

    /// Generation counter for cache invalidation.
    /// Incremented when image data changes (re-transmission with same ID).
    pub transmit_time: web_time::Instant,
}

impl GraphicData {
    /// Check if the image may contain transparent pixels. If it
    /// returns `false`, it is guaranteed that there are no transparent
    /// pixels.
    #[inline]
    pub fn maybe_transparent(&self) -> bool {
        !self.is_opaque && self.color_type == ColorType::Rgba
    }

    /// Check if all pixels under a region are opaque.
    ///
    /// If the region exceeds the boundaries of the image it is
    /// considered as not filled.
    pub fn is_filled(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        if x + width >= self.width || y + height >= self.height {
            return false;
        }

        if !self.maybe_transparent() {
            return true;
        }

        debug_assert!(self.color_type == ColorType::Rgba);

        for offset_y in y..y + height {
            let offset = offset_y * self.width * 4;
            let row = &self.pixels[offset..offset + width * 4];

            if row.chunks_exact(4).any(|pixel| pixel.last() != Some(&255)) {
                return false;
            }
        }

        true
    }

    /// Build a `GraphicData` from an `image_rs::DynamicImage`.
    pub fn from_dynamic_image(id: GraphicId, image: DynamicImage) -> Self {
        let color_type;
        let width;
        let height;
        let pixels;

        match image {
            DynamicImage::ImageRgba8(image) => {
                color_type = ColorType::Rgba;
                width = image.width() as usize;
                height = image.height() as usize;
                pixels = image.into_raw();
            }

            _ => {
                let image = image.into_rgba8();
                color_type = ColorType::Rgba;
                width = image.width() as usize;
                height = image.height() as usize;
                pixels = image.into_raw();
            }
        }

        GraphicData {
            id,
            width,
            height,
            color_type,
            pixels,
            is_opaque: false,
            resize: None,
            display_width: None,
            display_height: None,
            transmit_time: web_time::Instant::now(),
        }
    }

    /// Compute the display dimensions for this graphic without
    /// modifying pixels. Returns (display_width, display_height) in
    /// pixels. If no resize is needed, returns the original dimensions.
    pub fn compute_display_dimensions(
        &self,
        cell_width: usize,
        cell_height: usize,
        view_width: usize,
        view_height: usize,
    ) -> (usize, usize) {
        let resize = match self.resize {
            Some(resize) => resize,
            None => return (self.width, self.height),
        };

        if (resize.width == ResizeParameter::Auto
            && resize.height == ResizeParameter::Auto)
            || self.height == 0
            || self.width == 0
        {
            return (self.width, self.height);
        }

        let mut width = match resize.width {
            ResizeParameter::Auto => 1,
            ResizeParameter::Pixels(n) => n as usize,
            ResizeParameter::Cells(n) => n as usize * cell_width,
            ResizeParameter::WindowPercent(n) => n as usize * view_width / 100,
        };

        let mut height = match resize.height {
            ResizeParameter::Auto => 1,
            ResizeParameter::Pixels(n) => n as usize,
            ResizeParameter::Cells(n) => n as usize * cell_height,
            ResizeParameter::WindowPercent(n) => n as usize * view_height / 100,
        };

        if width == 0 || height == 0 {
            return (self.width, self.height);
        }

        if resize.width == ResizeParameter::Auto {
            width =
                (self.width as f64 * height as f64 / self.height as f64).round() as usize;
        }

        if resize.height == ResizeParameter::Auto {
            height =
                (self.height as f64 * width as f64 / self.width as f64).round() as usize;
        }

        width = cmp::min(width, MAX_GRAPHIC_DIMENSIONS[0]);
        height = cmp::min(height, MAX_GRAPHIC_DIMENSIONS[1]);

        if resize.preserve_aspect_ratio {
            let scale_w = width as f64 / self.width as f64;
            let scale_h = height as f64 / self.height as f64;
            let scale = scale_w.min(scale_h);
            width = (self.width as f64 * scale).round() as usize;
            height = (self.height as f64 * scale).round() as usize;
        }

        (width, height)
    }
}

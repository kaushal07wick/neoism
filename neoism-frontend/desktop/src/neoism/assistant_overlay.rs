// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Desktop adapter for the shared top-right assistant overlay.
//!
//! The visual + hit-test code lives in
//! [`neoism_ui::panels::assistant_overlay::AssistantOverlay`]. This
//! file only handles the `RioError → AssistantOverlayPayload` shim so
//! existing desktop call sites (`set_error(RioError)`) keep working
//! without dragging `neoism_backend` into the shared crate.

use neoism_backend::error::{RioError, RioErrorLevel};
use neoism_backend::sugarloaf::Sugarloaf;

use neoism_ui::panels::assistant_overlay::{
    AssistantOverlay as SharedOverlay, AssistantOverlayLevel, AssistantOverlayPayload,
};

pub use neoism_ui::panels::assistant_overlay::AssistantOverlayAction;

#[derive(Default)]
pub struct AssistantOverlay {
    inner: SharedOverlay,
}

impl AssistantOverlay {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    /// Convert the `RioError` into the shared crate's POD payload and
    /// hand it to the visual overlay.
    #[inline]
    pub fn set_error(&mut self, error: RioError) {
        let level = match error.level {
            RioErrorLevel::Error => AssistantOverlayLevel::Error,
            RioErrorLevel::Warning => AssistantOverlayLevel::Warning,
        };
        let message = error.report.to_string();
        self.inner
            .set_payload(AssistantOverlayPayload { level, message });
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[inline]
    pub fn hovered_button(&self) -> Option<AssistantOverlayAction> {
        self.inner.hovered_button()
    }

    #[inline]
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> Result<Option<AssistantOverlayAction>, ()> {
        self.inner
            .hit_test(mouse_x, mouse_y, window_width, scale_factor)
    }

    #[inline]
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> bool {
        self.inner
            .hover(mouse_x, mouse_y, window_width, scale_factor)
    }

    #[inline]
    pub fn render(&mut self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        self.inner.render(sugarloaf, dimensions);
    }
}

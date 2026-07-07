use crate::sugarloaf::{Colorspace, SugarloafWindow, SugarloafWindowSize};
use crate::SugarloafRenderer;

pub struct WgpuContext<'a> {
    pub device: wgpu::Device,
    pub surface: wgpu::Surface<'a>,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    alpha_mode: wgpu::CompositeAlphaMode,
    pub adapter_info: wgpu::AdapterInfo,
    surface_caps: wgpu::SurfaceCapabilities,
    surface_ready: bool,
    pub size: SugarloafWindowSize,
    pub scale: f32,
    pub supports_f16: bool,
    pub colorspace: Colorspace,
    pub max_texture_dimension_2d: u32,
}

impl<'a> WgpuContext<'a> {
    /// Synchronous constructor (native targets). Uses
    /// `futures::executor::block_on` to await wgpu's async adapter +
    /// device requests; not available on `wasm32-unknown-unknown`
    /// where `block_on` panics inside the browser event loop. Browser
    /// callers must use `new_async`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        sugarloaf_window: SugarloafWindow,
        renderer_config: SugarloafRenderer,
        wgpu_backend: wgpu::Backends,
    ) -> WgpuContext<'a> {
        let size = sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        // The backend can be configured using the `WGPU_BACKEND`
        // environment variable. If the variable is not set, the primary backend
        // will be used. The following values are allowed:
        // - `vulkan`
        // - `metal`
        // - `dx12`
        // - `dx11`
        // - `gl`
        // - `webgpu`
        // - `primary`
        let env_backend = std::env::var("WGPU_BACKEND").ok();
        let backend = wgpu::Backends::from_env().unwrap_or(wgpu_backend);
        tracing::info!(
            target: "sugarloaf::context",
            requested_backends = ?wgpu_backend,
            env_backend = ?env_backend,
            selected_backends = ?backend,
            "creating wgpu instance"
        );
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: backend,
            ..Default::default()
        });

        tracing::info!("selected instance: {instance:?}");

        #[cfg(not(target_arch = "wasm32"))]
        {
            tracing::info!("Available adapters:");
            for a in futures::executor::block_on(
                instance.enumerate_adapters(wgpu::Backends::all()),
            ) {
                tracing::info!("    {:?}", a.get_info())
            }
        }

        tracing::info!("initializing the surface");

        let surface: wgpu::Surface<'a> =
            instance.create_surface(sugarloaf_window).unwrap();
        let power_preference =
            match std::env::var("SUGARLOAF_POWER_PREFERENCE").ok().as_deref() {
                Some("low-power") | Some("low_power") | Some("LowPower") => {
                    wgpu::PowerPreference::LowPower
                }
                _ => wgpu::PowerPreference::HighPerformance,
            };

        let adapter = futures::executor::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ))
        .expect("Request adapter");

        let adapter_info = adapter.get_info();
        tracing::info!(?power_preference, "Selected adapter: {:?}", adapter_info);

        let surface_caps = surface.get_capabilities(&adapter);

        #[cfg(target_os = "macos")]
        let format = get_macos_texture_format(renderer_config.colorspace);
        #[cfg(not(target_os = "macos"))]
        let format = find_best_texture_format(
            surface_caps.formats.as_slice(),
            renderer_config.colorspace,
        );

        let (device, queue) = {
            {
                if let Ok(result) = futures::executor::block_on(
                    adapter.request_device(&wgpu::DeviceDescriptor::default()),
                ) {
                    (result.0, result.1)
                } else {
                    // These downlevel limits will allow the code to run on all possible hardware
                    let result = futures::executor::block_on(adapter.request_device(
                        &wgpu::DeviceDescriptor {
                            memory_hints: wgpu::MemoryHints::Performance,
                            label: None,
                            required_features: wgpu::Features::empty(),
                            required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                            ..Default::default()
                        },
                    ))
                    .expect("Request device");
                    (result.0, result.1)
                }
            }
        };

        let alpha_mode = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };

        // Configure view formats for wide color gamut support
        let view_formats = match renderer_config.colorspace {
            Colorspace::DisplayP3 | Colorspace::Rec2020 => {
                // For wide color gamut, we may want to support additional view formats
                // This allows the surface to be viewed in different formats
                vec![format]
            }
            Colorspace::Srgb => {
                vec![]
            }
        };

        let max_texture_dimension_2d = device.limits().max_texture_dimension_2d;
        let (surface_width, surface_height) = Self::clamp_surface_size(
            size.width as u32,
            size.height as u32,
            max_texture_dimension_2d,
        );

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: Self::get_texture_usage(&surface_caps),
                format,
                width: surface_width,
                height: surface_height,
                view_formats,
                alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );

        tracing::info!("Configured colorspace: {:?}", renderer_config.colorspace);
        tracing::info!("Surface format: {:?}", format);
        tracing::info!(
            target: "sugarloaf::context",
            adapter = ?adapter_info,
            format = ?format,
            alpha_mode = ?alpha_mode,
            present_mode = ?wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency = 2,
            width = surface_width,
            height = surface_height,
            "configured wgpu surface"
        );

        WgpuContext {
            device,
            queue,
            surface,
            format,
            alpha_mode,
            size: SugarloafWindowSize {
                width: surface_width as f32,
                height: surface_height as f32,
            },
            scale,
            adapter_info,
            surface_caps,
            surface_ready: true,
            // Always disabled on webgpu
            supports_f16: false,
            colorspace: renderer_config.colorspace,
            max_texture_dimension_2d,
        }
    }

    fn get_texture_usage(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureUsages {
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;

        // COPY_DST and COPY_SRC are required for FiltersBrush
        // But some backends like OpenGL might not support COPY_DST and COPY_SRC
        // https://github.com/emilk/egui/pull/3078

        if caps.usages.contains(wgpu::TextureUsages::COPY_DST) {
            usage |= wgpu::TextureUsages::COPY_DST;
        }

        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }

        usage
    }

    fn clamp_surface_size(width: u32, height: u32, max_size: u32) -> (u32, u32) {
        let width = width.max(1);
        let height = height.max(1);
        let max_size = max_size.max(1);

        if width <= max_size && height <= max_size {
            return (width, height);
        }

        let scale = (max_size as f32 / width as f32).min(max_size as f32 / height as f32);
        let clamped_width = ((width as f32 * scale).floor() as u32).clamp(1, max_size);
        let clamped_height = ((height as f32 * scale).floor() as u32).clamp(1, max_size);
        (clamped_width, clamped_height)
    }

    pub fn max_texture_dimension_2d(&self) -> u32 {
        self.max_texture_dimension_2d
    }

    #[inline]
    pub fn suspend_surface(&mut self) {
        self.surface_ready = false;
        self.size.width = 0.0;
        self.size.height = 0.0;
    }

    #[inline]
    pub fn surface_ready(&self) -> bool {
        self.surface_ready && self.size.width >= 1.0 && self.size.height >= 1.0
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            self.suspend_surface();
            return;
        }

        let (width, height) =
            Self::clamp_surface_size(width, height, self.max_texture_dimension_2d);

        self.size.width = width as f32;
        self.size.height = height as f32;

        // Configure view formats for wide color gamut support
        let view_formats = match self.colorspace {
            Colorspace::DisplayP3 | Colorspace::Rec2020 => {
                vec![self.format]
            }
            Colorspace::Srgb => {
                vec![]
            }
        };

        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: Self::get_texture_usage(&self.surface_caps),
                format: self.format,
                width,
                height,
                view_formats,
                alpha_mode: self.alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );
        self.surface_ready = true;
    }

    #[inline]
    pub fn surface_caps(&self) -> &wgpu::SurfaceCapabilities {
        &self.surface_caps
    }

    #[inline]
    pub fn supports_f16(&self) -> bool {
        self.supports_f16
    }

    #[inline]
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn get_optimal_texture_format(&self) -> wgpu::TextureFormat {
        // wgpu always uses f32 formats, not f16
        wgpu::TextureFormat::Rgba8Unorm
    }

    pub fn get_optimal_texture_sample_type(&self) -> wgpu::TextureSampleType {
        // wgpu uses Rgba8Unorm (f32) with Float sample type and filtering
        wgpu::TextureSampleType::Float { filterable: true }
    }

    pub fn convert_rgba8_to_optimal_format(&self, rgba8_data: &[u8]) -> Vec<u8> {
        // wgpu always uses f32 (Rgba8Unorm), no f16 conversion needed
        rgba8_data.to_vec()
    }

    /// Async constructor for browser targets. The shape mirrors `new`
    /// but `.await`s wgpu's `request_adapter` / `request_device`
    /// instead of calling `futures::executor::block_on`, which panics
    /// under wasm-bindgen's single-threaded event loop.
    ///
    /// `target` is a `wgpu::SurfaceTarget` — typically built from an
    /// `HtmlCanvasElement` via `SurfaceTarget::Canvas(canvas)`. The
    /// initial size + scale come straight from the caller (the canvas
    /// element doesn't know its CSS size at adapter-request time).
    #[cfg(target_arch = "wasm32")]
    pub async fn new_async(
        target: wgpu::SurfaceTarget<'a>,
        size: SugarloafWindowSize,
        scale: f32,
        renderer_config: SugarloafRenderer,
        wgpu_backend: wgpu::Backends,
    ) -> Result<WgpuContext<'a>, String> {
        let backend = wgpu::Backends::from_env().unwrap_or(wgpu_backend);
        tracing::info!(
            target: "sugarloaf::context",
            requested_backends = ?wgpu_backend,
            selected_backends = ?backend,
            "creating wgpu instance (wasm32 / async)"
        );
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: backend,
            ..Default::default()
        });

        let surface = instance
            .create_surface(target)
            .map_err(|e| format!("create_surface failed: {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("request_adapter failed: {e}"))?;

        let adapter_info = adapter.get_info();
        tracing::info!("Selected adapter (wasm32): {:?}", adapter_info);

        let surface_caps = surface.get_capabilities(&adapter);
        let format = find_best_texture_format(
            surface_caps.formats.as_slice(),
            renderer_config.colorspace,
        );

        let (device, queue) = match adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
        {
            Ok((device, queue)) => (device, queue),
            Err(_) => {
                // Fall back to downlevel WebGL2 limits.
                let (device, queue) = adapter
                    .request_device(&wgpu::DeviceDescriptor {
                        memory_hints: wgpu::MemoryHints::Performance,
                        label: None,
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                        ..Default::default()
                    })
                    .await
                    .map_err(|e| format!("request_device failed: {e}"))?;
                (device, queue)
            }
        };

        let alpha_mode = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };

        let view_formats = match renderer_config.colorspace {
            Colorspace::DisplayP3 | Colorspace::Rec2020 => vec![format],
            Colorspace::Srgb => vec![],
        };

        let max_texture_dimension_2d = device.limits().max_texture_dimension_2d;
        let (surface_width, surface_height) = Self::clamp_surface_size(
            size.width as u32,
            size.height as u32,
            max_texture_dimension_2d,
        );

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: Self::get_texture_usage(&surface_caps),
                format,
                width: surface_width,
                height: surface_height,
                view_formats,
                alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );

        Ok(WgpuContext {
            device,
            queue,
            surface,
            format,
            alpha_mode,
            size: SugarloafWindowSize {
                width: surface_width as f32,
                height: surface_height as f32,
            },
            scale,
            adapter_info,
            surface_caps,
            surface_ready: true,
            supports_f16: false,
            colorspace: renderer_config.colorspace,
            max_texture_dimension_2d,
        })
    }
}

#[inline]
#[cfg(not(target_os = "macos"))]
fn find_best_texture_format(
    formats: &[wgpu::TextureFormat],
    colorspace: Colorspace,
) -> wgpu::TextureFormat {
    let mut format: wgpu::TextureFormat = formats.first().unwrap().to_owned();

    // TODO: Fix formats with signs
    // FIXME: On Nvidia GPUs usage Rgba16Float texture format causes driver to enable HDR.
    // Reason for this is currently output color space is poorly defined in wgpu and
    // anything other than Srgb texture formats can cause undeterministic output color
    // space selection which also causes colors to mismatch. Optionally we can whitelist
    // only the Srgb texture formats for now until output color space selection lands in wgpu. See #205
    // TODO: use output color format for the CanvasConfiguration when it lands on the wgpu
    #[cfg(windows)]
    let unsupported_formats = [
        wgpu::TextureFormat::Rgba8Snorm,
        wgpu::TextureFormat::Rgba16Float,
    ];

    // not reproduce-able on mac
    #[cfg(not(windows))]
    let unsupported_formats = [
        wgpu::TextureFormat::Rgba8Snorm,
        // Features::TEXTURE_FORMAT_16BIT_NORM must be enabled to use these texture format.
        wgpu::TextureFormat::R16Unorm,
        wgpu::TextureFormat::R16Snorm,
    ];

    // Bgra8Unorm is the most widely supported and guaranteed format in wgpu
    // Prefer it explicitly if available
    if formats.contains(&wgpu::TextureFormat::Bgra8Unorm) {
        format = wgpu::TextureFormat::Bgra8Unorm;
        tracing::info!(
            "Sugarloaf selected format: {format:?} from {:?} for colorspace {:?}",
            formats,
            colorspace
        );
        return format;
    }

    let filtered_formats: Vec<wgpu::TextureFormat> = formats
        .iter()
        .copied()
        .filter(|&x| {
            // On non-macOS platforms, always avoid sRGB formats
            // This maintains compatibility with existing Linux/Windows color handling
            !wgpu::TextureFormat::is_srgb(&x) && !unsupported_formats.contains(&x)
        })
        .collect();

    // If no compatible formats found, fall back to any non-unsupported format
    let final_formats = if filtered_formats.is_empty() {
        formats
            .iter()
            .copied()
            .filter(|&x| !unsupported_formats.contains(&x))
            .collect()
    } else {
        filtered_formats
    };

    if !final_formats.is_empty() {
        final_formats.first().unwrap().clone_into(&mut format);
    }

    tracing::info!(
        "Sugarloaf selected format: {format:?} from {:?} for colorspace {:?}",
        formats,
        colorspace
    );

    format
}

#[inline]
#[cfg(target_os = "macos")]
fn get_macos_texture_format(colorspace: Colorspace) -> wgpu::TextureFormat {
    match colorspace {
        Colorspace::Srgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        Colorspace::DisplayP3 | Colorspace::Rec2020 => wgpu::TextureFormat::Bgra8Unorm,
    }
}

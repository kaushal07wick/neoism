# Sugarloaf wasm32 Build Audit

Branch: `main` @ `04931fb5`. Probe: `cargo build --target wasm32-unknown-unknown -p sugarloaf` fails with `can't find crate for core/std` — toolchain absent in sandbox, but the dep graph enumerates the remaining issues statically.

## TL;DR

**Feasible, medium effort.** The "WebGPU + WebAssembly" claim is half-true: wasm32 cfg skeleton exists in 5 files plus a 130-line `web-sys` feature list, but it does **not** compile. ~5 deps need substitution, 4 modules need additional cfg gates, and `Sugarloaf::new` depends on `raw-window-handle` types meaningless to a canvas — a `from_canvas` async constructor must be added. Two unconditional `use` statements (`twox_hash`, `wyhash`) are the cheapest wins.

## Current wasm scaffolding (is it real?)

Real for **font fallback bytes** and **wgpu backend selection**; aspirational elsewhere. `FontLibraryData::load` installs bundled CascadiaMono (`font/mod.rs:898`). `SugarloafRenderer::default()` picks `BROWSER_WEBGPU | GL` on wasm32 (`sugarloaf.rs:216`). `font/loader.rs` has `_dummy: u32` placeholders but the module is gated out at `font/mod.rs:5`. **No** wasm context constructor — `WgpuContext::new` calls `instance.create_surface(sugarloaf_window)` expecting `RawWindowHandle`, not `HtmlCanvasElement`. Never wired end-to-end.

## Cargo.toml feature gates that already exist

| Gate | Effect |
|---|---|
| `default = ["scale", "render"]` | swash/zeno glyph eval. |
| `wgpu` (opt-in) | `wgpu` + librashader filter chain. |
| `cfg(wasm32)` deps | `console_error_panic_hook`, `console_log`, `js-sys`, `wasm-bindgen{,-futures}`, `web-sys` (big GPU feature list, `HtmlCanvasElement`, `OffscreenCanvas`). |
| `cfg(not wasm32)` deps | `rayon`, `twox-hash`, `wyhash`, `memmap2`, `font-kit`, `walkdir`, `softbuffer`, `ttf-parser`. |
| `cfg(macos)` deps | metal + CoreText. |
| `cfg(unix not mac)` deps | `fontconfig-parser`, `yeslogic-fontconfig-sys`, `ash`. |

## Deps that work on wasm32 as-is

`bytemuck`, `tracing`, `serde`, `image_rs`, `unicode-width`, `guillotiere`, `rustc-hash`, `raw-window-handle` (types only), `parking_lot`, `dashmap`, `lru`, `smallvec`, `skrifa`, `halfbrown`, `half`, `num-traits`, `yazi`, `zeno`, `swash`, `futures` (types only — executor unusable), `tiny-skia`, `wide`, `thiserror`.

## Deps that need work

| Dep | Failure mode | Substitute |
|---|---|---|
| `twox-hash` | `not(wasm32)`-gated but **used unconditionally** in `components/core/shapes.rs:26`. Hard compile error. | Un-gate (builds on wasm) or swap to `rustc-hash::FxHasher`. |
| `wyhash` | Same — unconditionally `use`d in `layout/render_data.rs:22`. | Un-gate. |
| `font-kit` | CoreText/DirectWrite/FreeType FFI. | Already gated; wasm `family_names` returns `Vec::new()`. Keep. |
| `walkdir`, `memmap2` | `std::fs`. | Already gated; bundled font path uses `&'static [u8]`. |
| `softbuffer` | Owns a `RawWindowHandle` surface. Used by `context/cpu.rs` + `renderer/cpu.rs` + `grid/cpu.rs`. | Gate the entire CPU backend off on wasm — wgpu is the strategic answer. |
| `librashader-*` | `librashader-pack` runtime + reflect dubious on wasm. | Stay behind `wgpu`; consider a `filters` feature so wasm gets wgpu without librashader. |
| `futures::executor::block_on` | `webgpu.rs:57,68,97` — panics in browser. `pollster` also blocks. | Async `Sugarloaf::new_async` via `wasm-bindgen-futures`. |
| `wgpu` 28 | Workspace pin (root `Cargo.toml:76`) lacks `webgpu`/`webgl` features. | See below. |

## Source modules requiring cfg gates

- `components/core/shapes.rs:26` — `twox_hash::XxHash64`. Un-gate dep or swap to FxHasher.
- `layout/render_data.rs:22` — `use wyhash::WyHash;`. Same.
- `context/cpu.rs`, `renderer/cpu.rs`, `grid/cpu.rs` — softbuffer. Gate the CPU backend `#[cfg(not(wasm32))]` at every `pub mod cpu` and every `Cpu` enum/match arm.
- `context/webgpu.rs:57,68,97` — `futures::executor::block_on`. Add `.await` wasm branch.
- `sugarloaf.rs::Sugarloaf::new` — synchronous; add async wasm variant.
- `sugarloaf.rs::set_background_image` — `image_rs::open(path)` does `std::fs`. Gate off on wasm or accept bytes.
- `build.rs` — already no-op on non-linux; fine.

## wgpu feature flags needed for browser targets

Workspace pin (`wgpu = "28.0.0"`) takes default features only. Sugarloaf needs `webgpu` (primary, backs `BROWSER_WEBGPU` at `sugarloaf.rs:218`) and `webgl` (fallback, backs `Backends::GL` at the same line). Override per-target inside sugarloaf rather than the workspace:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
wgpu = { workspace = true, features = ["webgpu", "webgl"], optional = true }
```

Add a `web = ["wgpu"]` feature alias.

## Public API the wasm consumer will call

From `lib.rs` + `sugarloaf.rs`:

```rust
pub struct SugarloafWindow { handle: RawWindowHandle, display: RawDisplayHandle, size, scale }
pub struct SugarloafWindowSize { width: f32, height: f32 }
pub enum SugarloafBackend { Wgpu(wgpu::Backends), /* Metal, Vulkan, Cpu — n/a on web */ }
pub struct SugarloafRenderer { backend, font_features, colorspace }

impl Sugarloaf<'_> {
    pub fn new(window, renderer, &FontLibrary, RootStyle) -> Result<_, _>;
    pub fn render(&mut self);
    pub fn render_with_grids(&mut self, grids: &mut [(&mut GridRenderer, GridUniforms)]);
    pub fn resize(&mut self, w: u32, h: u32);  pub fn rescale(&mut self, scale: f32);
    pub fn content(&mut self) -> &mut Content;  pub fn text(&mut self, id: Option<usize>) -> usize;
    pub fn rect(...); pub fn quad(...); pub fn rounded_rect(...);
    pub fn set_background_color(&mut self, Option<Color>);
}
```

`neoism-terminal-wasm` needs an async `Sugarloaf::from_canvas(canvas: HtmlCanvasElement, …)` bypassing `SugarloafWindow`, using `wgpu::SurfaceTarget::Canvas` (wgpu 28). Same render/content surface after.

## Smallest viable PR to land a building wasm32 sugarloaf

1. Move `twox-hash` and `wyhash` to unconditional `[dependencies]` (both build on wasm).
2. Add `web = ["wgpu"]` feature and a wasm32 `wgpu` entry with `features = ["webgpu", "webgl"]`.
3. Gate the CPU backend off on wasm: `#[cfg(not(wasm32))]` on `pub mod cpu` in `context/`, `renderer/`, `grid/`, plus every `Cpu` enum/match arm.
4. Gate `image_rs::open(path)` in `set_background_image` off on wasm.
5. Add `Sugarloaf::new_async` that `.await`s; gate the existing sync `new` `#[cfg(not(wasm32))]`.
6. Add `WgpuContext::new_async` taking `HtmlCanvasElement` / `wgpu::SurfaceTarget` instead of `SugarloafWindow`.

After these, `cargo build --target wasm32-unknown-unknown -p sugarloaf --features web` should succeed.

## Open questions

- wgpu 28 `SurfaceTarget::Canvas` for `OffscreenCanvas`? Cargo.toml lists `OffscreenCanvas` features — worker-render was planned. Confirm at compile.
- Do `swash` / `skrifa` compile clean on wasm32? Pure-Rust, no `std::fs`/FFI — expect yes; couldn't verify without cargo.
- Does `librashader-reflect` (naga-backed) build on wasm32? If not, split filters behind a `filters` feature distinct from `wgpu`.

//! X11 Present extension integration — true display-vblank vsync.
//!
//! Replaces the calloop periodic `Timer` (whose phase wasn't aligned
//! to actual vsync edges, capping smooth scroll at ~140fps on a 165Hz
//! monitor) with `PresentCompleteNotify` events delivered the moment
//! the X server hits vblank. Same underlying mechanism ghostty / zed
//! / mutter / picom use to lock to real refresh on X11.
//!
//! ## Why the dlopen
//!
//! libxcb has the right primitive for this — `xcb_register_for_special_xge`
//! plus `xcb_poll_for_special_event` divert an extension's GenericEvents
//! out of the main event queue into a per-window/per-eid special queue,
//! so xlib never sees them and there's no contention with the main
//! event drain. x11rb 0.13's public API doesn't expose those functions,
//! and xlib has no typed struct for `PresentCompleteNotifyEvent` (so
//! parsing through `XGetEventData` is a layout-guessing exercise that
//! breaks on different xlib versions). Going one level lower — directly
//! into libxcb via `libloading` — is the cleanest, most-portable option:
//!
//! - libxcb is already loaded into the process by x11rb's `dl-libxcb`
//!   feature, so this is a symbol lookup against an existing handle,
//!   not a second load.
//! - We don't link `libxcb-present`; the only Present-specific datum
//!   we need is the `xcb_extension_t { name: "Present", global_id: 0 }`
//!   struct, which we synthesize ourselves and let `xcb_get_extension_data`
//!   populate `global_id` on first use.
//!
//! ## Lifecycle (per visible window)
//!
//! 1. Allocate an XID (`eid`) and call x11rb's `present_select_input`
//!    on (eid, window, COMPLETE_NOTIFY).
//! 2. `register(conn, eid)` — install a special event queue against
//!    that eid via `xcb_register_for_special_xge`. Subsequent Present
//!    events for this window are diverted there, away from xlib.
//! 3. Arm with `present_notify_msc(window, ..., divisor=1, remainder=0)`
//!    so the server fires at the next vblank.
//! 4. Whenever the X11 fd becomes readable (calloop wakes us), drain
//!    every registered special queue with `poll`. For each event:
//!    fire a vsync hint and re-arm `notify_msc`.
//! 5. On window hide / destroy, drop the `SpecialEvent` handle (RAII
//!    calls `xcb_unregister_for_special_event`).

use std::ffi::c_void;
use std::os::raw::c_int;
use std::sync::OnceLock;

/// Synthetic `xcb_extension_t`. libxcb-present.so exports its own
/// global `xcb_present_id` of this type — we don't link that library,
/// so we declare an equivalent ourselves. `xcb_get_extension_data`
/// fills in `global_id` on the first call; further calls just return
/// the cached info.
///
/// libxcb writes to `global_id` from inside its own functions and
/// synchronizes that access via its connection lock — we never read
/// or write `global_id` from Rust, only hand the pointer to libxcb.
/// To express that "the contents are interior-mutable from C" we
/// wrap the field in `UnsafeCell` (sound; the `*const u8` and
/// `UnsafeCell<c_int>` keep the struct's wire layout intact while
/// satisfying Rust's aliasing rules).
#[repr(C)]
struct XcbExtensionT {
    name: *const u8,
    global_id: std::cell::UnsafeCell<c_int>,
}

// SAFETY: All access to the inner fields happens inside libxcb,
// which serializes access via its connection lock. The Rust-side
// references we hand out are immutable (`&XcbExtensionT`) and we
// only ever take a `*mut` from them at call sites.
unsafe impl Sync for XcbExtensionT {}
unsafe impl Send for XcbExtensionT {}

/// Opaque libxcb types. We never dereference these — only pass the
/// pointers back through libxcb function calls.
#[repr(C)]
struct XcbConnectionT(c_void);
#[repr(C)]
pub struct XcbSpecialEventT(c_void);

type XcbGetExtensionData =
    unsafe extern "C" fn(*mut XcbConnectionT, *mut XcbExtensionT) -> *mut c_void;

type XcbRegisterForSpecialXge = unsafe extern "C" fn(
    *mut XcbConnectionT,
    *mut XcbExtensionT,
    u32,
    *mut u32,
) -> *mut XcbSpecialEventT;

type XcbPollForSpecialEvent =
    unsafe extern "C" fn(*mut XcbConnectionT, *mut XcbSpecialEventT) -> *mut u8;

type XcbUnregisterForSpecialEvent =
    unsafe extern "C" fn(*mut XcbConnectionT, *mut XcbSpecialEventT);

type LibcFree = unsafe extern "C" fn(*mut c_void);

struct LibxcbSyms {
    // Order matters for drop: keep the library loaded until after
    // the symbol references are gone.
    get_extension_data: libloading::os::unix::Symbol<XcbGetExtensionData>,
    register_for_special_xge: libloading::os::unix::Symbol<XcbRegisterForSpecialXge>,
    poll_for_special_event: libloading::os::unix::Symbol<XcbPollForSpecialEvent>,
    unregister_for_special_event:
        libloading::os::unix::Symbol<XcbUnregisterForSpecialEvent>,
    free: libloading::os::unix::Symbol<LibcFree>,
    _xcb: libloading::os::unix::Library,
    _libc: libloading::os::unix::Library,
}

static LIBXCB: OnceLock<Option<LibxcbSyms>> = OnceLock::new();
static PRESENT_EXT: OnceLock<XcbExtensionT> = OnceLock::new();

fn syms() -> Option<&'static LibxcbSyms> {
    LIBXCB
        .get_or_init(|| {
            // SAFETY: dlopen-style dynamic loading. libxcb is already
            // resident in the process (x11rb loaded it via
            // `dl-libxcb`), so this just bumps its refcount and
            // returns a fresh handle for symbol lookup. We hold the
            // Library forever (`_xcb` field) so the symbol pointers
            // stay valid.
            unsafe {
                let xcb = libloading::os::unix::Library::new("libxcb.so.1")
                    .or_else(|_| libloading::os::unix::Library::new("libxcb.so"))
                    .ok()?;
                let get_extension_data: libloading::os::unix::Symbol<
                    XcbGetExtensionData,
                > = xcb.get(b"xcb_get_extension_data\0").ok()?;
                let register_for_special_xge: libloading::os::unix::Symbol<
                    XcbRegisterForSpecialXge,
                > = xcb.get(b"xcb_register_for_special_xge\0").ok()?;
                let poll_for_special_event: libloading::os::unix::Symbol<
                    XcbPollForSpecialEvent,
                > = xcb.get(b"xcb_poll_for_special_event\0").ok()?;
                let unregister_for_special_event: libloading::os::unix::Symbol<
                    XcbUnregisterForSpecialEvent,
                > = xcb.get(b"xcb_unregister_for_special_event\0").ok()?;
                let libc = libloading::os::unix::Library::new("libc.so.6")
                    .or_else(|_| libloading::os::unix::Library::new("libc.so"))
                    .ok()?;
                let free: libloading::os::unix::Symbol<LibcFree> =
                    libc.get(b"free\0").ok()?;
                Some(LibxcbSyms {
                    get_extension_data,
                    register_for_special_xge,
                    poll_for_special_event,
                    unregister_for_special_event,
                    free,
                    _xcb: xcb,
                    _libc: libc,
                })
            }
        })
        .as_ref()
}

/// Resolve / lazily-initialize the global `xcb_extension_t` for
/// Present. Returns the pointer libxcb expects — `*mut` because the
/// C signature wants mutability for `global_id`, even though the
/// pointee is logically `Sync` (libxcb owns the synchronization).
fn ensure_present_ext(
    syms: &LibxcbSyms,
    conn: *mut c_void,
) -> Option<*mut XcbExtensionT> {
    let ext = PRESENT_EXT.get_or_init(|| XcbExtensionT {
        name: b"Present\0".as_ptr(),
        global_id: std::cell::UnsafeCell::new(0),
    });
    let ext_ptr = ext as *const XcbExtensionT as *mut XcbExtensionT;
    // SAFETY: see `XcbExtensionT` doc comment — libxcb serializes
    // writes to `global_id` internally.
    unsafe {
        let info = (syms.get_extension_data)(conn as *mut XcbConnectionT, ext_ptr);
        if info.is_null() {
            return None;
        }
    }
    Some(ext_ptr)
}

/// Public predicate: is the libxcb special-event API available in
/// this process? `false` means we'll fall back to the periodic
/// `Timer` path in `start_refresh_loop`.
pub fn is_available() -> bool {
    syms().is_some()
}

/// RAII handle for a Present special event queue. Drop calls
/// `xcb_unregister_for_special_event` on the libxcb side, releasing
/// the queue. The held connection pointer is borrowed from x11rb's
/// `XCBConnection`, which outlives every window.
pub struct PresentSubscription {
    conn: *mut c_void,
    se: *mut XcbSpecialEventT,
}

// SAFETY: The contained pointers are libxcb-owned and accessed only
// via libxcb's own (thread-safe) calls.
unsafe impl Send for PresentSubscription {}
unsafe impl Sync for PresentSubscription {}

impl PresentSubscription {
    pub fn raw(&self) -> *mut XcbSpecialEventT {
        self.se
    }
}

impl Drop for PresentSubscription {
    fn drop(&mut self) {
        if let Some(syms) = syms() {
            // SAFETY: `se` was returned by `xcb_register_for_special_xge`
            // and is a valid pointer until this call. After unregister
            // libxcb may free it; we never use `se` again.
            unsafe {
                (syms.unregister_for_special_event)(
                    self.conn as *mut XcbConnectionT,
                    self.se,
                );
            }
        }
    }
}

/// Register a Present special-event queue for `eid` on `conn`.
/// `eid` must already have been the target of `present_select_input`
/// (we expect the caller to have made that x11rb call before this
/// one — pairing them keeps the registration order required by the
/// X server).
///
/// Returns `None` if libxcb couldn't be opened, the Present
/// extension isn't actually present on this server, or the
/// registration call itself failed.
pub fn register(conn: *mut c_void, eid: u32) -> Option<PresentSubscription> {
    let syms = syms()?;
    let ext_ptr = ensure_present_ext(syms, conn)?;
    let mut stamp: u32 = 0;
    // SAFETY: arguments mirror the libxcb prototype; `&mut stamp`
    // gives us a sequence-number stamp libxcb writes back. We don't
    // read it.
    let se = unsafe {
        (syms.register_for_special_xge)(
            conn as *mut XcbConnectionT,
            ext_ptr,
            eid,
            &mut stamp as *mut u32,
        )
    };
    if se.is_null() {
        return None;
    }
    Some(PresentSubscription { conn, se })
}

/// Drain one queued event from `sub`'s special queue. Returns the
/// `window` XID encoded in `PresentCompleteNotify`'s body when the
/// next event is one (and it should always be — that's the only
/// `event_mask` we subscribed to). Returns `None` when the queue is
/// empty for this iteration.
///
/// The libxcb-allocated event buffer is freed via `libc::free`
/// before this function returns (libxcb's contract is that callers
/// own the returned event buffer).
pub fn poll_complete_notify_window(sub: &PresentSubscription) -> Option<u32> {
    let syms = syms()?;
    // SAFETY: arguments mirror the libxcb prototype. The returned
    // pointer, if non-null, points to at least
    // `sizeof(xcb_present_complete_notify_event_t)` (32 bytes) of
    // valid event data and is owned by us until we call `free`.
    let raw =
        unsafe { (syms.poll_for_special_event)(sub.conn as *mut XcbConnectionT, sub.se) };
    if raw.is_null() {
        return None;
    }
    // Parse the wire format. xcb's
    // `xcb_present_complete_notify_event_t` layout:
    //   0: response_type (u8)  — 35 (XGE)
    //   1: extension     (u8)
    //   2: sequence      (u16)
    //   4: length        (u32) — # of u32s after the 32-byte header
    //   8: event_type    (u16) — 1 for COMPLETE_NOTIFY
    //  10: kind          (u8)
    //  11: mode          (u8)
    //  12: event         (u32) — the eid we passed to select_input
    //  16: window        (u32) ←
    //  20: serial        (u32)
    //  24: ust           (u64)
    //  32: msc           (u64)
    // We only need `window`, but read defensively to validate this
    // is the event type we expect.
    // SAFETY: 32-byte read from a valid libxcb event buffer.
    let (event_type, window) = unsafe {
        let bytes = std::slice::from_raw_parts(raw, 32);
        let event_type = u16::from_ne_bytes([bytes[8], bytes[9]]);
        let window = u32::from_ne_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        (event_type, window)
    };
    // SAFETY: libxcb-allocated buffer; freeing through libc's `free`
    // is exactly libxcb's documented ownership transfer.
    unsafe { (syms.free)(raw as *mut c_void) };
    if event_type != 1 {
        // Not a CompleteNotify — drop on the floor. The only way to
        // get here is a future Present event-type we don't subscribe
        // to slipping through; cheap to ignore.
        return None;
    }
    Some(window)
}

//! One-copy GPU screen capture via `wlr-screencopy` with dmabuf —
//! the Hyprland-compatible path.
//!
//! Hyprland dropped `wlr_export_dmabuf` support in v0.42.0; the maintainer
//! recommends `wlr_screencopy` which "also does dmabuf" ([Hyprland #6623](https://github.com/hyprwm/Hyprland/issues/6623)).
//! This module implements that path: the client allocates a dmabuf via GBM,
//! creates a `wl_buffer` from it via `zwp_linux_buffer_params_v1`, and the
//! compositor copies the frame into it. One GPU-to-GPU copy, no CPU
//! readback, no host upload.
//!
//! Flow: bind registry → find `wl_output`, `zwlr_screencopy_manager_v1`,
//! `zwp_linux_dmabuf_v1` → `capture_output` → receive `linux_dmabuf` event
//! (format, width, height) → allocate dmabuf via GBM → create `wl_buffer`
//! via `zwp_linux_buffer_params_v1` → `frame.copy(&buffer)` → wait for
//! `ready` → return dmabuf fd for Vulkan import.

use super::{DmabufFrame, DmabufPlane, DRM_FORMAT_MOD_INVALID};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1,
};

// ---------------------------------------------------------------------------
// GBM FFI — raw dlopen at runtime, avoids libclang build dependency
// (same pattern as the V4L2 backend's hand-written ioctl code).
// ---------------------------------------------------------------------------

mod gbm_ffi {
    #[repr(C)]
    pub struct gbm_device { _opaque: [u8; 0] }
    #[repr(C)]
    pub struct gbm_bo { _opaque: [u8; 0] }

    pub const GBM_BO_USE_RENDERING: u32 = 1 << 2;
    pub const GBM_BO_USE_LINEAR: u32 = 1 << 3;

    pub type CreateDeviceFn = unsafe extern "C" fn(fd: i32) -> *mut gbm_device;
    pub type DestroyDeviceFn = unsafe extern "C" fn(gbm: *mut gbm_device);
    pub type BoCreateFn = unsafe extern "C" fn(
        gbm: *mut gbm_device,
        width: u32,
        height: u32,
        format: u32,
        usage: u32,
    ) -> *mut gbm_bo;
    pub type BoDestroyFn = unsafe extern "C" fn(bo: *mut gbm_bo);
    pub type BoGetFdFn = unsafe extern "C" fn(bo: *mut gbm_bo) -> i32;
    pub type BoGetStrideFn = unsafe extern "C" fn(bo: *mut gbm_bo) -> u32;
    pub type BoGetModifierFn = unsafe extern "C" fn(bo: *mut gbm_bo) -> u64;
}

/// Runtime-loaded GBM device. The DRM render-node fd stays open for the
/// lifetime of this struct (GBM doesn't take ownership of it).
struct GbmCtx {
    _lib: libloading::Library,
    _drm_file: std::fs::File,
    device: *mut gbm_ffi::gbm_device,
    destroy_device: gbm_ffi::DestroyDeviceFn,
    bo_create: gbm_ffi::BoCreateFn,
    bo_destroy: gbm_ffi::BoDestroyFn,
    bo_get_fd: gbm_ffi::BoGetFdFn,
    bo_get_stride: gbm_ffi::BoGetStrideFn,
    bo_get_modifier: gbm_ffi::BoGetModifierFn,
}

unsafe impl Send for GbmCtx {}

struct GbmBoGuard<'a> {
    bo: *mut gbm_ffi::gbm_bo,
    ctx: &'a GbmCtx,
}

impl GbmCtx {
    fn open() -> Result<Self, String> {
        let lib = libloading::Library::new("libgbm.so.1")
            .or_else(|_| libloading::Library::new("libgbm.so"))
            .map_err(|e| format!("failed to load libgbm: {e}"))?;

        let load_sym = |name: &[u8]| -> Result<unsafe extern "C" fn(), String> {
            unsafe {
                *lib.get(name)
                    .map_err(|e| format!("symbol {}: {e}", std::str::from_utf8(name).unwrap_or("?")))?
            }
        };

        let create_device: gbm_ffi::CreateDeviceFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_create_device\0")?) };
        let destroy_device: gbm_ffi::DestroyDeviceFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_device_destroy\0")?) };
        let bo_create: gbm_ffi::BoCreateFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_bo_create\0")?) };
        let bo_destroy: gbm_ffi::BoDestroyFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_bo_destroy\0")?) };
        let bo_get_fd: gbm_ffi::BoGetFdFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_bo_get_fd\0")?) };
        let bo_get_stride: gbm_ffi::BoGetStrideFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_bo_get_stride\0")?) };
        let bo_get_modifier: gbm_ffi::BoGetModifierFn =
            unsafe { std::mem::transmute(load_sym(b"gbm_bo_get_modifier\0")?) };

        let drm_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/dri/renderD128")
            .or_else(|_| {
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open("/dev/dri/renderD129")
            })
            .map_err(|e| format!("open DRM render node: {e}"))?;

        let device = unsafe { create_device(drm_file.as_raw_fd()) };
        if device.is_null() {
            return Err("gbm_create_device returned null".into());
        }

        Ok(Self {
            _lib: lib,
            _drm_file: drm_file,
            device,
            destroy_device,
            bo_create,
            bo_destroy,
            bo_get_fd,
            bo_get_stride,
            bo_get_modifier,
        })
    }

    fn create_bo(&self, width: u32, height: u32, format: u32) -> Result<GbmBoGuard, String> {
        let bo = unsafe {
            (self.bo_create)(
                self.device,
                width,
                height,
                format,
                gbm_ffi::GBM_BO_USE_RENDERING | gbm_ffi::GBM_BO_USE_LINEAR,
            )
        };
        if bo.is_null() {
            return Err("gbm_bo_create returned null".into());
        }
        Ok(GbmBoGuard { bo, ctx: self })
    }
}

impl Drop for GbmCtx {
    fn drop(&mut self) {
        unsafe { (self.destroy_device)(self.device) };
    }
}

impl GbmBoGuard<'_> {
    fn fd(&self) -> Result<OwnedFd, String> {
        let fd = unsafe { (self.ctx.bo_get_fd)(self.bo) };
        if fd < 0 {
            return Err("gbm_bo_get_fd failed".into());
        }
        // SAFETY: gbm_bo_get_fd returns a new fd that we own.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn stride(&self) -> u32 {
        unsafe { (self.ctx.bo_get_stride)(self.bo) }
    }

    fn modifier(&self) -> u64 {
        unsafe { (self.ctx.bo_get_modifier)(self.bo) }
    }
}

impl Drop for GbmBoGuard<'_> {
    fn drop(&mut self) {
        unsafe { (self.ctx.bo_destroy)(self.bo) };
    }
}

// ---------------------------------------------------------------------------
// Wayland state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CaptureState {
    output: Option<wl_output::WlOutput>,
    screencopy_manager: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    linux_dmabuf: Option<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1>,

    // From the `linux_dmabuf` event
    dmabuf_format: u32,
    dmabuf_width: u32,
    dmabuf_height: u32,

    // From the `buffer` event (SHM fallback info — not used but must be handled)
    buffer_done: bool,
    y_invert: bool,
    ready: bool,
    failed: bool,

    // From `zwp_linux_buffer_params_v1::failed`
    buffer_create_failed: bool,
}

impl CaptureState {
    fn reset_capture(&mut self) {
        self.dmabuf_format = 0;
        self.dmabuf_width = 0;
        self.dmabuf_height = 0;
        self.buffer_done = false;
        self.y_invert = false;
        self.ready = false;
        self.failed = false;
        self.buffer_create_failed = false;
    }
}

// --- Wayland Dispatch implementations ---

impl Dispatch<wl_registry::WlRegistry, ()> for CaptureState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_output" if state.output.is_none() => {
                    state.output = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "zwlr_screencopy_manager_v1" if state.screencopy_manager.is_none() => {
                    state.screencopy_manager =
                        Some(registry.bind(name, version.min(3), qh, ()));
                }
                "zwp_linux_dmabuf_v1" if state.linux_dmabuf.is_none() => {
                    state.linux_dmabuf = Some(registry.bind(name, version.min(3), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::LinuxDmabuf { format, width, height } => {
                state.dmabuf_format = format;
                state.dmabuf_width = width;
                state.dmabuf_height = height;
            }
            zwlr_screencopy_frame_v1::Event::Buffer { .. } => {
                // SHM buffer info — we only use the dmabuf path.
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                if let WEnum::Value(f) = flags {
                    state.y_invert = f.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
                }
            }
            zwlr_screencopy_frame_v1::Event::BufferDone => {
                state.buffer_done = true;
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                state.ready = true;
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                state.failed = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_buffer_params_v1::Event::Failed => {
                state.buffer_create_failed = true;
                params.destroy();
            }
            _ => {}
        }
    }
}

delegate_noop!(CaptureState: ignore wl_output::WlOutput);
delegate_noop!(CaptureState: ignore wl_buffer::WlBuffer);
delegate_noop!(CaptureState: ignore zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);
delegate_noop!(CaptureState: ignore zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1);

// ---------------------------------------------------------------------------
// DmabufScreenSource
// ---------------------------------------------------------------------------

/// Screen-capture source using `wlr-screencopy` with dmabuf.
///
/// Allocates a dmabuf via GBM for each frame, creates a `wl_buffer` from
/// it via `zwp_linux_buffer_params_v1`, and has the compositor copy the
/// frame into it. The dmabuf fd is then returned for Vulkan import.
///
/// This is the Hyprland-compatible replacement for `wlr_export_dmabuf`,
/// which Hyprland dropped in v0.42.0.
pub struct DmabufScreenSource {
    conn: Connection,
    event_queue: EventQueue<CaptureState>,
    state: CaptureState,
    gbm: GbmCtx,
}

impl DmabufScreenSource {
    /// Connects to the compositor and initializes GBM. Fails if the
    /// compositor doesn't support `wlr-screencopy` with dmabuf, or if
    /// GBM/libgbm is unavailable.
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::connect_to_env()
            .map_err(|e| format!("couldn't connect to a Wayland compositor: {e}"))?;
        let mut event_queue = conn.new_event_queue::<CaptureState>();
        let qh = event_queue.handle();
        let mut state = CaptureState::default();

        let _registry = conn.display().get_registry(&qh, ());
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| format!("initial roundtrip failed: {e}"))?;

        if state.output.is_none() {
            return Err("compositor advertised no wl_output".to_string());
        }
        if state.screencopy_manager.is_none() {
            return Err(
                "compositor doesn't implement wlr-screencopy-unstable-v1"
                    .to_string(),
            );
        }
        if state.linux_dmabuf.is_none() {
            return Err(
                "compositor doesn't implement zwp_linux_dmabuf_v1 (dmabuf screencopy not supported)"
                    .to_string(),
            );
        }

        let gbm = GbmCtx::open()?;

        Ok(Self { conn, event_queue, state, gbm })
    }

    /// Captures one frame. Returns `Ok(Some(frame))` on success,
    /// `Ok(None)` if the capture was cancelled (temporary), or `Err`
    /// on a protocol/transport error.
    pub fn next_frame(&mut self) -> Result<Option<DmabufFrame>, String> {
        let qh = self.event_queue.handle();
        let output = self
            .state
            .output
            .as_ref()
            .ok_or("no wl_output bound")?
            .clone();
        let manager = self
            .state
            .screencopy_manager
            .as_ref()
            .ok_or("no screencopy manager")?
            .clone();
        let linux_dmabuf = self
            .state
            .linux_dmabuf
            .as_ref()
            .ok_or("no zwp_linux_dmabuf_v1")?
            .clone();

        self.state.reset_capture();

        let frame = manager.capture_output(0, &output, &qh, ());

        // Wait for the linux_dmabuf event (and buffer_done).
        for i in 0..32 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("screencopy buffer roundtrip: {e}"))?;
            if self.state.failed {
                eprintln!("[screen_dmabuf] capture failed before buffer_done");
                return Ok(None);
            }
            if self.state.buffer_done || (self.state.dmabuf_width > 0 && i >= 8) {
                break;
            }
        }

        if self.state.dmabuf_width == 0 {
            return Err(
                "compositor didn't send linux_dmabuf event (dmabuf screencopy not supported for this output)"
                    .to_string(),
            );
        }

        let width = self.state.dmabuf_width;
        let height = self.state.dmabuf_height;
        let format = self.state.dmabuf_format;

        // Allocate a dmabuf via GBM.
        let bo = self.gbm.create_bo(width, height, format)?;
        let stride = bo.stride();
        let raw_modifier = bo.modifier();

        // Get a fd for the wl_buffer.
        let wl_fd = bo.fd()?;

        // Create a wl_buffer from the dmabuf fd.
        let params = linux_dmabuf.create_params(&qh, ());
        let (mod_hi, mod_lo) = if raw_modifier == DRM_FORMAT_MOD_INVALID {
            (0u32, 0u32)
        } else {
            ((raw_modifier >> 32) as u32, (raw_modifier & 0xFFFFFFFF) as u32)
        };
        params.add(wl_fd.as_fd(), 0, 0, stride, mod_hi, mod_lo);
        let buffer = params.create_immed(
            width as i32,
            height as i32,
            format,
            0, // flags
            &qh,
            (),
        );

        // Check for buffer creation failure.
        for _ in 0..4 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("buffer create roundtrip: {e}"))?;
            if self.state.buffer_create_failed {
                eprintln!("[screen_dmabuf] zwp_linux_buffer_params_v1::failed — dmabuf import rejected by compositor");
                return Ok(None);
            }
            break;
        }

        // Copy the frame into our dmabuf-backed buffer.
        frame.copy(&buffer);

        for _ in 0..32 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("screencopy ready roundtrip: {e}"))?;
            if self.state.ready || self.state.failed {
                break;
            }
        }

        // Clean up Wayland objects.
        buffer.destroy();
        params.destroy();
        frame.destroy();

        if self.state.failed {
            return Ok(None);
        }
        if !self.state.ready {
            return Err("screencopy frame never became ready".to_string());
        }

        // Get a fresh fd for Vulkan import — the compositor has written
        // the frame into the dmabuf. gbm_bo_get_fd returns a new fd each
        // call; the fd remains valid after the BO is destroyed because
        // it holds its own reference to the underlying buffer.
        let import_fd = bo.fd()?;
        drop(bo); // BO is destroyed, but import_fd is still valid.

        // Use modifier 0 (linear) when GBM returned INVALID, so Vulkan
        // import uses linear tiling with the explicit stride.
        let modifier = if raw_modifier == DRM_FORMAT_MOD_INVALID {
            0
        } else {
            raw_modifier
        };

        let dmabuf_frame = DmabufFrame {
            width,
            height,
            drm_format: format,
            modifier,
            planes: vec![DmabufPlane {
                fd: import_fd,
                stride,
                offset: 0,
                plane_index: 0,
            }],
        };

        println!(
            "[screen_dmabuf] captured {}x{} format=0x{:08X} modifier=0x{:016X} stride={}",
            dmabuf_frame.width, dmabuf_frame.height, dmabuf_frame.drm_format,
            dmabuf_frame.modifier, stride
        );

        Ok(Some(dmabuf_frame))
    }
}

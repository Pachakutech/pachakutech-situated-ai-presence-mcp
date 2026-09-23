#![allow(dead_code)] // real code, compile-verified only — not yet called from main.rs (no ingress-tick loop exists to wire it into). See module doc comment for the honest real-vs-tested boundary.
//! Screen capture via the `wlr-screencopy-unstable-v1` Wayland protocol —

//! the Omarchy-relevant path (Hyprland, and wlroots compositors generally,
//! implement this). Real protocol code, compiled against
//! `wayland-client`/`wayland-protocols-wlr` in this sandbox — but this
//! sandbox has no compositor to connect to, so `WlrScreenSource::connect`
//! has only ever been exercised as far as "does it type-check and link",
//! never "does it actually get a frame". That needs a real Omarchy
//! session.
//!
//! Flow: bind the registry -> find one `wl_output` and the
//! `zwlr_screencopy_manager_v1` global -> `capture_output` -> wait for the
//! `buffer` event (which tells us the format/size the compositor wants to
//! write into) -> allocate a shared-memory buffer of that size -> `copy`
//! into it -> wait for `ready` -> read the pixels back out of the shm
//! region as an `IngressFrame`.

use super::{FrameSource, IngressFrame};
use std::os::fd::AsFd;
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

/// What the compositor told us via the frame's `buffer` event: the exact
/// shm format/geometry it wants us to allocate and copy into.
#[derive(Debug, Clone, Copy)]
struct BufferSpec {
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
}

#[derive(Default)]
struct CaptureState {
    output: Option<wl_output::WlOutput>,
    shm: Option<wl_shm::WlShm>,
    screencopy_manager: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    buffer_spec: Option<BufferSpec>,
    buffer_done: bool,
    y_invert: bool,
    ready: bool,
    failed: bool,
}

// Handled entirely by hand (rather than via `wayland_client::globals`'
// `registry_queue_init`/`GlobalList` helper) because `wl_output` is a
// multi-instance global that helper deliberately doesn't bind for you —
// see its doc comment. Once one non-multi-instance global (`wl_shm`) has
// to be picked up through this same registry Dispatch anyway, it's
// simpler to do all three globals the same way here than to mix both
// binding styles.
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
                "wl_shm" if state.shm.is_none() => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "zwlr_screencopy_manager_v1" => {
                    state.screencopy_manager =
                        Some(registry.bind(name, version.min(3), qh, ()));
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
            zwlr_screencopy_frame_v1::Event::Buffer { format, width, height, stride } => {
                let format = match format {
                    WEnum::Value(f) => f,
                    WEnum::Unknown(_) => wl_shm::Format::Argb8888,
                };
                state.buffer_spec = Some(BufferSpec { width, height, stride, format });
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

delegate_noop!(CaptureState: ignore wl_output::WlOutput);
delegate_noop!(CaptureState: ignore wl_shm::WlShm);
delegate_noop!(CaptureState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(CaptureState: ignore wl_buffer::WlBuffer);
delegate_noop!(CaptureState: ignore zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1);

/// A screen-capture source bound to one Wayland output. Reconnects nothing
/// automatically today — a dropped compositor connection surfaces as an
/// `Err` from `next_frame`, and the caller (the Presence daemon's ingress
/// loop) decides whether/when to retry `connect()`.
pub struct WlrScreenSource {
    conn: Connection,
    event_queue: EventQueue<CaptureState>,
    state: CaptureState,
}

impl WlrScreenSource {
    /// Connects to the compositor on `$WAYLAND_DISPLAY` and binds the
    /// globals this needs. Fails clearly (not a panic) if there's no
    /// compositor, no `wl_output`, or the compositor doesn't implement
    /// `wlr-screencopy` (e.g. a non-wlroots compositor) — exactly the
    /// failure mode this can't help but hit in this sandbox.
    pub fn connect() -> Result<Self, String> {
        let conn = Connection::connect_to_env()
            .map_err(|e| format!("couldn't connect to a Wayland compositor: {e}"))?;
        let mut event_queue = conn.new_event_queue::<CaptureState>();
        let qh = event_queue.handle();
        let mut state = CaptureState::default();

        // All three globals this needs (wl_output, wl_shm,
        // zwlr_screencopy_manager_v1) are picked up through one registry
        // Dispatch impl above — see its doc comment for why wl_output in
        // particular rules out using the `wayland_client::globals` helper.
        let _registry = conn.display().get_registry(&qh, ());
        event_queue
            .roundtrip(&mut state)
            .map_err(|e| format!("initial roundtrip failed: {e}"))?;

        if state.output.is_none() {
            return Err("compositor advertised no wl_output".to_string());
        }
        if state.shm.is_none() {
            return Err("compositor has no wl_shm".to_string());
        }
        if state.screencopy_manager.is_none() {
            return Err(
                "compositor doesn't implement wlr-screencopy-unstable-v1 (not a wlroots compositor?)"
                    .to_string(),
            );
        }

        Ok(Self { conn, event_queue, state })
    }
}

impl FrameSource for WlrScreenSource {
    fn next_frame(&mut self) -> Result<Option<IngressFrame>, String> {
        let qh = self.event_queue.handle();
        let output = self.state.output.as_ref().unwrap().clone();
        let manager = self.state.screencopy_manager.as_ref().unwrap().clone();

        self.state.buffer_spec = None;
        self.state.buffer_done = false;
        self.state.y_invert = false;
        self.state.ready = false;
        self.state.failed = false;

        let frame = manager.capture_output(0, &output, &qh, ());

        // Wait until the compositor has advertised a shm buffer (and, on
        // protocol v3+, `buffer_done`). A single roundtrip is not always
        // enough on Hyprland.
        for i in 0..32 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("roundtrip while awaiting buffer spec failed: {e}"))?;
            if self.state.failed {
                return Ok(None);
            }
            // v3+ sends `buffer_done` after the last `buffer`/`linux_dmabuf`.
            // Older compositors never send it — fall through after a few trips.
            if self.state.buffer_done || (self.state.buffer_spec.is_some() && i >= 8) {
                break;
            }
        }

        let spec = match self.state.buffer_spec {
            Some(s) => s,
            None => return Err("compositor never sent a 'buffer' event".to_string()),
        };

        let size = (spec.stride as usize) * (spec.height as usize);
        let shm_fd = create_anonymous_shm(size)?;
        let pool = self.state.shm.as_ref().unwrap().create_pool(shm_fd.as_fd(), size as i32, &qh, ());
        let buffer = pool.create_buffer(
            0,
            spec.width as i32,
            spec.height as i32,
            spec.stride as i32,
            spec.format,
            &qh,
            (),
        );

        frame.copy(&buffer);

        for _ in 0..32 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("roundtrip while awaiting ready/failed failed: {e}"))?;
            if self.state.ready || self.state.failed {
                break;
            }
        }

        if self.state.failed {
            return Ok(None);
        }
        if !self.state.ready {
            return Err("frame neither ready nor failed after copy".to_string());
        }

        // Read the shm-backed pixels back out. `Argb8888` on the wire is
        // little-endian, so byte order in memory is B, G, R, A.
        let mmap = map_shm_readonly(&shm_fd, size)?;
        let mut rgba = vec![0u8; (spec.width * spec.height * 4) as usize];
        let y_invert = self.state.y_invert;
        for y in 0..spec.height as usize {
            let src_y = if y_invert { spec.height as usize - 1 - y } else { y };
            let row_start = src_y * spec.stride as usize;
            for x in 0..spec.width as usize {
                let src = row_start + x * 4;
                let dst = (y * spec.width as usize + x) * 4;
                // Wayland shm 8888 is little-endian; byte 0 is B.
                rgba[dst] = mmap[src + 2];
                rgba[dst + 1] = mmap[src + 1];
                rgba[dst + 2] = mmap[src];
                rgba[dst + 3] = match spec.format {
                    wl_shm::Format::Argb8888 => mmap[src + 3],
                    _ => 255,
                };
            }
        }

        Ok(Some(IngressFrame { width: spec.width, height: spec.height, rgba }))
    }
}

/// Wayland shm buffers are backed by a plain memory-mapped file descriptor
/// the client owns — `memfd_create` is the standard way to get one
/// without touching the real filesystem.
pub(crate) fn create_anonymous_shm(size: usize) -> Result<std::os::fd::OwnedFd, String> {
    use std::os::fd::FromRawFd;
    let name = c"pachakutech-presence-screencopy";
    let fd = unsafe { libc::memfd_create(name.as_ptr(), 0) };
    if fd < 0 {
        return Err(format!("memfd_create failed: {}", std::io::Error::last_os_error()));
    }
    if unsafe { libc::ftruncate(fd, size as libc::off_t) } != 0 {
        return Err(format!("ftruncate failed: {}", std::io::Error::last_os_error()));
    }
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
}

pub(crate) fn map_shm_readonly(fd: &std::os::fd::OwnedFd, size: usize) -> Result<memmap_shim::Mmap, String> {
    memmap_shim::Mmap::map(fd, size)
}

/// A tiny hand-rolled read-only mmap wrapper — avoids pulling in the
/// `memmap2` crate for one call site.
mod memmap_shim {
    use std::ops::Deref;
    use std::os::fd::{AsRawFd, OwnedFd};

    pub struct Mmap {
        ptr: *mut libc::c_void,
        len: usize,
    }

    impl Mmap {
        pub fn map(fd: &OwnedFd, len: usize) -> Result<Self, String> {
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    len,
                    libc::PROT_READ,
                    libc::MAP_SHARED,
                    fd.as_raw_fd(),
                    0,
                )
            };
            if ptr == libc::MAP_FAILED {
                return Err(format!("mmap failed: {}", std::io::Error::last_os_error()));
            }
            Ok(Self { ptr, len })
        }
    }

    impl Deref for Mmap {
        type Target = [u8];
        fn deref(&self) -> &[u8] {
            unsafe { std::slice::from_raw_parts(self.ptr as *const u8, self.len) }
        }
    }

    impl Drop for Mmap {
        fn drop(&mut self) {
            unsafe {
                libc::munmap(self.ptr, self.len);
            }
        }
    }
}

#![allow(dead_code)] // real code, compile-verified only — not yet called from main.rs (no ingress-tick loop exists to wire it into). See module doc comment for the honest real-vs-tested boundary.
//! Zero-copy screen capture via `wlr-export-dmabuf-unstable-v1` — the
//! compositor exports a DMA-BUF directly, which Vulkan imports with
//! `VK_KHR_external_memory_fd` + `VK_EXT_external_memory_dma_buf` +
//! `VK_EXT_image_drm_format_modifier`. No CPU-visible `wl_shm` buffer, no
//! `vkMapMemory`, no host readback/upload, and no CPU-side pixel copies.
//!
//! This is the preferred capture path. `screen_wlr` (SHM-based screencopy)
//! remains as a fallback for compositors/drivers that don't support
//! export-dmabuf or can't import the exported modifier.
//!
//! Flow: bind the registry → find one `wl_output` and the
//! `zwlr_export_dmabuf_manager_v1` global → `capture_output` → wait for
//! `frame` (metadata) → collect `object` events (plane FDs + strides) →
//! wait for `ready` → return a `DmabufFrame` for Vulkan import.
//!
//! **Initial constraints** (per the correct_screen_ingress design PDF):
//! - Accept only one exported object / one fd (single-plane).
//! - Accept only single-plane XRGB/ARGB/XBGR/ABGR formats.
//! - Log the DRM fourcc, modifier, strides, offsets, object count, and
//!   any Vulkan failure code so unsupported formats can be diagnosed.
//! Multi-plane YUV / multi-object support should be added deliberately
//! rather than treated as an edge case.

use super::{DmabufFrame, DmabufPlane};
use std::os::fd::OwnedFd;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle, WEnum};
use wayland_protocols_wlr::export_dmabuf::v1::client::{
    zwlr_export_dmabuf_frame_v1, zwlr_export_dmabuf_manager_v1,
};

/// The number of dmabuf objects (FDs) the compositor advertised for this
/// frame. The protocol sends one `object` event per exported memory
/// object; for common single-plane RGB exports this is always 1.
type PlaneCount = u32;

#[derive(Default)]
struct CaptureState {
    output: Option<wl_output::WlOutput>,
    export_dmabuf_manager: Option<zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1>,

    // Frame metadata (from the `frame` event)
    width: u32,
    height: u32,
    drm_format: u32,
    modifier: u64,
    flags: u32,
    expected_objects: PlaneCount,

    // Plane data (from `object` events)
    planes: Vec<Option<(OwnedFd, u32, u32, u32)>>, // (fd, stride, offset, plane_index)

    // State machine
    frame_received: bool,
    ready: bool,
    failed: bool,
    cancel_reason: Option<u32>,
}

impl CaptureState {
    fn reset_capture(&mut self) {
        self.width = 0;
        self.height = 0;
        self.drm_format = 0;
        self.modifier = 0;
        self.flags = 0;
        self.expected_objects = 0;
        self.planes.clear();
        self.frame_received = false;
        self.ready = false;
        self.failed = false;
        self.cancel_reason = None;
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
                "zwlr_export_dmabuf_manager_v1" if state.export_dmabuf_manager.is_none() => {
                    state.export_dmabuf_manager =
                        Some(registry.bind(name, version.min(1), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_export_dmabuf_frame_v1::ZwlrExportDmabufFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        frame: &zwlr_export_dmabuf_frame_v1::ZwlrExportDmabufFrameV1,
        event: zwlr_export_dmabuf_frame_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_export_dmabuf_frame_v1::Event::Frame {
                width,
                height,
                offset_x: _,
                offset_y: _,
                buffer_flags: _,
                flags,
                format,
                mod_high,
                mod_low,
                num_objects,
            } => {
                state.width = width;
                state.height = height;
                state.drm_format = format;
                state.modifier = (u64::from(mod_high) << 32) | u64::from(mod_low);
                state.flags = match flags {
                    WEnum::Value(f) => f as u32,
                    WEnum::Unknown(v) => v,
                };
                state.expected_objects = num_objects;
                state.planes.clear();
                state.planes.reserve(num_objects as usize);
                for _ in 0..num_objects {
                    state.planes.push(None);
                }
                state.frame_received = true;
            }

            zwlr_export_dmabuf_frame_v1::Event::Object {
                index,
                fd,
                size: _,
                offset,
                stride,
                plane_index,
            } => {
                // `fd` is already an `OwnedFd` from the wayland-client crate.
                if (index as usize) < state.planes.len() {
                    state.planes[index as usize] = Some((fd, stride, offset, plane_index));
                } else {
                    // Object index out of range — fd will be closed on drop.
                    eprintln!(
                        "[screen_dmabuf] object index {} out of range (expected {})",
                        index, state.expected_objects
                    );
                    drop(fd);
                    state.failed = true;
                }
            }

            zwlr_export_dmabuf_frame_v1::Event::Ready { .. } => {
                state.ready = true;
                // Destroy the frame proxy — the compositor has finished
                // producing the exported buffer and the fd(s) are now ours.
                frame.destroy();
            }

            zwlr_export_dmabuf_frame_v1::Event::Cancel { reason } => {
                state.failed = true;
                state.cancel_reason = Some(match reason {
                    WEnum::Value(r) => r as u32,
                    WEnum::Unknown(v) => v,
                });
                frame.destroy();
            }

            _ => {}
        }
    }
}

delegate_noop!(CaptureState: ignore wl_output::WlOutput);
delegate_noop!(CaptureState: ignore zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1);

/// A screen-capture source bound to one Wayland output, using the
/// `zwlr_export_dmabuf_manager_v1` protocol for zero-copy DMA-BUF capture.
///
/// Reconnects nothing automatically today — a dropped compositor
/// connection surfaces as an `Err` from `next_frame`, and the caller
/// decides whether/when to retry `connect()`.
pub struct DmabufScreenSource {
    conn: Connection,
    event_queue: EventQueue<CaptureState>,
    state: CaptureState,
}

impl DmabufScreenSource {
    /// Connects to the compositor on `$WAYLAND_DISPLAY` and binds the
    /// globals this needs. Fails clearly (not a panic) if there's no
    /// compositor, no `wl_output`, or the compositor doesn't implement
    /// `wlr-export-dmabuf-unstable-v1` (e.g. a non-wlroots compositor, or
    /// one that only supports screencopy).
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
        if state.export_dmabuf_manager.is_none() {
            return Err(
                "compositor doesn't implement wlr-export-dmabuf-unstable-v1 \
                 (not a wlroots/Hyprland compositor, or dmabuf export disabled?)"
                    .to_string(),
            );
        }

        Ok(Self { conn, event_queue, state })
    }

    /// Requests one dmabuf frame capture from the compositor. Returns
    /// `Ok(Some(frame))` on success, `Ok(None)` if the capture was
    /// cancelled (temporary failure — safe to retry), or `Err` on a
    /// protocol/transport error.
    ///
    /// The returned `DmabufFrame` owns the exported file descriptor(s).
    /// When it is passed to `VulkanContext::import_dmabuf`, ownership of
    /// the fd transfers to the Vulkan driver and must not be closed.
    pub fn next_frame(&mut self) -> Result<Option<DmabufFrame>, String> {
        let qh = self.event_queue.handle();
        let output = self.state.output.as_ref().ok_or("no wl_output bound")?.clone();
        let manager = self
            .state
            .export_dmabuf_manager
            .as_ref()
            .ok_or("no zwlr_export_dmabuf_manager_v1 bound")?
            .clone();

        self.state.reset_capture();

        // overlay_cursor = 0: texture-only desktop capture, no cursor composited.
        // If the hyperbubble needs the cursor embedded, pass 1 here instead.
        let _frame = manager.capture_output(0, &output, &qh, ());

        // Wait for the full sequence: frame → objects → ready (or cancel).
        for _ in 0..64 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| format!("export-dmabuf roundtrip failed: {e}"))?;

            if self.state.failed {
                let reason = self.state.cancel_reason.unwrap_or(0xFFFF);
                eprintln!(
                    "[screen_dmabuf] capture cancelled (reason={}) — format=0x{:08X} {}x{}",
                    reason, self.state.drm_format, self.state.width, self.state.height
                );
                return Ok(None);
            }
            if self.state.ready {
                break;
            }
        }

        if !self.state.ready {
            return Err("export-dmabuf frame never became ready".to_string());
        }
        if self.state.planes.is_empty() || self.state.planes[0].is_none() {
            return Err("export-dmabuf frame had no objects".to_string());
        }

        // Initial constraint: accept only one exported object / one fd.
        if self.state.planes.len() != 1 {
            eprintln!(
                "[screen_dmabuf] multi-object export ({} objects) — rejecting. \
                 Multi-object/multi-plane support is not yet implemented.",
                self.state.planes.len()
            );
            return Ok(None);
        }

        let (fd, stride, offset, plane_index) =
            self.state.planes[0].take().expect("checked non-empty above");

        let frame = DmabufFrame {
            width: self.state.width,
            height: self.state.height,
            drm_format: self.state.drm_format,
            modifier: self.state.modifier,
            planes: vec![DmabufPlane {
                fd,
                stride,
                offset,
                plane_index,
            }],
        };

        println!(
            "[screen_dmabuf] captured {}x{} format=0x{:08X} modifier=0x{:016X} stride={} offset={}",
            frame.width, frame.height, frame.drm_format, frame.modifier, stride, offset
        );

        Ok(Some(frame))
    }
}

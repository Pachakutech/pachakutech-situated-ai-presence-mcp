//! Webcam frames as dma-bufs. The camera writes YUYV into a V4L2 buffer,
//! `VIDIOC_EXPBUF` hands us the fd, and Vulkan imports it. This module
//! never maps that buffer and never reads a pixel.
//!
//! The integrated camera on this machine captures MJPEG or YUYV, not RGB.
//! YUYV is the uncompressed one, so it can enter the GPU the way the
//! desktop dmabuf does. `webcam_v4l2.rs` is the older RGB24 mmap path and
//! is not used for avatar colors.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;
const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
/// `enum v4l2_priority`: background loses to any other opener.
const V4L2_PRIORITY_BACKGROUND: u32 = 1;
/// `v4l2_fourcc('Y','U','Y','V')`.
const V4L2_PIX_FMT_YUYV: u32 = (b'Y' as u32) | ((b'U' as u32) << 8) | ((b'Y' as u32) << 16) | ((b'V' as u32) << 24);

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;
const V_TYPE: u32 = b'V' as u32;

const fn ioc(dir: u32, ty: u32, nr: u32, size: u32) -> u32 {
    (dir << IOC_DIRSHIFT) | (ty << IOC_TYPESHIFT) | (nr << IOC_NRSHIFT) | (size << IOC_SIZESHIFT)
}

fn videoc_querycap() -> u32 { ioc(IOC_READ, V_TYPE, 0, std::mem::size_of::<V4l2Capability>() as u32) }
fn videoc_s_fmt() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 5, std::mem::size_of::<V4l2Format>() as u32) }
fn videoc_reqbufs() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 8, std::mem::size_of::<V4l2RequestBuffers>() as u32) }
fn videoc_querybuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 9, std::mem::size_of::<V4l2Buffer>() as u32) }
fn videoc_qbuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 15, std::mem::size_of::<V4l2Buffer>() as u32) }
fn videoc_expbuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 16, std::mem::size_of::<V4l2ExportBuffer>() as u32) }
fn videoc_dqbuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 17, std::mem::size_of::<V4l2Buffer>() as u32) }
fn videoc_streamon() -> u32 { ioc(IOC_WRITE, V_TYPE, 18, std::mem::size_of::<i32>() as u32) }
fn videoc_streamoff() -> u32 { ioc(IOC_WRITE, V_TYPE, 19, std::mem::size_of::<i32>() as u32) }
/// `VIDIOC_S_PRIORITY` is `_IOW('V', 68, __u32)` on this kernel, request 68.
fn videoc_s_priority() -> u32 { ioc(IOC_WRITE, V_TYPE, 68, 4) }

#[repr(C)]
struct V4l2Capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct V4l2PixFormat {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    ycbcr_or_hsv_enc: u32,
    quantization: u32,
    xfer_func: u32,
}

#[repr(C, align(8))]
union V4l2FormatUnion {
    pix: V4l2PixFormat,
    raw: [u8; 200],
}

#[repr(C)]
struct V4l2Format {
    type_: u32,
    fmt: V4l2FormatUnion,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct V4l2RequestBuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    reserved: [u32; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct V4l2Timecode {
    type_: u32,
    flags: u32,
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    userbits: [u8; 4],
}

#[repr(C)]
union V4l2BufferM {
    offset: u32,
    userptr: u64,
    planes: *mut std::ffi::c_void,
    fd: i32,
}

#[repr(C)]
union V4l2BufferTail {
    request_fd: i32,
    reserved: u32,
}

#[repr(C)]
struct V4l2Buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp: libc::timeval,
    timecode: V4l2Timecode,
    sequence: u32,
    memory: u32,
    m: V4l2BufferM,
    length: u32,
    reserved2: u32,
    tail: V4l2BufferTail,
}

impl Default for V4l2Buffer {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
struct V4l2ExportBuffer {
    type_: u32,
    index: u32,
    plane: u32,
    flags: u32,
    fd: i32,
    reserved: [u32; 11],
}

impl Default for V4l2ExportBuffer {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

const _: () = assert!(std::mem::size_of::<V4l2PixFormat>() == 48);
const _: () = assert!(std::mem::size_of::<V4l2Capability>() == 104);
const _: () = assert!(std::mem::size_of::<V4l2RequestBuffers>() == 20);
const _: () = assert!(std::mem::size_of::<V4l2Format>() == 208);
const _: () = assert!(std::mem::size_of::<V4l2Buffer>() == 88);
const _: () = assert!(std::mem::size_of::<V4l2ExportBuffer>() == 64);

unsafe fn ioctl_raw<T>(fd: i32, request: u32, arg: *mut T) -> i32 {
    unsafe { libc::ioctl(fd, request as libc::c_ulong, arg) }
}

fn ioctl_checked<T>(fd: i32, request: u32, arg: *mut T, what: &str) -> Result<(), String> {
    ioctl_io(fd, request, arg).map_err(|e| format!("{what} failed: {e}"))
}

fn ioctl_io<T>(fd: i32, request: u32, arg: *mut T) -> Result<(), io::Error> {
    if unsafe { ioctl_raw(fd, request, arg) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn is_busy(err: &io::Error) -> bool {
    err.raw_os_error() == Some(libc::EBUSY)
}

/// Sent on the requeue channel once the imported dma-bufs are destroyed.
/// Buffer indexes are `0..count`, so this cannot be a real buffer.
pub const WEBCAM_YIELD_ACK: u32 = u32::MAX;

/// One-time description of the exported buffers, then one index per frame
/// the camera has finished writing. `Yielded` means this thread is about
/// to close the device and is waiting for `WEBCAM_YIELD_ACK`.
pub enum WebcamMsg {
    Started {
        width: u32,
        height: u32,
        stride: u32,
        fds: Vec<OwnedFd>,
    },
    Frame(u32),
    Yielded,
}

pub fn device_path() -> String {
    match std::env::var("PRESENCE_WEBCAM") {
        Ok(path) if !path.is_empty() => path,
        _ => "/dev/video0".to_string(),
    }
}

struct Camera {
    file: File,
    width: u32,
    height: u32,
    stride: u32,
    count: u32,
    fds: Vec<OwnedFd>,
    streaming: bool,
}

impl Camera {
    fn open(path: &str) -> Result<Self, String> {
        let file = File::options()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| format!("couldn't open {path}: {e}"))?;
        let fd = file.as_raw_fd();

        let mut cap = unsafe { std::mem::zeroed::<V4l2Capability>() };
        ioctl_checked(fd, videoc_querycap(), &mut cap, "VIDIOC_QUERYCAP")?;
        if cap.capabilities & V4L2_CAP_VIDEO_CAPTURE == 0 {
            return Err(format!("{path} does not capture video"));
        }

        // Before any buffer is allocated, so a later opener at the default
        // interactive priority can take the device. A driver that rejects
        // the call still streams; we just cannot step aside by priority.
        let mut priority_label = "background";
        let mut prio = V4L2_PRIORITY_BACKGROUND;
        if let Err(e) = ioctl_io(fd, videoc_s_priority(), &mut prio) {
            eprintln!("[webcam] VIDIOC_S_PRIORITY background failed: {e} — streaming anyway");
            priority_label = "unchanged";
        }

        let mut fmt = V4l2Format {
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            fmt: V4l2FormatUnion {
                pix: V4l2PixFormat {
                    width: 640,
                    height: 480,
                    pixelformat: V4L2_PIX_FMT_YUYV,
                    field: V4L2_FIELD_NONE,
                    ..Default::default()
                },
            },
        };
        ioctl_checked(fd, videoc_s_fmt(), &mut fmt, "VIDIOC_S_FMT")?;
        let negotiated = unsafe { fmt.fmt.pix };
        if negotiated.pixelformat != V4L2_PIX_FMT_YUYV {
            return Err(format!(
                "{path} did not accept YUYV (got fourcc {:#x})",
                negotiated.pixelformat
            ));
        }
        if negotiated.width == 0 || negotiated.height == 0 || negotiated.bytesperline < negotiated.width * 2 {
            return Err(format!(
                "{path} negotiated an unusable YUYV layout {}x{} stride {}",
                negotiated.width, negotiated.height, negotiated.bytesperline
            ));
        }

        let mut req = V4l2RequestBuffers {
            count: 3,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        ioctl_checked(fd, videoc_reqbufs(), &mut req, "VIDIOC_REQBUFS")?;
        if req.count < 2 {
            return Err(format!("{path} granted {} capture buffers", req.count));
        }

        let mut fds = Vec::with_capacity(req.count as usize);
        for index in 0..req.count {
            let mut query = V4l2Buffer {
                index,
                type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
                memory: V4L2_MEMORY_MMAP,
                ..Default::default()
            };
            ioctl_checked(fd, videoc_querybuf(), &mut query, "VIDIOC_QUERYBUF")?;
            let mut exp = V4l2ExportBuffer {
                type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
                index,
                plane: 0,
                flags: (libc::O_CLOEXEC | libc::O_RDWR) as u32,
                fd: -1,
                ..Default::default()
            };
            ioctl_checked(fd, videoc_expbuf(), &mut exp, "VIDIOC_EXPBUF")?;
            if exp.fd < 0 {
                return Err(format!("{path} EXPBUF returned no fd for buffer {index}"));
            }
            fds.push(unsafe { OwnedFd::from_raw_fd(exp.fd) });
        }

        println!(
            "[webcam] {} YUYV {}x{} stride {} — {} dma-bufs, priority {priority_label}, no CPU read (colorspace {} enc {} quant {})",
            path,
            negotiated.width,
            negotiated.height,
            negotiated.bytesperline,
            fds.len(),
            negotiated.colorspace,
            negotiated.ycbcr_or_hsv_enc,
            negotiated.quantization
        );
        Ok(Self {
            file,
            width: negotiated.width,
            height: negotiated.height,
            stride: negotiated.bytesperline,
            count: req.count,
            fds,
            streaming: false,
        })
    }

    fn qbuf(&self, index: u32) -> Result<(), io::Error> {
        let mut buf = V4l2Buffer {
            index,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        ioctl_io(self.file.as_raw_fd(), videoc_qbuf(), &mut buf)
    }

    fn dqbuf(&self) -> Result<Option<u32>, io::Error> {
        let mut buf = V4l2Buffer {
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        match ioctl_io(self.file.as_raw_fd(), videoc_dqbuf(), &mut buf) {
            Ok(()) => Ok(Some(buf.index)),
            Err(err) if err.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn stream_on(&mut self, held: &HashSet<u32>) -> Result<(), io::Error> {
        for index in 0..self.count {
            if !held.contains(&index) {
                self.qbuf(index)?;
            }
        }
        let mut type_arg: i32 = V4L2_BUF_TYPE_VIDEO_CAPTURE as i32;
        ioctl_io(self.file.as_raw_fd(), videoc_streamon(), &mut type_arg)?;
        self.streaming = true;
        Ok(())
    }

    /// Setting background again is a no-op while we are the only opener.
    /// Once a higher-priority fd exists, this ioctl is rejected and the
    /// caller must close the device: `QBUF` and `DQBUF` are not priority
    /// checked, so a streaming loop would not notice otherwise.
    fn preempted_by_another_opener(&self) -> Result<bool, io::Error> {
        let mut prio = V4L2_PRIORITY_BACKGROUND;
        match ioctl_io(self.file.as_raw_fd(), videoc_s_priority(), &mut prio) {
            Ok(()) => Ok(false),
            Err(e) if is_busy(&e) => Ok(true),
            Err(e) => Err(e),
        }
    }

    fn stream_off(&mut self) {
        if !self.streaming {
            return;
        }
        let mut type_arg: i32 = V4L2_BUF_TYPE_VIDEO_CAPTURE as i32;
        // EBUSY here means a higher-priority opener already won. Closing the
        // fd is what frees the queue; the ioctl itself is priority-checked.
        if let Err(e) = ioctl_io(self.file.as_raw_fd(), videoc_streamoff(), &mut type_arg) {
            if !is_busy(&e) {
                eprintln!("[webcam] VIDIOC_STREAMOFF failed: {e}");
            }
        }
        self.streaming = false;
    }

    /// Drop the vb2 queue. Call only after the GPU's dma-buf dups are closed,
    /// or the driver keeps the buffers until those fds go away.
    fn free_buffers(&mut self) {
        self.stream_off();
        let mut req = V4l2RequestBuffers {
            count: 0,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        if let Err(e) = ioctl_io(self.file.as_raw_fd(), videoc_reqbufs(), &mut req) {
            if !is_busy(&e) {
                eprintln!("[webcam] VIDIOC_REQBUFS(0) failed: {e}");
            }
        }
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.stream_off();
    }
}

/// Capture thread. `requeue` receives buffer indexes the GPU has finished
/// sampling, and `WEBCAM_YIELD_ACK` once a yield's imports are destroyed.
/// The returned receiver gets `Started`, then `Frame`, and `Yielded` when
/// the device is about to be closed.
pub fn spawn(
    path: String,
    enabled: Arc<AtomicBool>,
    wake: Arc<(Mutex<()>, Condvar)>,
    requeue: Receiver<u32>,
) -> Receiver<WebcamMsg> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("presence-webcam".into())
        .spawn(move || run(path, enabled, wake, requeue, tx))
        .ok();
    rx
}

enum PumpEnd {
    Dead,
    /// Another opener won the device. Close it so their `REQBUFS` can proceed.
    YieldBusy,
    /// Session hidden. Close it so the camera is free while we are parked.
    YieldHidden,
}

fn run(
    path: String,
    enabled: Arc<AtomicBool>,
    wake: Arc<(Mutex<()>, Condvar)>,
    requeue: Receiver<u32>,
    tx: SyncSender<WebcamMsg>,
) {
    let mut logged_open_fail = false;
    loop {
        if !enabled.load(Ordering::Relaxed) {
            park(&wake, Duration::from_secs(1));
            continue;
        }
        let mut camera = match Camera::open(&path) {
            Ok(camera) => {
                logged_open_fail = false;
                camera
            }
            Err(e) => {
                if !logged_open_fail {
                    eprintln!("[webcam] {e} — avatar keeps sampling the desktop");
                    logged_open_fail = true;
                }
                park(&wake, Duration::from_secs(2));
                continue;
            }
        };
        let msg = WebcamMsg::Started {
            width: camera.width,
            height: camera.height,
            stride: camera.stride,
            fds: std::mem::take(&mut camera.fds),
        };
        if tx.send(msg).is_err() {
            return;
        }

        match pump(&mut camera, &enabled, &wake, &requeue, &tx) {
            PumpEnd::Dead => return,
            PumpEnd::YieldBusy => {
                if release(&mut camera, &tx, &requeue) {
                    return;
                }
                eprintln!("[webcam] released the camera — another app has it");
                thread::sleep(Duration::from_secs(2));
            }
            PumpEnd::YieldHidden => {
                if release(&mut camera, &tx, &requeue) {
                    return;
                }
                eprintln!("[webcam] stream off — session hidden");
            }
        }
    }
}

fn park(wake: &Arc<(Mutex<()>, Condvar)>, dur: Duration) {
    let (lock, cvar) = &**wake;
    let guard = lock.lock().unwrap();
    let _ = cvar.wait_timeout(guard, dur);
}

/// STREAMOFF, tell the main thread, wait until its dma-buf imports are
/// gone, then free the queue. Returns true when the main thread is gone.
fn release(camera: &mut Camera, tx: &SyncSender<WebcamMsg>, requeue: &Receiver<u32>) -> bool {
    camera.stream_off();
    if tx.send(WebcamMsg::Yielded).is_err() {
        return true;
    }
    if !wait_ack(requeue) {
        return true;
    }
    camera.free_buffers();
    false
}

fn wait_ack(requeue: &Receiver<u32>) -> bool {
    loop {
        match requeue.recv_timeout(Duration::from_millis(200)) {
            Ok(index) if index == WEBCAM_YIELD_ACK => return true,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

fn pump(
    camera: &mut Camera,
    enabled: &AtomicBool,
    wake: &Arc<(Mutex<()>, Condvar)>,
    requeue: &Receiver<u32>,
    tx: &SyncSender<WebcamMsg>,
) -> PumpEnd {
    let mut held: HashSet<u32> = HashSet::new();
    let mut logged_prio = false;
    loop {
        match camera.preempted_by_another_opener() {
            Ok(true) => return PumpEnd::YieldBusy,
            Ok(false) => {}
            Err(e) => {
                if !logged_prio {
                    eprintln!("[webcam] priority check failed: {e}");
                    logged_prio = true;
                }
            }
        }
        while let Ok(index) = requeue.try_recv() {
            if index == WEBCAM_YIELD_ACK {
                continue;
            }
            held.remove(&index);
            if camera.streaming {
                if let Err(e) = camera.qbuf(index) {
                    if is_busy(&e) {
                        return PumpEnd::YieldBusy;
                    }
                    eprintln!("[webcam] requeue of buffer {index} failed: {e}");
                }
            }
        }

        if !enabled.load(Ordering::Relaxed) {
            return PumpEnd::YieldHidden;
        }

        if !camera.streaming {
            match camera.stream_on(&held) {
                Ok(()) => eprintln!("[webcam] streaming"),
                Err(e) if is_busy(&e) => return PumpEnd::YieldBusy,
                Err(e) => {
                    eprintln!("[webcam] {e}");
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
            }
        }

        match camera.dqbuf() {
            Ok(Some(index)) => match tx.try_send(WebcamMsg::Frame(index)) {
                Ok(()) => {
                    held.insert(index);
                }
                // The main thread already has a frame it has not sampled.
                // This one never reached the GPU, so the camera can reuse it.
                Err(TrySendError::Full(_)) => {
                    if let Err(e) = camera.qbuf(index) {
                        if is_busy(&e) {
                            return PumpEnd::YieldBusy;
                        }
                        eprintln!("[webcam] requeue of dropped buffer {index} failed: {e}");
                    }
                }
                Err(TrySendError::Disconnected(_)) => {
                    let _ = camera.qbuf(index);
                    return PumpEnd::Dead;
                }
            },
            Ok(None) => park(wake, Duration::from_millis(20)),
            Err(e) if is_busy(&e) => return PumpEnd::YieldBusy,
            Err(e) => {
                eprintln!("[webcam] VIDIOC_DQBUF failed: {e}");
                camera.stream_off();
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_ioctl_matches_the_kernel_header() {
        assert_eq!(videoc_expbuf(), 0xc040_5610);
        assert_eq!(videoc_s_priority(), 0x4004_5644);
        assert_eq!(V4L2_PRIORITY_BACKGROUND, 1);
        assert_eq!(std::mem::size_of::<V4l2ExportBuffer>(), 64);
        assert_eq!(V4L2_PIX_FMT_YUYV, 0x5659_5559);
    }

    #[test]
    fn open_on_missing_device_fails_clearly() {
        match Camera::open("/dev/does-not-exist-presence-webcam") {
            Err(err) => assert!(err.contains("couldn't open"), "{err}"),
            Ok(_) => panic!("opened a missing device"),
        }
    }
}

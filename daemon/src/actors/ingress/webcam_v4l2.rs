#![allow(dead_code)] // real code, compile-verified only — not yet called from main.rs (no ingress-tick loop exists to wire it into). See module doc comment for the honest real-vs-tested boundary.
//! Webcam capture via raw V4L2 `ioctl(2)` calls — no client library needed

//! at all (V4L2 talks to a `/dev/videoN` file descriptor directly), so
//! unlike the Wayland screen-capture backend this has zero runtime
//! library dependency, only a device node. Struct layouts and ioctl
//! request-code encoding below were taken directly from this sandbox's
//! own `/usr/include/linux/videodev2.h` and `/usr/include/asm-generic/
//! ioctl.h` — not from memory — specifically to avoid the failure mode of
//! a plausible-looking but subtly wrong FFI struct silently corrupting
//! memory on an ioctl call. Ioctl numbers are computed with the same
//! bit-packing formula the kernel headers use, from real struct sizes,
//! rather than hand-copied as magic numbers.
//!
//! Still: this sandbox has no `/dev/video0`, so — same honesty note as
//! `screen_wlr.rs` — this compiles here but has never actually opened a
//! camera. `V4l2WebcamSource::open` will fail clearly (ENOENT) if pointed
//! at a path that doesn't exist, which is the correct failure mode for a
//! headless build box; it needs a real run on hardware with a camera to
//! prove out the actual capture loop.

use super::{FrameSource, IngressFrame};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

// ---- ioctl request-code encoding, from asm-generic/ioctl.h ----
const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_NONE: u32 = 0;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn ioc(dir: u32, ty: u32, nr: u32, size: u32) -> u32 {
    (dir << IOC_DIRSHIFT) | (ty << IOC_TYPESHIFT) | (nr << IOC_NRSHIFT) | (size << IOC_SIZESHIFT)
}
const V_TYPE: u32 = b'V' as u32;

// ---- struct layouts, field-for-field from videodev2.h ----

#[repr(C)]
#[derive(Clone, Copy)]
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
const _: () = assert!(std::mem::size_of::<V4l2PixFormat>() == 48);

// `v4l2_format.fmt` is a C union whose size is fixed at 200 bytes (the
// `raw_data[200]` member is what pins that) — but its *alignment* is 8,
// not 4, because another variant we don't use (`struct v4l2_window`)
// contains a pointer. That one detail cost a real bug here, caught only
// by cross-checking `sizeof(struct v4l2_format)` against a tiny C program
// compiled with the actual kernel header (208 bytes) rather than trusting
// hand-computed Rust layout math (which gave 204 — silently wrong by 4
// bytes, from missing that alignment requirement). `align(8)` below fixes
// it: `type_` gets padded to offset 8 before the union starts, matching
// the real struct exactly.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
union V4l2FormatUnion {
    pix: V4l2PixFormat,
    raw: [u8; 200],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct V4l2Format {
    type_: u32,
    fmt: V4l2FormatUnion,
}
const _: () = assert!(std::mem::size_of::<V4l2Format>() == 208);

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
#[derive(Clone, Copy)]
union V4l2BufferM {
    offset: u32,
    userptr: u64,
    planes: *mut std::ffi::c_void,
    fd: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
union V4l2BufferTail {
    request_fd: i32,
    reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
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
        // Safety: an all-zero bit pattern is valid for every field here
        // (unions included — zero is a valid `u32`/`i32`/null-equivalent
        // reading of each variant), which is the standard way the kernel
        // itself expects these structs to be pre-zeroed before an ioctl.
        unsafe { std::mem::zeroed() }
    }
}

// ---- enum values actually used, from videodev2.h ----
const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;
const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
/// `v4l2_fourcc('R','G','B','3')` — RGB-8-8-8, computed the same way the
/// kernel macro does rather than hand-typed as an opaque hex literal.
const V4L2_PIX_FMT_RGB24: u32 = (b'R' as u32) | ((b'G' as u32) << 8) | ((b'B' as u32) << 16) | ((b'3' as u32) << 24);

fn videoc_querycap() -> u32 { ioc(IOC_READ, V_TYPE, 0, std::mem::size_of::<V4l2Capability>() as u32) }
fn videoc_s_fmt() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 5, std::mem::size_of::<V4l2Format>() as u32) }
fn videoc_reqbufs() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 8, std::mem::size_of::<V4l2RequestBuffers>() as u32) }
fn videoc_querybuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 9, std::mem::size_of::<V4l2Buffer>() as u32) }
fn videoc_qbuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 15, std::mem::size_of::<V4l2Buffer>() as u32) }
fn videoc_dqbuf() -> u32 { ioc(IOC_READ | IOC_WRITE, V_TYPE, 17, std::mem::size_of::<V4l2Buffer>() as u32) }
fn videoc_streamon() -> u32 { ioc(IOC_WRITE, V_TYPE, 18, std::mem::size_of::<i32>() as u32) }
fn videoc_streamoff() -> u32 { ioc(IOC_WRITE, V_TYPE, 19, std::mem::size_of::<i32>() as u32) }

unsafe fn ioctl_checked<T>(fd: i32, request: u32, arg: *mut T, what: &str) -> Result<(), String> {
    let ret = unsafe { libc::ioctl(fd, request as libc::c_ulong, arg) };
    if ret < 0 {
        Err(format!("{what} failed: {}", io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

struct MappedBuffer {
    ptr: *mut libc::c_void,
    len: usize,
}

/// One open V4L2 device, streaming RGB24 frames via a single mmap'd
/// capture buffer (the simplest correct V4L2 loop: request 1 buffer,
/// queue it, stream on, dequeue/read/requeue in a loop — no double
/// buffering, which real-time-critical capture would want, but is the
/// right amount of complexity for "prove the ingress path is real").
pub struct V4l2WebcamSource {
    file: File,
    width: u32,
    height: u32,
    buffer: MappedBuffer,
    streaming: bool,
}

impl V4l2WebcamSource {
    pub fn open(path: &str, width: u32, height: u32) -> Result<Self, String> {
        let file = File::options()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| format!("couldn't open {path}: {e}"))?;
        let fd = file.as_raw_fd();

        let mut cap = unsafe { std::mem::zeroed::<V4l2Capability>() };
        unsafe { ioctl_checked(fd, videoc_querycap(), &mut cap, "VIDIOC_QUERYCAP")? };
        if cap.capabilities & V4L2_CAP_VIDEO_CAPTURE == 0 {
            return Err(format!("{path} doesn't report V4L2_CAP_VIDEO_CAPTURE"));
        }

        let mut fmt = V4l2Format {
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            fmt: V4l2FormatUnion {
                pix: V4l2PixFormat {
                    width,
                    height,
                    pixelformat: V4L2_PIX_FMT_RGB24,
                    field: V4L2_FIELD_NONE,
                    ..Default::default()
                },
            },
        };
        unsafe { ioctl_checked(fd, videoc_s_fmt(), &mut fmt, "VIDIOC_S_FMT")? };
        // The driver may adjust width/height/format to something it
        // actually supports; read back what we really got.
        let negotiated = unsafe { fmt.fmt.pix };
        if negotiated.pixelformat != V4L2_PIX_FMT_RGB24 {
            return Err(format!(
                "{path} doesn't support RGB24 (got fourcc {:#x}) — this minimal backend doesn't do YUV conversion",
                negotiated.pixelformat
            ));
        }

        let mut reqbufs = V4l2RequestBuffers {
            count: 1,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        unsafe { ioctl_checked(fd, videoc_reqbufs(), &mut reqbufs, "VIDIOC_REQBUFS")? };
        if reqbufs.count == 0 {
            return Err(format!("{path} granted 0 capture buffers"));
        }

        let mut querybuf = V4l2Buffer {
            index: 0,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        unsafe { ioctl_checked(fd, videoc_querybuf(), &mut querybuf, "VIDIOC_QUERYBUF")? };

        let length = querybuf.length as usize;
        let offset = unsafe { querybuf.m.offset } as libc::off_t;
        let ptr = unsafe {
            libc::mmap(std::ptr::null_mut(), length, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd, offset)
        };
        if ptr == libc::MAP_FAILED {
            return Err(format!("mmap of capture buffer failed: {}", io::Error::last_os_error()));
        }

        Ok(Self {
            file,
            width: negotiated.width,
            height: negotiated.height,
            buffer: MappedBuffer { ptr, len: length },
            streaming: false,
        })
    }

    fn start_streaming(&mut self) -> Result<(), String> {
        let fd = self.file.as_raw_fd();
        let mut qbuf = V4l2Buffer {
            index: 0,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        unsafe { ioctl_checked(fd, videoc_qbuf(), &mut qbuf, "VIDIOC_QBUF")? };
        let mut type_arg: i32 = V4L2_BUF_TYPE_VIDEO_CAPTURE as i32;
        unsafe { ioctl_checked(fd, videoc_streamon(), &mut type_arg, "VIDIOC_STREAMON")? };
        self.streaming = true;
        Ok(())
    }
}

impl FrameSource for V4l2WebcamSource {
    fn next_frame(&mut self) -> Result<Option<IngressFrame>, String> {
        if !self.streaming {
            self.start_streaming()?;
        }
        let fd = self.file.as_raw_fd();

        let mut dqbuf = V4l2Buffer {
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        unsafe { ioctl_checked(fd, videoc_dqbuf(), &mut dqbuf, "VIDIOC_DQBUF")? };

        // RGB24 is already 8-8-8 RGB with no alpha; expand to RGBA8 to
        // match `IngressFrame`'s contract.
        let bytes_used = dqbuf.bytesused as usize;
        let rgb: &[u8] = unsafe { std::slice::from_raw_parts(self.buffer.ptr as *const u8, bytes_used) };
        let mut rgba = Vec::with_capacity((self.width * self.height * 4) as usize);
        for px in rgb.chunks_exact(3) {
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }

        // Re-queue the same buffer immediately so the driver can keep
        // capturing into it.
        let mut qbuf = V4l2Buffer {
            index: dqbuf.index,
            type_: V4L2_BUF_TYPE_VIDEO_CAPTURE,
            memory: V4L2_MEMORY_MMAP,
            ..Default::default()
        };
        unsafe { ioctl_checked(fd, videoc_qbuf(), &mut qbuf, "VIDIOC_QBUF (requeue)")? };

        Ok(Some(IngressFrame { width: self.width, height: self.height, rgba }))
    }
}

impl Drop for V4l2WebcamSource {
    fn drop(&mut self) {
        if self.streaming {
            let mut type_arg: i32 = V4L2_BUF_TYPE_VIDEO_CAPTURE as i32;
            let _ = unsafe { ioctl_checked(self.file.as_raw_fd(), videoc_streamoff(), &mut type_arg, "VIDIOC_STREAMOFF") };
        }
        unsafe {
            libc::munmap(self.buffer.ptr, self.buffer.len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_kernel_abi() {
        // Cross-checked two ways: against this sandbox's own
        // /usr/include/linux/videodev2.h field-by-field (see module doc
        // comment), and independently against `sizeof()` from a tiny C
        // program compiled with that same header (see
        // daemon/docs/gpu-splat-pipeline.md's ingress section for the
        // transcript) — the second check is what caught `V4l2Format`
        // actually being 208 bytes, not the 204 hand-computed layout math
        // gave before `V4l2FormatUnion` had its alignment fixed.
        assert_eq!(std::mem::size_of::<V4l2PixFormat>(), 48);
        assert_eq!(std::mem::size_of::<V4l2Format>(), 208); // gcc: sizeof(struct v4l2_format) == 208
        assert_eq!(std::mem::size_of::<V4l2Capability>(), 104); // gcc: 104
        assert_eq!(std::mem::size_of::<V4l2RequestBuffers>(), 20); // gcc: 20
        assert_eq!(std::mem::size_of::<V4l2Buffer>(), 88); // gcc: 88
    }

    #[test]
    fn ioctl_numbers_match_gcc_compiled_kernel_header() {
        // Full values (not just the nr/type sub-fields), each one printed
        // by a small C program that #included the real
        // <linux/videodev2.h> in this sandbox and printed
        // VIDIOC_QUERYCAP etc. This is the strongest check available
        // short of running against a real device: it confirms both the
        // struct sizes *and* the encoding formula agree with what a C
        // compiler actually produces from the kernel's own macros.
        assert_eq!(videoc_querycap(), 0x80685600);
        assert_eq!(videoc_s_fmt(), 0xc0d05605);
        assert_eq!(videoc_reqbufs(), 0xc0145608);
        assert_eq!(videoc_querybuf(), 0xc0585609);
        assert_eq!(videoc_qbuf(), 0xc058560f);
        assert_eq!(videoc_dqbuf(), 0xc0585611);
        assert_eq!(videoc_streamon(), 0x40045612);
        assert_eq!(videoc_streamoff(), 0x40045613);
    }

    #[test]
    fn open_on_missing_device_fails_clearly_not_a_panic() {
        let result = V4l2WebcamSource::open("/dev/does-not-exist-in-this-sandbox", 640, 480);
        assert!(result.is_err());
    }

    #[test]
    fn fourcc_rgb24_matches_kernel_macro() {
        // Recomputed independently the way v4l2_fourcc('R','G','B','3') is
        // defined in videodev2.h, as a cross-check against the constant
        // used above.
        let expected = (b'R' as u32) | (b'G' as u32) << 8 | (b'B' as u32) << 16 | (b'3' as u32) << 24;
        assert_eq!(V4L2_PIX_FMT_RGB24, expected);
        assert_eq!(V4L2_PIX_FMT_RGB24, 0x3342_4752);
    }
}

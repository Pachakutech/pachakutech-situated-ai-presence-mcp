# Webcam ingress, same path as the desktop

The avatar’s colors come from frozen rectangles of whatever picture sources are up. The desktop is one source. The webcam is another. Each snapshot picks a source with equal probability among the sources that currently have a frame, then a rectangle inside that source. One source up means every sample comes from it, which is how the desktop looks today when the camera is missing. Adding a source does not switch the avatar over to it.

No pixel of either source is read on the CPU, and the CPU does not write a color into the splats.

## What the camera actually offers

`/dev/video0` is the integrated UVC camera (`uvcvideo`). It captures MJPEG or YUYV. It does not capture RGB. MJPEG would have to be decoded before it could be sampled. YUYV is uncompressed, so the frame can stay in a dma-buf.

`/dev/video1` is metadata. `/dev/video10` is a v4l2loopback output (`PhoneCam`), not this capture.

The existing `webcam_v4l2.rs` path mmaps RGB24 and expands it to a `Vec<u8>`. This camera will not negotiate RGB24, and that read is a CPU copy. Coloration does not use it.

## GPU path

1. Open `PRESENCE_WEBCAM`, or `/dev/video0`. Set YUYV at 640×480. Request three mmap buffers and export each with `VIDIOC_EXPBUF`. The export is a dma-buf fd. Nothing maps or reads the bytes.
2. Import each fd once, the same way the desktop dmabuf is imported (`VkImportMemoryFdInfoKHR`, linear modifier). The memory is YUYV, two bytes per pixel, so the Vulkan format is `R8G8_UNORM` labeled with DRM `RG88`: `.r` is Y, `.g` is the shared chroma. The first byte is R, which matches YUYV’s layout.
3. The capture thread dequeues a buffer and hands over its index. The buffer is not queued again until the GPU fence of the draw that sampled it has signaled. The device is opened at `V4L2_PRIORITY_BACKGROUND`. Queue and dequeue are not priority-checked, so while streaming the thread sets that priority again; `EBUSY` means a higher-priority opener is present. Then, or when the session is hidden, the stream is stopped, the GPU imports are destroyed after the present fence, and the fd is closed so the other app can allocate buffers. The daemon retries the open later, still at background priority. While the camera is released, new patches come from the desktop. Tiles already copied stay until a later desktop snapshot replaces them.
4. `screen_patch.comp` already writes a rectangle into the atlas. It now has two samplers. Source 0 samples the desktop image (the imported dmabuf, or the SHM path’s uploaded image) and still stretches that rectangle to fill the tile. Source 1 fetches the YUYV image and converts it to RGB in the shader. This camera reports colorspace sRGB, encoding 601, quantization default, and for YUYV that default is limited-range BT.601, so the shader expands studio swing. A webcam rectangle is copied with a uniform fit and no upscale, anchored at the tile origin. The disc shader places that content once on the body region, at the rectangle’s own aspect, magnified by two, and crops whatever does not fit. It does not repeat, and the glass lens still bends the sample.

If the camera is missing, busy, or the import fails, the daemon logs it and keeps sampling the desktop. A failed camera does not stop the process.

## Scheduler

`Appearance::tick` takes the list of sources that have a frame. The list is built by the overlay from the desktop extent and, once a webcam buffer is current, the webcam extent. The pick is `sources[rng % sources.len()]`. The rectangle is in that source’s pixels. Face-first and the short gap between regions stay as they are.

## Not this

- No mmap read, no JPEG decode on the CPU, no upload of webcam bytes through a staging buffer.
- No “if webcam, use the webcam” switch.
- The webcam rectangle is not mapped across the whole body, and it is not tiled. One region, one placement.
- Skeletal skinning, the jaw, and speech stay as they are. This only chooses the picture a region’s tile is copied from.

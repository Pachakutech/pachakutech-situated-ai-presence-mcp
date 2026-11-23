//! The hover row is its own layer-shell surface, outside the tint.
//! Keyboard focus exists only while the note field is open, and only on this surface.
//! `get_keyboard` stays in this file so `overlay.rs` does not take the keyboard.

use std::os::fd::{AsFd, OwnedFd};

use wayland_client::protocol::{wl_buffer, wl_compositor, wl_keyboard, wl_seat, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Dispatch, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::control_row::{paint_buffer, Paint, RowInput};

use super::{OverlayState, ShmMap};

#[derive(Clone, Copy)]
pub(super) struct RowToken;

pub(super) struct RowSlot {
    pub(super) origin_x: i32,
    pub(super) origin_y: i32,
    w: i32,
    h: i32,
    editor: bool,
    surface: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pool: Option<wl_shm_pool::WlShmPool>,
    buffer: Option<wl_buffer::WlBuffer>,
    map: Option<ShmMap>,
    fd: Option<OwnedFd>,
    pub(super) configured: bool,
    free: bool,
    buf_w: i32,
    buf_h: i32,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    stamp: String,
}

pub(super) fn is_row(state: &OverlayState, surface: &wl_surface::WlSurface) -> bool {
    state.row.as_ref().is_some_and(|row| &row.surface == surface)
}

pub(super) fn sync(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: Option<&Paint>) {
    let Some(paint) = paint else {
        destroy(state);
        return;
    };
    if state.row.is_none() && spawn(state, qh, paint).is_err() {
        return;
    }
    place(state, qh, paint);
}

pub(super) fn paint_now(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: &Paint) {
    let stamp = format!(
        "{:?}:{}:{}:{}:{}:{}:{}",
        paint.color, paint.editor as u8, paint.text, paint.origin_x, paint.origin_y, paint.width, paint.height
    );
    let ready = state.row.as_ref().is_some_and(|row| row.configured);
    if !ready {
        return;
    }
    let same = state.row.as_ref().is_some_and(|row| row.stamp == stamp && row.buffer.is_some());
    if same {
        return;
    }
    let busy = state.row.as_ref().is_some_and(|row| row.buffer.is_some() && !row.free);
    if busy {
        return;
    }
    let Some(shm) = state.shm.clone() else { return };
    if let Err(e) = ensure_buffer(state, qh, &shm, paint) {
        eprintln!("[highlight] row {e}");
        return;
    }
    {
        let Some(row) = state.row.as_mut() else { return };
        if let Some(map) = row.map.as_mut() {
            paint_buffer(map.bytes(), paint);
        }
        row.stamp = stamp;
        row.free = false;
    }
    let Some(row) = state.row.as_ref() else { return };
    let Some(buffer) = row.buffer.as_ref() else { return };
    row.surface.attach(Some(buffer), 0, 0);
    row.surface.damage_buffer(0, 0, paint.width.max(1), paint.height.max(1));
    row.surface.commit();
}

fn spawn(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: &Paint) -> Result<(), String> {
    let compositor = state.compositor.clone().ok_or("row: no compositor")?;
    let shell = state.layer_shell.clone().ok_or("row: no layer shell")?;
    let output = state.output.clone();
    let surface = compositor.create_surface(qh, ());
    set_row_input(&compositor, qh, &surface, paint);
    let layer = shell.get_layer_surface(
        &surface,
        output.as_ref(),
        zwlr_layer_shell_v1::Layer::Top,
        "pachakutech-row".to_string(),
        qh,
        RowToken,
    );
    layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    layer.set_size(paint.width.max(1) as u32, paint.height.max(1) as u32);
    layer.set_margin(paint.origin_y, 0, 0, paint.origin_x);
    surface.commit();
    state.row = Some(RowSlot {
        origin_x: paint.origin_x,
        origin_y: paint.origin_y,
        w: paint.width,
        h: paint.height,
        editor: false,
        surface,
        layer,
        pool: None,
        buffer: None,
        map: None,
        fd: None,
        configured: false,
        free: true,
        buf_w: 0,
        buf_h: 0,
        keyboard: None,
        stamp: String::new(),
    });
    arm_keys(state, qh, paint.editor);
    Ok(())
}

fn place(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: &Paint) {
    let compositor = state.compositor.clone();
    let surface = {
        let Some(row) = state.row.as_mut() else { return };
        let moved = paint.origin_x != row.origin_x || paint.origin_y != row.origin_y;
        let resized = paint.width != row.w || paint.height != row.h;
        row.origin_x = paint.origin_x;
        row.origin_y = paint.origin_y;
        if resized {
            row.w = paint.width;
            row.h = paint.height;
            row.configured = false;
            row.stamp.clear();
            release_buffer(row);
            row.layer.set_size(paint.width.max(1) as u32, paint.height.max(1) as u32);
        }
        if moved || resized {
            row.layer.set_margin(paint.origin_y, 0, 0, paint.origin_x);
            row.surface.commit();
        }
        row.surface.clone()
    };
    // The pointer's screen origin moves with the layer. A field that grows
    // does not leave the click on the pad's old x.
    if state.on_row {
        state.pointer_origin = (paint.origin_x, paint.origin_y);
    }
    if let Some(compositor) = compositor.as_ref() {
        set_row_input(compositor, qh, &surface, paint);
    }
    arm_keys(state, qh, paint.editor);
}

fn arm_keys(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, editor: bool) {
    let seat = state.seat.clone();
    let Some(row) = state.row.as_mut() else { return };
    if editor {
        row.layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive);
        if row.keyboard.is_none() {
            if let Some(seat) = seat {
                row.keyboard = Some(seat.get_keyboard(qh, RowToken));
            }
        }
        row.editor = true;
    } else if row.editor || row.keyboard.is_some() {
        row.layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        if let Some(keyboard) = row.keyboard.take() {
            keyboard.release();
        }
        row.editor = false;
    }
}

fn destroy(state: &mut OverlayState) {
    let Some(mut row) = state.row.take() else { return };
    row.layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    if let Some(keyboard) = row.keyboard.take() {
        keyboard.release();
    }
    release_buffer(&mut row);
    row.layer.destroy();
    row.surface.destroy();
    state.on_row = false;
}

fn set_row_input(
    compositor: &wl_compositor::WlCompositor,
    qh: &QueueHandle<OverlayState>,
    surface: &wl_surface::WlSurface,
    paint: &Paint,
) {
    let region = compositor.create_region(qh, ());
    for (_, rect) in &paint.squares {
        let x = rect.x - paint.origin_x;
        let y = rect.y - paint.origin_y;
        if rect.w > 0 && rect.h > 0 {
            region.add(x, y, rect.w, rect.h);
        }
    }
    surface.set_input_region(Some(&region));
    region.destroy();
}

fn ensure_buffer(
    state: &mut OverlayState,
    qh: &QueueHandle<OverlayState>,
    shm: &wl_shm::WlShm,
    paint: &Paint,
) -> Result<(), String> {
    let bw = paint.width.max(1);
    let bh = paint.height.max(1);
    let fits = state.row.as_ref().is_some_and(|row| row.buffer.is_some() && row.buf_w == bw && row.buf_h == bh);
    if fits {
        return Ok(());
    }
    if let Some(row) = state.row.as_mut() {
        release_buffer(row);
    }
    let stride = bw.saturating_mul(4);
    let size = stride.saturating_mul(bh);
    if size <= 0 {
        return Err("row buffer is empty".into());
    }
    let fd = crate::actors::ingress::screen_wlr::create_anonymous_shm(size as usize)?;
    let pool = shm.create_pool(fd.as_fd(), size, qh, ());
    let buffer = pool.create_buffer(0, bw, bh, stride, wl_shm::Format::Argb8888, qh, RowToken);
    let map = ShmMap::writable(&fd, size as usize)?;
    let Some(row) = state.row.as_mut() else {
        return Err("row is gone".into());
    };
    row.pool = Some(pool);
    row.buffer = Some(buffer);
    row.map = Some(map);
    row.fd = Some(fd);
    row.buf_w = bw;
    row.buf_h = bh;
    row.free = true;
    Ok(())
}

fn release_buffer(row: &mut RowSlot) {
    if let Some(buffer) = row.buffer.take() {
        buffer.destroy();
    }
    if let Some(pool) = row.pool.take() {
        pool.destroy();
    }
    row.map = None;
    row.fd = None;
    row.free = true;
    row.buf_w = 0;
    row.buf_h = 0;
}

fn key_char(key: u32, shift: bool) -> Option<char> {
    let lower = match key {
        2 => '1',
        3 => '2',
        4 => '3',
        5 => '4',
        6 => '5',
        7 => '6',
        8 => '7',
        9 => '8',
        10 => '9',
        11 => '0',
        12 => '-',
        13 => '=',
        16 => 'q',
        17 => 'w',
        18 => 'e',
        19 => 'r',
        20 => 't',
        21 => 'y',
        22 => 'u',
        23 => 'i',
        24 => 'o',
        25 => 'p',
        30 => 'a',
        31 => 's',
        32 => 'd',
        33 => 'f',
        34 => 'g',
        35 => 'h',
        36 => 'j',
        37 => 'k',
        38 => 'l',
        39 => ';',
        40 => '\'',
        44 => 'z',
        45 => 'x',
        46 => 'c',
        47 => 'v',
        48 => 'b',
        49 => 'n',
        50 => 'm',
        51 => ',',
        52 => '.',
        53 => '/',
        57 => ' ',
        _ => return None,
    };
    if !shift {
        return Some(lower);
    }
    Some(match lower {
        '1' => '!',
        '2' => '@',
        '3' => '#',
        '4' => '$',
        '5' => '%',
        '6' => '^',
        '7' => '&',
        '8' => '*',
        '9' => '(',
        '0' => ')',
        '-' => '_',
        '=' => '+',
        ';' => ':',
        '\'' => '"',
        ',' => '<',
        '.' => '>',
        '/' => '?',
        ch if ch.is_ascii_lowercase() => ch.to_ascii_uppercase(),
        ch => ch,
    })
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, RowToken> for OverlayState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &RowToken,
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(row) = state.row.as_mut() else {
            if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
                layer_surface.ack_configure(serial);
            }
            return;
        };
        if &row.layer != layer_surface {
            if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
                layer_surface.ack_configure(serial);
            }
            return;
        }
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                layer_surface.ack_configure(serial);
                row.configured = true;
            }
            zwlr_layer_surface_v1::Event::Closed => {
                row.configured = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, RowToken> for OverlayState {
    fn event(
        state: &mut Self,
        _proxy: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _data: &RowToken,
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            if let Some(row) = state.row.as_mut() {
                row.free = true;
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, RowToken> for OverlayState {
    fn event(
        state: &mut Self,
        _proxy: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _data: &RowToken,
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let wl_keyboard::Event::Key { key, state: key_state, .. } = event else {
            return;
        };
        let down = matches!(key_state, WEnum::Value(wl_keyboard::KeyState::Pressed));
        if key == 42 || key == 54 {
            if down {
                state.row_shift = state.row_shift.saturating_add(1);
            } else {
                state.row_shift = state.row_shift.saturating_sub(1);
            }
            return;
        }
        if !down {
            return;
        }
        if key == 14 {
            state.row_input.push(RowInput::Backspace);
            return;
        }
        if key == 28 || key == 96 {
            state.row_input.push(RowInput::Text("\n".into()));
            return;
        }
        if let Some(ch) = key_char(key, state.row_shift > 0) {
            state.row_input.push(RowInput::Text(ch.to_string()));
        }
    }
}

// The seat is named so `get_keyboard` is the call above. This keeps the import live
// if a later edit reaches the seat through the protocol module.
#[allow(dead_code)]
fn _seat_ty(seat: &wl_seat::WlSeat) -> &wl_seat::WlSeat {
    seat
}

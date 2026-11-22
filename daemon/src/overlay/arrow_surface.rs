//! Back and forward bubbles. Each is its own layer-shell surface.
//! The hit target is the bubble. The rest of the overlay stays click-through.
//! Keyboard interactivity stays `None`.

use std::os::fd::{AsFd, OwnedFd};

use wayland_client::protocol::{wl_buffer, wl_compositor, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Dispatch, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::document_arrows::{paint_bubble, Arrow, BubblePaint, BUBBLE_PX};

use super::{OverlayState, ShmMap};

pub(super) struct BubbleSlot {
    pub(super) origin_x: i32,
    pub(super) origin_y: i32,
    surface: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pool: Option<wl_shm_pool::WlShmPool>,
    buffer: Option<wl_buffer::WlBuffer>,
    map: Option<ShmMap>,
    fd: Option<OwnedFd>,
    pub(super) configured: bool,
    free: bool,
    stamp: String,
}

pub(super) fn which(state: &OverlayState, surface: &wl_surface::WlSurface) -> Option<Arrow> {
    if state.arrow_back.as_ref().is_some_and(|slot| &slot.surface == surface) {
        return Some(Arrow::Back);
    }
    if state.arrow_forward.as_ref().is_some_and(|slot| &slot.surface == surface) {
        return Some(Arrow::Forward);
    }
    None
}

pub(super) fn origin(state: &OverlayState, which: Arrow) -> (i32, i32) {
    let slot = match which {
        Arrow::Back => state.arrow_back.as_ref(),
        Arrow::Forward => state.arrow_forward.as_ref(),
    };
    slot.map(|slot| (slot.origin_x, slot.origin_y)).unwrap_or((0, 0))
}

pub(super) fn waiting(state: &OverlayState) -> bool {
    state.arrow_back.as_ref().is_some_and(|slot| !slot.configured)
        || state.arrow_forward.as_ref().is_some_and(|slot| !slot.configured)
}

pub(super) fn sync(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paints: &[BubblePaint]) {
    sync_one(state, qh, Arrow::Back, paints.iter().find(|paint| paint.which == Arrow::Back));
    sync_one(state, qh, Arrow::Forward, paints.iter().find(|paint| paint.which == Arrow::Forward));
}

pub(super) fn paint_now(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paints: &[BubblePaint]) {
    for paint in paints {
        paint_one(state, qh, paint);
    }
}

fn sync_one(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, which: Arrow, paint: Option<&BubblePaint>) {
    let present = slot(state, which).is_some();
    match paint {
        None if present => destroy(state, which),
        None => {}
        Some(paint) if !present => {
            let _ = spawn(state, qh, paint);
        }
        Some(paint) => place(state, qh, paint),
    }
}

fn paint_one(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: &BubblePaint) {
    let stamp = format!("{}:{}:{}", paint.hover as u8, paint.origin_x, paint.origin_y);
    let ready = slot(state, paint.which).is_some_and(|slot| slot.configured);
    if !ready {
        return;
    }
    let same = slot(state, paint.which).is_some_and(|slot| slot.stamp == stamp && slot.buffer.is_some());
    if same {
        return;
    }
    let busy = slot(state, paint.which).is_some_and(|slot| slot.buffer.is_some() && !slot.free);
    if busy {
        return;
    }
    let Some(shm) = state.shm.clone() else { return };
    if ensure_buffer(state, qh, &shm, paint.which).is_err() {
        return;
    }
    {
        let Some(slot) = slot_mut(state, paint.which) else { return };
        if let Some(map) = slot.map.as_mut() {
            paint_bubble(map.bytes(), paint.diameter, paint.hover, paint.which);
        }
        slot.stamp = stamp;
        slot.free = false;
    }
    let Some(slot) = slot(state, paint.which) else { return };
    let Some(buffer) = slot.buffer.as_ref() else { return };
    slot.surface.attach(Some(buffer), 0, 0);
    slot.surface.damage_buffer(0, 0, paint.diameter.max(1), paint.diameter.max(1));
    slot.surface.commit();
}

fn spawn(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: &BubblePaint) -> Result<(), String> {
    let compositor = state.compositor.clone().ok_or("arrow: no compositor")?;
    let shell = state.layer_shell.clone().ok_or("arrow: no layer shell")?;
    let output = state.output.clone();
    let surface = compositor.create_surface(qh, ());
    set_input(&compositor, qh, &surface, paint.diameter);
    let layer = shell.get_layer_surface(
        &surface,
        output.as_ref(),
        zwlr_layer_shell_v1::Layer::Top,
        namespace(paint.which),
        qh,
        paint.which,
    );
    layer.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    layer.set_size(paint.diameter.max(1) as u32, paint.diameter.max(1) as u32);
    layer.set_margin(paint.origin_y, 0, 0, paint.origin_x);
    surface.commit();
    let slot = BubbleSlot {
        origin_x: paint.origin_x,
        origin_y: paint.origin_y,
        surface,
        layer,
        pool: None,
        buffer: None,
        map: None,
        fd: None,
        configured: false,
        free: true,
        stamp: String::new(),
    };
    *slot_mut_opt(state, paint.which) = Some(slot);
    Ok(())
}

fn place(state: &mut OverlayState, qh: &QueueHandle<OverlayState>, paint: &BubblePaint) {
    let compositor = state.compositor.clone();
    let surface = {
        let Some(slot) = slot_mut(state, paint.which) else { return };
        let moved = paint.origin_x != slot.origin_x || paint.origin_y != slot.origin_y;
        slot.origin_x = paint.origin_x;
        slot.origin_y = paint.origin_y;
        if moved {
            slot.layer.set_margin(paint.origin_y, 0, 0, paint.origin_x);
            slot.surface.commit();
        }
        slot.surface.clone()
    };
    if let Some(compositor) = compositor.as_ref() {
        set_input(compositor, qh, &surface, paint.diameter);
    }
}

fn destroy(state: &mut OverlayState, which: Arrow) {
    let Some(mut slot) = slot_mut_opt(state, which).take() else { return };
    release_buffer(&mut slot);
    slot.layer.destroy();
    slot.surface.destroy();
}

fn set_input(
    compositor: &wl_compositor::WlCompositor,
    qh: &QueueHandle<OverlayState>,
    surface: &wl_surface::WlSurface,
    diameter: i32,
) {
    let region = compositor.create_region(qh, ());
    let side = diameter.max(1);
    region.add(0, 0, side, side);
    surface.set_input_region(Some(&region));
    region.destroy();
}

fn ensure_buffer(
    state: &mut OverlayState,
    qh: &QueueHandle<OverlayState>,
    shm: &wl_shm::WlShm,
    which: Arrow,
) -> Result<(), String> {
    if slot(state, which).is_some_and(|slot| slot.buffer.is_some()) {
        return Ok(());
    }
    if let Some(slot) = slot_mut(state, which) {
        release_buffer(slot);
    }
    let side = BUBBLE_PX.max(1);
    let stride = side.saturating_mul(4);
    let size = stride.saturating_mul(side);
    let fd = crate::actors::ingress::screen_wlr::create_anonymous_shm(size as usize)?;
    let pool = shm.create_pool(fd.as_fd(), size, qh, ());
    let buffer = pool.create_buffer(0, side, side, stride, wl_shm::Format::Argb8888, qh, which);
    let map = ShmMap::writable(&fd, size as usize)?;
    let Some(slot) = slot_mut(state, which) else {
        return Err("arrow is gone".into());
    };
    slot.pool = Some(pool);
    slot.buffer = Some(buffer);
    slot.map = Some(map);
    slot.fd = Some(fd);
    slot.free = true;
    Ok(())
}

fn release_buffer(slot: &mut BubbleSlot) {
    if let Some(buffer) = slot.buffer.take() {
        buffer.destroy();
    }
    if let Some(pool) = slot.pool.take() {
        pool.destroy();
    }
    slot.map = None;
    slot.fd = None;
    slot.free = true;
}

fn namespace(which: Arrow) -> String {
    match which {
        Arrow::Back => "pachakutech-arrow-back".to_string(),
        Arrow::Forward => "pachakutech-arrow-forward".to_string(),
    }
}

fn slot(state: &OverlayState, which: Arrow) -> Option<&BubbleSlot> {
    match which {
        Arrow::Back => state.arrow_back.as_ref(),
        Arrow::Forward => state.arrow_forward.as_ref(),
    }
}

fn slot_mut(state: &mut OverlayState, which: Arrow) -> Option<&mut BubbleSlot> {
    match which {
        Arrow::Back => state.arrow_back.as_mut(),
        Arrow::Forward => state.arrow_forward.as_mut(),
    }
}

fn slot_mut_opt(state: &mut OverlayState, which: Arrow) -> &mut Option<BubbleSlot> {
    match which {
        Arrow::Back => &mut state.arrow_back,
        Arrow::Forward => &mut state.arrow_forward,
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, Arrow> for OverlayState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        data: &Arrow,
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(slot) = slot_mut(state, *data) else {
            if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
                layer_surface.ack_configure(serial);
            }
            return;
        };
        if &slot.layer != layer_surface {
            if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
                layer_surface.ack_configure(serial);
            }
            return;
        }
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                layer_surface.ack_configure(serial);
                slot.configured = true;
            }
            zwlr_layer_surface_v1::Event::Closed => {
                slot.configured = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, Arrow> for OverlayState {
    fn event(
        state: &mut Self,
        _proxy: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        data: &Arrow,
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            if let Some(slot) = slot_mut(state, *data) {
                slot.free = true;
            }
        }
    }
}

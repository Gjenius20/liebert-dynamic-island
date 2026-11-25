use std::ffi::c_void;
use std::io::{BufRead, BufReader};
use std::num::NonZeroU32;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::ptr::NonNull;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop::ping::make_ping;
use calloop_wayland_source::WaylandSource;
use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use glow::HasContext;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::{
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::calendar::{self, CalendarCommand};
use crate::config::Config;
use crate::lyrics;
use crate::platform::{self, linux};
use crate::tray::TrayController;
use crate::ui::{self, AppChannels, IslandApp, OverlayBackendMode, SETTINGS_WINDOW_TITLE};

const LAYER_NAMESPACE: &str = "liebert-dynamic-island";

enum SettingsIpcCmd {
    ConfigUpdated,
    TodosUpdated,
    TestNotification,
    CalendarSetUrl(String),
    CalendarRefresh,
}

struct WaylandState {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    layer: LayerSurface,
    empty_region: Option<Region>,
    pointer: Option<wl_pointer::WlPointer>,
    configured: bool,
    exit: bool,
    needs_redraw: bool,
    scale_factor: i32,
    logical_width: u32,
    logical_height: u32,
    pointer_inside: bool,
    last_pointer_pos: Pos2,
    pending_events: Vec<Event>,
}

impl CompositorHandler for WaylandState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        if self.layer.wl_surface() == surface {
            let next = new_factor.max(1);
            if next != self.scale_factor {
                self.scale_factor = next;
                self.layer.wl_surface().set_buffer_scale(next);
                self.needs_redraw = true;
            }
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.needs_redraw = true;
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if self.layer.wl_surface() == surface
            && let Some(info) = self.output_state.info(output)
        {
            let next = info.scale_factor.max(1);
            if next != self.scale_factor {
                self.scale_factor = next;
                self.layer.wl_surface().set_buffer_scale(next);
                self.needs_redraw = true;
            }
        }
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for WaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for WaylandState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if configure.new_size.0 > 0 {
            self.logical_width = configure.new_size.0;
        }
        if configure.new_size.1 > 0 {
            self.logical_height = configure.new_size.1;
        }
        self.configured = true;
        self.needs_redraw = true;
    }
}

impl SeatHandler for WaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && self.pointer.is_none()
            && let Ok(pointer) = self.seat_state.get_pointer(qh, &seat)
        {
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for WaylandState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            let pos = Pos2::new(event.position.0 as f32, event.position.1 as f32);
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    self.pointer_inside = true;
                    self.last_pointer_pos = pos;
                    self.pending_events.push(Event::PointerMoved(pos));
                    self.needs_redraw = true;
                }
                PointerEventKind::Leave { .. } => {
                    self.pointer_inside = false;
                    self.pending_events.push(Event::PointerGone);
                    self.needs_redraw = true;
                }
                PointerEventKind::Motion { .. } => {
                    self.pointer_inside = true;
                    self.last_pointer_pos = pos;
                    self.pending_events.push(Event::PointerMoved(pos));
                    self.needs_redraw = true;
                }
                PointerEventKind::Press { button, .. } => {
                    if let Some(btn) = map_wayland_button(button) {
                        self.last_pointer_pos = pos;
                        self.pending_events.push(Event::PointerButton {
                            pos,
                            button: btn,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        });
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if let Some(btn) = map_wayland_button(button) {
                        self.last_pointer_pos = pos;
                        self.pending_events.push(Event::PointerButton {
                            pos,
                            button: btn,
                            pressed: false,
                            modifiers: Modifiers::NONE,
                        });
                        self.needs_redraw = true;
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let dx = if horizontal.absolute.abs() > 0.0 {
                        -(horizontal.absolute as f32)
                    } else {
                        -(horizontal.discrete as f32) * 20.0
                    };
                    let dy = if vertical.absolute.abs() > 0.0 {
                        -(vertical.absolute as f32)
                    } else {
                        -(vertical.discrete as f32) * 20.0
                    };
                    if dx != 0.0 || dy != 0.0 {
                        self.pending_events.push(Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: Vec2::new(dx, dy),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: Modifiers::NONE,
                        });
                        self.needs_redraw = true;
                    }
                }
            }
        }
    }
}

impl ProvidesRegistryState for WaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(WaylandState);
delegate_output!(WaylandState);
delegate_layer!(WaylandState);
delegate_seat!(WaylandState);
delegate_pointer!(WaylandState);
delegate_registry!(WaylandState);

fn map_wayland_button(button: u32) -> Option<PointerButton> {
    match button {
        BTN_LEFT => Some(PointerButton::Primary),
        BTN_RIGHT => Some(PointerButton::Secondary),
        BTN_MIDDLE => Some(PointerButton::Middle),
        _ => None,
    }
}

fn spawn_settings_ipc_listener(
    tx: mpsc::Sender<SettingsIpcCmd>,
    ping: calloop::ping::Ping,
) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let sock_path = runtime.join(format!("liebert-settings-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path).ok()?;
    std::thread::Builder::new()
        .name("settings-ipc-listener".to_owned())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let reader = BufReader::new(stream);
                for line in reader.lines().map_while(Result::ok) {
                    let cmd = match line.as_str() {
                        "config" => Some(SettingsIpcCmd::ConfigUpdated),
                        "todos" => Some(SettingsIpcCmd::TodosUpdated),
                        "test_notif" => Some(SettingsIpcCmd::TestNotification),
                        "cal_refresh" => Some(SettingsIpcCmd::CalendarRefresh),
                        other => other
                            .strip_prefix("cal_url:")
                            .map(|url| SettingsIpcCmd::CalendarSetUrl(url.to_owned())),
                    };
                    if let Some(cmd) = cmd {
                        let _ = tx.send(cmd);
                        ping.ping();
                    }
                }
            }
        })
        .ok()?;
    Some(sock_path)
}

fn launch_or_focus_settings(child: &mut Option<Child>, ipc_sock: Option<&Path>) {
    if let Some(proc) = child.as_mut() {
        match proc.try_wait() {
            Ok(None) => {
                let _ = linux::bring_window_to_front(SETTINGS_WINDOW_TITLE);
                return;
            }
            Ok(Some(_)) | Err(_) => {
                *child = None;
            }
        }
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg("--settings");
    if let Some(sock) = ipc_sock {
        cmd.arg("--ipc-sock").arg(sock);
    }
    if let Ok(spawned) = cmd.spawn() {
        *child = Some(spawned);
    }
}

struct SocketCleanup(Option<PathBuf>, Option<Child>);

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if let Some(mut child) = self.1.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn try_run_layer_shell(
    config: Config,
    tray: Option<TrayController>,
    demo_mode: bool,
    initial_w: f32,
    initial_h: f32,
    initial_y: f32,
) -> Result<(), String> {
    let conn = Connection::connect_to_env()
        .map_err(|e| format!("Wayland connection unavailable: {e}"))?;
    let (globals, mut event_queue) =
        registry_queue_init(&conn).map_err(|e| format!("Wayland registry init failed: {e}"))?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| format!("wl_compositor unavailable: {e}"))?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|e| format!("wlr-layer-shell unavailable: {e}"))?;

    linux::prepare_layer_shell_rules(LAYER_NAMESPACE);

    let init_w = initial_w.round().max(1.0) as u32;
    let init_h = initial_h.round().max(1.0) as u32;
    let init_margin_top = initial_y.round() as i32;

    let wl_surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        wl_surface,
        Layer::Overlay,
        Some(LAYER_NAMESPACE),
        None,
    );
    layer.set_anchor(Anchor::TOP);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_margin(init_margin_top, 0, 0, 0);
    layer.set_size(init_w, init_h);
    layer.commit();

    let empty_region = Region::new(&compositor).ok();

    let mut wl_state = WaylandState {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        layer,
        empty_region,
        pointer: None,
        configured: false,
        exit: false,
        needs_redraw: true,
        scale_factor: 1,
        logical_width: init_w,
        logical_height: init_h,
        pointer_inside: false,
        last_pointer_pos: Pos2::ZERO,
        pending_events: Vec::new(),
    };

    while !wl_state.configured && !wl_state.exit {
        event_queue
            .blocking_dispatch(&mut wl_state)
            .map_err(|e| format!("Wayland initial configure failed: {e}"))?;
    }
    if wl_state.exit {
        return Ok(());
    }

    let display_ptr = NonNull::new(conn.backend().display_ptr() as *mut c_void)
        .ok_or_else(|| "Null Wayland display pointer".to_owned())?;
    let raw_display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display_ptr));
    let gl_display = unsafe { Display::new(raw_display, DisplayApiPreference::Egl) }
        .map_err(|e| format!("EGL display init failed: {e}"))?;

    let template = ConfigTemplateBuilder::new()
        .with_transparency(true)
        .with_alpha_size(8)
        .build();
    let gl_config = unsafe { gl_display.find_configs(template) }
        .map_err(|e| format!("EGL config lookup failed: {e}"))?
        .next()
        .ok_or_else(|| "No transparent EGL config found".to_owned())?;

    let surface_ptr = NonNull::new(wl_state.layer.wl_surface().id().as_ptr() as *mut c_void)
        .ok_or_else(|| "Null wl_surface pointer".to_owned())?;
    let raw_window = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface_ptr));

    let ctx_attrs = ContextAttributesBuilder::new().build(Some(raw_window));
    let not_current_ctx = unsafe { gl_display.create_context(&gl_config, &ctx_attrs) }
        .map_err(|e| format!("EGL context creation failed: {e}"))?;

    let scale = wl_state.scale_factor.max(1) as u32;
    let fb_w = NonZeroU32::new((wl_state.logical_width * scale).max(1)).unwrap();
    let fb_h = NonZeroU32::new((wl_state.logical_height * scale).max(1)).unwrap();
    let surf_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window, fb_w, fb_h);
    let gl_surface: Surface<WindowSurface> =
        unsafe { gl_display.create_window_surface(&gl_config, &surf_attrs) }
            .map_err(|e| format!("EGL window surface creation failed: {e}"))?;

    let gl_context: PossiblyCurrentContext = not_current_ctx
        .make_current(&gl_surface)
        .map_err(|e| format!("EGL make_current failed: {e}"))?;
    let _ = gl_surface.set_swap_interval(&gl_context, SwapInterval::DontWait);

    let gl = Arc::new(unsafe {
        glow::Context::from_loader_function_cstr(|name| gl_display.get_proc_address(name))
    });
    let mut painter = egui_glow::Painter::new(gl, "", None, false)
        .map_err(|e| format!("egui_glow Painter init failed: {e}"))?;

    let egui_ctx = egui::Context::default();
    ui::install_base_fallback_fonts(&egui_ctx);

    let mut event_loop: EventLoop<WaylandState> =
        EventLoop::try_new().map_err(|e| format!("calloop init failed: {e}"))?;
    let loop_handle = event_loop.handle();

    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .map_err(|e| format!("WaylandSource insert failed: {e}"))?;

    let (ping, ping_source) = make_ping().map_err(|e| format!("calloop ping failed: {e}"))?;
    loop_handle
        .insert_source(ping_source, |_, _, state| {
            state.needs_redraw = true;
        })
        .map_err(|e| format!("Ping source insert failed: {e}"))?;

    let (event_tx, event_rx) = mpsc::channel();
    let (lyrics_req_tx, lyrics_req_rx) = mpsc::channel();
    let (media_cmd_tx, media_cmd_rx) = mpsc::channel();
    let (calendar_cmd_tx, calendar_cmd_rx) = mpsc::channel();

    let wake_ping = ping.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || wake_ping.ping());

    platform::spawn_listener(event_tx.clone(), wake.clone());
    platform::spawn_media_listener(event_tx.clone(), media_cmd_rx, wake.clone());
    lyrics::spawn_lyrics_worker(lyrics_req_rx, event_tx.clone(), wake.clone());
    calendar::spawn_calendar_worker(
        config.google_calendar_ics_url.clone(),
        calendar_cmd_rx,
        event_tx,
        wake,
    );

    let (settings_tx, settings_rx) = mpsc::channel();
    let ipc_sock_path = spawn_settings_ipc_listener(settings_tx, ping);
    let mut cleanup = SocketCleanup(ipc_sock_path.clone(), None);

    let mut app = IslandApp::new(
        config,
        AppChannels {
            event_rx,
            lyrics_req_tx,
            media_cmd_tx,
            calendar_cmd_tx,
        },
        tray,
        demo_mode,
    );

    let start_time = Instant::now();
    let mut applied_w = init_w;
    let mut applied_h = init_h;
    let mut applied_margin_top = init_margin_top;
    let mut applied_visible = true;
    let mut applied_scale = wl_state.scale_factor.max(1);

    loop {
        while let Ok(cmd) = settings_rx.try_recv() {
            match cmd {
                SettingsIpcCmd::ConfigUpdated => {
                    app.apply_external_config(Config::load(), &egui_ctx);
                }
                SettingsIpcCmd::TodosUpdated => {
                    app.reload_todos(&egui_ctx);
                }
                SettingsIpcCmd::TestNotification => {
                    app.trigger_test_notification(&egui_ctx);
                }
                SettingsIpcCmd::CalendarSetUrl(url) => {
                    app.send_calendar_command(CalendarCommand::SetUrl(url));
                }
                SettingsIpcCmd::CalendarRefresh => {
                    app.send_calendar_command(CalendarCommand::Refresh);
                }
            }
            wl_state.needs_redraw = true;
        }

        let step = app.step_logic(
            &egui_ctx,
            OverlayBackendMode::WaylandLayerShell,
            wl_state.pointer_inside,
        );

        if step.want_exit || wl_state.exit {
            break;
        }
        if step.open_settings_requested {
            launch_or_focus_settings(&mut cleanup.1, cleanup.0.as_deref());
        }

        let want_w = if step.visible {
            step.size.x.round().max(1.0) as u32
        } else {
            1
        };
        let want_h = if step.visible {
            step.size.y.round().max(1.0) as u32
        } else {
            1
        };
        let want_margin_top = step.top_y.round() as i32;
        let cur_scale = wl_state.scale_factor.max(1);

        let mut layer_dirty = false;
        if step.visible != applied_visible {
            applied_visible = step.visible;
            if step.visible {
                wl_state.layer.wl_surface().set_input_region(None);
            } else if let Some(region) = &wl_state.empty_region {
                wl_state
                    .layer
                    .wl_surface()
                    .set_input_region(Some(region.wl_region()));
                wl_state.pointer_inside = false;
            }
            layer_dirty = true;
        }
        if want_w != applied_w || want_h != applied_h {
            applied_w = want_w;
            applied_h = want_h;
            wl_state.logical_width = want_w;
            wl_state.logical_height = want_h;
            wl_state.layer.set_size(want_w, want_h);
            layer_dirty = true;
        }
        if want_margin_top != applied_margin_top {
            applied_margin_top = want_margin_top;
            wl_state.layer.set_margin(want_margin_top, 0, 0, 0);
            layer_dirty = true;
        }
        if cur_scale != applied_scale {
            applied_scale = cur_scale;
            layer_dirty = true;
        }
        if layer_dirty {
            let phys_w = NonZeroU32::new((want_w * cur_scale as u32).max(1)).unwrap();
            let phys_h = NonZeroU32::new((want_h * cur_scale as u32).max(1)).unwrap();
            wl_state.layer.wl_surface().set_buffer_scale(cur_scale);
            gl_surface.resize(&gl_context, phys_w, phys_h);
            wl_state.layer.commit();
            wl_state.needs_redraw = true;
        }

        let phys_w_px = (applied_w * cur_scale as u32).max(1);
        let phys_h_px = (applied_h * cur_scale as u32).max(1);
        let ppp = cur_scale as f32;

        let raw_input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(applied_w as f32, applied_h as f32),
            )),
            time: Some(start_time.elapsed().as_secs_f64()),
            events: std::mem::take(&mut wl_state.pending_events),
            ..Default::default()
        };

        egui_ctx.set_pixels_per_point(ppp);
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            if step.visible {
                app.render_island_ui(ui, false);
            }
        });

        let clipped_primitives = egui_ctx.tessellate(full_output.shapes, ppp);
        unsafe {
            painter
                .gl()
                .viewport(0, 0, phys_w_px as i32, phys_h_px as i32);
        }
        painter.clear([phys_w_px, phys_h_px], [0.0, 0.0, 0.0, 0.0]);
        if step.visible {
            painter.paint_and_update_textures(
                [phys_w_px, phys_h_px],
                ppp,
                &clipped_primitives,
                &mut full_output.textures_delta,
            );
        }
        let _ = gl_surface.swap_buffers(&gl_context);
        let _ = conn.flush();
        wl_state.needs_redraw = false;

        let wait = if step.repaint_after.is_zero() {
            Duration::from_millis(6)
        } else {
            step.repaint_after.min(Duration::from_millis(80))
        };
        if event_loop.dispatch(Some(wait), &mut wl_state).is_err() {
            break;
        }
    }

    painter.destroy();
    Ok(())
}

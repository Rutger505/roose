mod args;
mod brain;
mod desktop;
mod geometry;
mod hypr;
mod sprite;

use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_registry,
    dispatch2::Dispatch2,
    output::{OutputHandler, OutputInfo, OutputState},
    reexports::{
        calloop::{
            EventLoop,
            timer::{TimeoutAction, Timer},
        },
        calloop_wayland_source::WaylandSource,
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, raw::RawPool},
    subcompositor::SubcompositorState,
};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_buffer, wl_output, wl_shm, wl_subsurface, wl_surface},
};

use crate::{
    args::Args,
    brain::{Facing, Goose, Prop, Settings},
    desktop::{Commands, HyprDesktop},
    geometry::Vec2,
    hypr::Hyprland,
    sprite::Source,
};

/// The goose never changes its look, so both facings are rendered once into shared memory
/// and moving only repositions a subsurface: no redraws, no per-frame allocations.
struct Sprites {
    right: wl_buffer::WlBuffer,
    left: wl_buffer::WlBuffer,
    _pool: RawPool,
}

impl Drop for Sprites {
    fn drop(&mut self) {
        self.right.destroy();
        self.left.destroy();
    }
}

struct SpriteBuffer;

impl Dispatch2<wl_buffer::WlBuffer, App> for SpriteBuffer {
    fn event(
        &self,
        _: &mut App,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
    }
}

/// Moving a layer surface (via margins) makes compositors re-arrange and reconfigure every layer
/// surface on the monitor, so N geese would cost O(N²) wakeups. Instead the layer surface is a
/// static transparent 1x1 anchor and the goose is a subsurface that is moved with set_position.
struct Body {
    layer: LayerSurface,
    anchor: wl_buffer::WlBuffer,
    _anchor_pool: RawPool,
    subsurface: wl_subsurface::WlSubsurface,
    surface: wl_surface::WlSurface,
}

struct App {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    source: Source,
    size: (u32, u32),
    body: Option<Body>,
    sprites: Option<Sprites>,
    scale: i32,
    shown: Option<Facing>,
    configured: bool,
    closed: bool,
}

impl App {
    fn build_sprites(&mut self, qh: &QueueHandle<Self>) {
        let (width, height) = (
            self.size.0 * self.scale as u32,
            self.size.1 * self.scale as u32,
        );
        let frame = (width * height * 4) as usize;
        let mut pool =
            RawPool::new(frame * 2, &self.shm).expect("could not allocate shared memory");
        let pixmap = self.source.render(width, height);
        sprite::write_argb(&pixmap, false, &mut pool.mmap()[..frame]);
        sprite::write_argb(&pixmap, true, &mut pool.mmap()[frame..]);
        let (w, h, stride) = (width as i32, height as i32, width as i32 * 4);
        let format = wl_shm::Format::Argb8888;
        let right = pool.create_buffer(0, w, h, stride, format, SpriteBuffer, qh);
        let left = pool.create_buffer(frame as i32, w, h, stride, format, SpriteBuffer, qh);
        self.sprites = Some(Sprites {
            right,
            left,
            _pool: pool,
        });
        self.shown = None;
    }

    fn present(&mut self, position: Vec2, facing: Facing) {
        let (Some(body), Some(sprites), true) = (&self.body, &self.sprites, self.configured) else {
            return;
        };
        body.subsurface
            .set_position(position.x.round() as i32, position.y.round() as i32);
        if self.shown != Some(facing) {
            let buffer = if facing == Facing::Right {
                &sprites.right
            } else {
                &sprites.left
            };
            body.surface.set_buffer_scale(self.scale);
            body.surface.attach(Some(buffer), 0, 0);
            body.surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
            self.shown = Some(facing);
        }
        // Hyprland only damages a subsurface's previous area when the subsurface itself commits.
        body.surface.commit();
        body.layer.commit();
    }
}

fn logical_geometry(info: &OutputInfo) -> (Vec2, Vec2) {
    let size = info.logical_size.unwrap_or_else(|| {
        let mode = info
            .modes
            .iter()
            .find(|m| m.current)
            .map_or((1920, 1080), |m| m.dimensions);
        (
            mode.0 / info.scale_factor.max(1),
            mode.1 / info.scale_factor.max(1),
        )
    });
    let origin = info.logical_position.unwrap_or(info.location);
    (
        Vec2::new(origin.0 as f32, origin.1 as f32),
        Vec2::new(size.0 as f32, size.1 as f32),
    )
}

fn main() {
    let args = Args::parse().unwrap_or_else(|err| {
        eprintln!("roose: {err}");
        std::process::exit(2);
    });
    if let Err(err) = run(args) {
        eprintln!("roose: {err}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let source = Source::load(args.sprite.as_deref())?;
    let size = (
        args.size.round() as u32,
        (args.size * source.aspect()).round().max(1.0) as u32,
    );

    let hypr = Hyprland::from_env()?;
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<App>(&conn)?;
    let qh = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell =
        LayerShell::bind(&globals, &qh).map_err(|_| "compositor has no wlr-layer-shell")?;

    let mut app = App {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm: Shm::bind(&globals, &qh)?,
        source,
        size,
        body: None,
        sprites: None,
        scale: 1,
        shown: None,
        configured: false,
        closed: false,
    };
    event_queue.roundtrip(&mut app)?;
    event_queue.roundtrip(&mut app)?;

    let wanted = hypr.focused_monitor();
    let outputs: Vec<(wl_output::WlOutput, OutputInfo)> = app
        .output_state
        .outputs()
        .filter_map(|o| app.output_state.info(&o).map(|info| (o, info)))
        .collect();
    let (output, info) = outputs
        .iter()
        .find(|(_, info)| wanted.is_some() && info.name == wanted)
        .or(outputs.first())
        .ok_or("no outputs found")?;
    let (origin, screen) = logical_geometry(info);
    app.scale = info.scale_factor.max(1);

    let layer = layer_shell.create_layer_surface(
        &qh,
        compositor.create_surface(&qh),
        Layer::Overlay,
        Some("roose"),
        Some(output),
    );
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_size(1, 1);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    let click_through = Region::new(&compositor)?;
    layer
        .wl_surface()
        .set_input_region(Some(click_through.wl_region()));
    layer.commit();

    let mut anchor_pool = RawPool::new(4, &app.shm)?;
    anchor_pool.mmap().fill(0);
    let anchor = anchor_pool.create_buffer(0, 1, 1, 4, wl_shm::Format::Argb8888, SpriteBuffer, &qh);

    let subcompositor =
        SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh)?;
    let (subsurface, surface) = subcompositor.create_subsurface(layer.wl_surface().clone(), &qh);
    surface.set_input_region(Some(click_through.wl_region()));
    app.body = Some(Body {
        layer,
        anchor,
        _anchor_pool: anchor_pool,
        subsurface,
        surface,
    });
    app.build_sprites(&qh);

    let mut props = Vec::new();
    if args.notes {
        props.push(Prop::Note);
    }
    if args.meme_props {
        props.push(Prop::Meme);
    }
    let settings = Settings {
        size: Vec2::new(size.0 as f32, size.1 as f32),
        speed: args.speed,
        frame: 1.0 / args.fps,
        steal_cursor: args.steal,
        props,
        prop_cooldown: args.cooldown,
    };
    let commands = Commands {
        editor: args.editor,
        viewer: args.viewer,
        memes: args.memes,
    };
    let mut desktop = HyprDesktop::new(hypr, origin, commands, fastrand::Rng::new());
    let mut goose = Goose::new(settings, screen, fastrand::Rng::new());

    let mut event_loop = EventLoop::<App>::try_new()?;
    WaylandSource::new(conn.clone(), event_queue).insert(event_loop.handle())?;
    let frame = 1.0 / args.fps;
    let mut last = Instant::now();
    let mut last_position = None;
    event_loop
        .handle()
        .insert_source(Timer::immediate(), move |_, _, app| {
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f32();
            last = now;
            let mut sleep = goose.tick(dt, &mut desktop);
            let position = goose.draw_position();
            app.present(position, goose.facing());
            // One more still frame after moving so the compositor damages the final old spot.
            if last_position.replace(position) != Some(position) {
                sleep = sleep.min(frame);
            }
            TimeoutAction::ToDuration(Duration::from_secs_f32(sleep))
        })?;

    while !app.closed {
        event_loop.dispatch(None, &mut app)?;
    }
    Ok(())
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if factor != self.scale && factor > 0 {
            self.scale = factor;
            self.build_sprites(qh);
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        if let (Some(body), false) = (&self.body, self.configured) {
            body.layer.wl_surface().attach(Some(&body.anchor), 0, 0);
            body.layer.commit();
        }
        self.configured = true;
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_registry!(App);
smithay_client_toolkit::delegate_dispatch2!(App);

//! Zakuro's frontend, a window showing the game library or a game w/ a menu over it,
//! plus the loop that drives the emulated console one frame at a time

mod audio;
mod cli;
mod gamepad;
mod gui;
mod imgui_ui;
mod input;
mod menus;
mod present;
mod pro_controller;
mod settings;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use zakuro_common::Screen;
use zakuro_core::{loader, Config, FrameOutcome, System};
use zakuro_gpu::{layout, GpuScreen, Overlay, PresentError, RendererKind, ScreenImage};

use gui::Gui;
use input::Keyboard;
use menus::{Action, Menus};
use present::Backend;
use settings::{Renderer, Screens, Settings};

pub use zakuro_core::memory::Memory;
pub use zakuro_core::recompiled::Linked;

use crate::settings::Orientation;

/// what a program built on Zakuro shows over its game, worked out from the game's memory,
/// a line kept in a corner and a notice shown for a moment
/// a recompiled game can show what the game keeps hidden this way (its alignment, a timer) w/o drawing into the game's own screens
pub trait GameOverlay {
    /// called after each frame the game runs, w/ its memory
    fn frame(&mut self, memory: &mut Memory) -> OverlayText;
}

/// what a GameOverlay shows after a frame
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct OverlayText {
    /// a line in the bottom right corner while it's some
    pub corner: Option<String>,
    /// a notice at the top for a moment, like a save state's
    pub notice: Option<String>,
}

/// runs the emulator like the command line says, on recompiled code linked into the program when there's some
pub fn run(linked: Option<Linked>) {
    run_with(linked, None)
}

/// runs the emulator like run does, w/ an overlay over the game
pub fn run_with(linked: Option<Linked>, overlay: Option<Box<dyn GameOverlay>>) {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();

    let options = match cli::parse() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("zakuro: {message}");
            std::process::exit(2);
        }
    };
    let mut settings = Settings::load();
    // the console's own number, made the first time and kept, it tells this one apart from others over local wireless
    if settings.console_id == 0 {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_nanos()));
        settings.console_id = hasher.finish() | 1;
        settings.save();
    }

    let data_dir = options.data.clone().map(PathBuf::from).or_else(default_data_dir);
    if let Some(dir) = &data_dir {
        bring_saves(dir);
    }

    let mut app = App {
        keyboard: Keyboard::new(settings.keys.clone()),
        gamepads: gamepad::Gamepads::new(settings.pad.clone()),
        scale: options.scale.unwrap_or(settings.scale).max(1),
        layout: settings.layout,
        orientation: settings.orientation,
        top_shape: settings.top_shape,
        options,
        active: settings.clone(),
        settings,
        linked,
        overlay,
        overlay_corner: None,
        data_dir,
        game: None,
        menus: {
            let mut menus = Menus::default();
            menus.slot = 1;
            menus
        },
        gui: None,
        imgui: None,
        backend: None,
        window: None,
        audio: None,
        mouse_down: false,
        tilting_from: None,
        cursor: (0.0, 0.0),
        last_title_update: Instant::now(),
        next_frame: Instant::now(),
        skipped: 0,
        shown: 0,
        spent: Spent::default(),
        paused: false,
        fast_forward_key: false,
        fast_forwarding: false,
        next_shown: Instant::now(),
        stop: false,
        maximized_before_full_screen: false,
        resize_after_full_screen: false,
    };

    if let Some(frames) = app.options.headless {
        app.start();
        if let Some(game) = &mut app.game {
            run_headless(&mut game.system, frames);
        }
        return;
    }

    app.audio = if app.options.mute {
        None
    } else {
        audio::Audio::open(zakuro_core::AUDIO_SAMPLE_RATE).inspect_err(|error| log::warn!("no sound, {error}")).ok()
    };
    app.apply_volume();

    let event_loop = EventLoop::new().expect("create an event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("zakuro: {error}");
    }
}

/// the system's place for a program's data
fn default_data_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let dir = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support"))
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local/share")))
    };
    dir.map(|dir| dir.join("zakuro"))
}

/// how long ago a state was saved, like a person would say it
fn ago(time: std::time::SystemTime) -> String {
    let seconds = time.elapsed().map_or(0, |elapsed| elapsed.as_secs());
    match seconds {
        0..60 => "saved just now".into(),
        60..120 => "saved a minute ago".into(),
        120..3600 => format!("saved {} minutes ago", seconds / 60),
        3600..7200 => "saved an hour ago".into(),
        7200..86400 => format!("saved {} hours ago", seconds / 3600),
        86400..172800 => "saved yesterday".into(),
        _ => format!("saved {} days ago", seconds / 86400),
    }
}

/// saves Zakuro kept in the cwd b4 it had a place for them, copied there the first time, the old ones stay as they were
fn bring_saves(data: &Path) {
    let (old, new) = (Path::new("user"), data.join("user"));
    if new.exists() || !old.is_dir() {
        return;
    }
    // a copy cut short is left under another name, and tried again
    let partial = data.join("user.copying");
    let copied = std::fs::remove_dir_all(&partial)
        .or_else(|error| if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) })
        .and_then(|()| copy_dir(old, &partial))
        .and_then(|()| std::fs::rename(&partial, &new));
    match copied {
        Ok(()) => log::info!("copied the saves in ./user to {}, the old ones stay as they were", new.display()),
        Err(error) => log::warn!("could not copy the saves in ./user to {}, {error}", new.display()),
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// builds a System w/ no title loaded and both screens filled w/ a solid color,
/// to check the presentation path (framebuffer config => guest memory => window) apart from any title's boot
/// logic, and that the GPU starts, a packed release gets tried w/ it
fn paint_test_pattern(config: Config) -> System {
    use zakuro_core::memory::{MemoryRegion, MemoryState, Permission};

    let mut system = System::new(config);
    if system.config.hardware_renderer {
        match system.gpu.enable_hardware_renderer(system.config.resolution, system.config.device.clone(), None) {
            Ok(name) => log::info!("the 3D draws on {name}"),
            Err(error) => log::warn!("the 3D can't draw on the GPU, {error}"),
        }
    }

    // ABGR in memory for format 0 (Rgba8), red = 00 00 00 FF?
    let paint = |system: &mut System, screen: Screen, rgba: [u8; 4]| {
        let width = screen.width();
        let height = screen.height();
        let stride = height * 4;
        let size = stride * width;

        let block = system
            .memory
            .phys
            .allocate(MemoryRegion::Base, size)
            .expect("allocate test pattern framebuffer");

        let vaddr = zakuro_core::services::gsp::physical_to_virtual(system, block.addr);
        system.memory.map(
            vaddr,
            block.addr,
            size,
            Permission::READ | Permission::WRITE,
            MemoryState::Shared,
        );

        let mut pixel = [0u8; 4];
        pixel[3] = rgba[0]; // r
        pixel[2] = rgba[1]; // g
        pixel[1] = rgba[2]; // b
        pixel[0] = rgba[3]; // a
        let mut data = Vec::with_capacity(size as usize);
        for _ in 0..(size / 4) {
            data.extend_from_slice(&pixel);
        }
        system.memory.write_physical(block.addr, &data);

        let index = match screen {
            Screen::Top => 0,
            Screen::Bottom => 1,
        };
        system.set_framebuffer(index, 0, block.addr, block.addr, stride, 0);
    };

    paint(&mut system, Screen::Top, [220, 40, 40, 255]); // red
    paint(&mut system, Screen::Bottom, [40, 90, 220, 255]); // blue
    system
}

fn run_headless(system: &mut System, frames: u64) {
    let start = Instant::now();
    let mut outcome = FrameOutcome::Completed;
    let mut ran = 0;
    for _ in 0..frames {
        outcome = system.run_frame();
        ran += 1;
        if outcome != FrameOutcome::Completed {
            break;
        }
    }
    let elapsed = start.elapsed();
    println!("{outcome:?} after {ran} frames in {elapsed:.2?}");
    println!(
        "{:.1} MIPS, {}",
        system.cpu.cycles as f64 / elapsed.as_secs_f64() / 1e6,
        system.status_line()
    );
    if !system.fatal_errors.is_empty() {
        println!("fatal errors: {}", system.fatal_errors.join("; "));
    }
    for (thread, pc, hits) in system.hot_spots(10) {
        println!("  hot {thread:<8} 0x{pc:08X} {hits}");
    }
}

/// a game being played
struct Running {
    system: System,
    path: PathBuf,
    name: String,
    /// frames run since counting_since, for the frame rate
    frames: u32,
    counting_since: Instant,
    fps: f32,
}

impl Running {
    fn new(system: System, path: PathBuf, name: String) -> Running {
        Running {
            system,
            path,
            name,
            frames: 0,
            counting_since: Instant::now(),
            fps: 0.0,
        }
    }

    fn count_frame(&mut self) {
        self.frames += 1;
        let elapsed = self.counting_since.elapsed();
        if elapsed >= Duration::from_millis(500) {
            self.fps = self.frames as f32 / elapsed.as_secs_f32();
            self.frames = 0;
            self.counting_since = Instant::now();
        }
    }
}

struct App {
    options: cli::Options,
    /// the settings as the user left them, those get saved
    settings: Settings,
    /// the settings in force, what the running game has of its own over the general ones, which is all there is w/o a game
    active: Settings,
    linked: Option<Linked>,
    /// what the program built on Zakuro shows over the game, and the line it keeps in the corner now
    overlay: Option<Box<dyn GameOverlay>>,
    overlay_corner: Option<String>,
    data_dir: Option<PathBuf>,
    game: Option<Running>,
    menus: Menus,
    gui: Option<Gui>,
    imgui: Option<imgui_ui::ImGuiUi>,
    // backend must be declared (and so dropped) b4 window, Rust drops struct fields in declaration order,
    // and the GL surface's Drop calls eglDestroySurface,
    // which on Wayland does a protocol round trip against the window's wl_surface
    backend: Option<Backend>,
    window: Option<Window>,
    keyboard: Keyboard,
    gamepads: gamepad::Gamepads,
    audio: Option<audio::Audio>,
    /// the window's size, times the console's
    scale: u32,
    /// how the screens are arranged
    layout: Screens,
    orientation: Orientation,
    /// how wide the top screen shows
    top_shape: settings::TopShape,
    /// the left button is down, and where the pointer is, in window pixels
    mouse_down: bool,
    /// where the pointer was when the right button went down, it tilts the console while held
    tilting_from: Option<(f32, f32)>,
    cursor: (f32, f32),
    last_title_update: Instant,
    /// when the next frame is due
    /// frames run to a schedule instead of one after another,
    /// so a slow one gets made up by the ones after it and the sound (made as the console runs) keeps up
    next_frame: Instant,
    /// frames in a row not shown while catching up
    skipped: u32,
    /// frames shown since the title was last updated
    shown: u32,
    /// how long emulating and showing frames took since the title was last updated, for the frame rate's log
    spent: Spent,
    /// stopped w/ F1, w/o the menu
    paused: bool,
    /// Tab is held down, for fast forward
    fast_forward_key: bool,
    /// the game runs as fast as it can, its sound left out
    fast_forwarding: bool,
    /// when fast forward shows its next frame, on a grid at the console's frame rate
    next_shown: Instant,
    stop: bool,
    /// the window was maximized when it went full screen, leaving full screen brings that back
    maximized_before_full_screen: bool,
    /// the layout or the scale changed while the window was full screen, which kept its size,
    /// so leaving full screen fits the window to them
    resize_after_full_screen: bool,
}

/// how long each step of the loop took, its mean and its longest
#[derive(Default)]
struct Spent {
    emulating: Times,
    showing: Times,
}

#[derive(Default)]
struct Times {
    total: Duration,
    longest: Duration,
    count: u32,
}

impl Times {
    fn add(&mut self, time: Duration) {
        self.total += time;
        self.longest = self.longest.max(time);
        self.count += 1;
    }
}

impl std::fmt::Display for Times {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mean = self.total.as_secs_f64() * 1e3 / self.count.max(1) as f64;
        write!(f, "{mean:.1} ms, at most {:.1}", self.longest.as_secs_f64() * 1e3)
    }
}

/// one 3DS frame, about 16.71 ms
const FRAME_TIME: Duration =
    Duration::from_nanos(zakuro_core::CYCLES_PER_FRAME * 1_000_000_000 / zakuro_core::kernel::thread::CPU_CLOCK_HZ);
/// how far behind the schedule may fall b4 it starts over instead of running fast to catch up, after a pause say
const CATCH_UP_LIMIT: Duration = Duration::from_millis(200);
/// frames that may go unshown in a row while catching up
const MAX_SKIPPED: u32 = 4;

/// does a frame go unshown, the game running behind the schedule w/ a presenter that waits for the display,
/// and fewer than MAX_SKIPPED unshown b4 it
fn skips_showing(playing: bool, behind: bool, waits: bool, skipped: u32) -> bool {
    playing && behind && waits && skipped < MAX_SKIPPED
}

/// what the player is told when the 3D can't be drawn on the GPU
const GPU_FALLBACK: &str = "The 3D can't be drawn on the GPU, as Vulkan isn't working on this computer, so it's drawn in software. \
Games run much slower that way, and texture packs are left out. Updating the graphics driver usually fixes it; the log says \
what went wrong. To draw in software without this message, turn off \"Draw the 3D on the GPU\" in Settings, Graphics.";

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let size = self.window_size();
        let attributes = Window::default_attributes().with_title("Zakuro").with_inner_size(size);
        let renderer = self.options.renderer.unwrap_or(match self.settings.renderer {
            Renderer::Vulkan => RendererKind::Vulkan,
            Renderer::OpenGl => RendererKind::OpenGl,
        });

        let backend = Backend::create(event_loop, attributes.clone(), renderer).or_else(|error| {
            if renderer != RendererKind::Vulkan {
                return Err(error);
            }
            // a machine w/o a working Vulkan driver still gets a window
            log::warn!("could not start the Vulkan backend, {error}, presenting with OpenGL instead");
            Backend::create(event_loop, attributes, RendererKind::OpenGl)
        });
        match backend {
            Ok((window, mut backend)) => {
                log::info!("presenting with the {} backend", backend.name());
                backend.set_layout(self.layout.screens());
                backend.set_orientation(self.orientation.screen_orientation());
                backend.set_top_width(self.active.top_shape.width());
                backend.set_scaling(self.active.filter.screens(), self.active.integer_scale);
                backend.set_stereo(self.active.stereo());
                self.gui = Some(Gui::new(&window));
                self.imgui = Some(imgui_ui::ImGuiUi::new(&window));
                self.window = Some(window);
                self.backend = Some(backend);
                // the game the command line names starts on the device the presenter may lend the renderer, once there's one
                self.start();
            }
            Err(error) => {
                eprintln!("zakuro: could not start the {renderer:?} backend: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Tab is fast forward while a game runs, not a key for moving the UI's focus around
        let (tab, repeat) = match &event {
            WindowEvent::KeyboardInput { event, .. } => (event.physical_key == PhysicalKey::Code(KeyCode::Tab), event.repeat),
            _ => (false, false),
        };
        let in_game = self.game.as_ref().is_some_and(|game| game.system.keyboard_request().is_none())
            && !self.menus.over_game();
        // a Tab held down keeps repeating over a message too
        let withheld = tab && (in_game || (repeat && self.game.is_some()));
        let mut consumed = match (&mut self.gui, &self.window) {
            (Some(gui), Some(window)) if !withheld => gui.event(window, &event),
            _ => false,
        };
        if let (Some(imgui), Some(window)) = (&self.imgui, &self.window) {
            if self.menus.menu_open && !self.menus.settings_open && self.menus.game_settings.is_none() {
                consumed |= imgui.event(window, &event);
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // keys let go while another window has the keyboard never come
            WindowEvent::Focused(false) => {
                self.fast_forward_key = false;
                self.keyboard.release();
            }
            WindowEvent::Resized(size) => {
                if let Some(backend) = &mut self.backend {
                    backend.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(event, consumed),
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.mouse_down = state == ElementState::Pressed && !self.pointer_taken(consumed);
                    self.touch();
                }
                if button == MouseButton::Right {
                    let pressed = state == ElementState::Pressed && !self.pointer_taken(consumed);
                    self.tilting_from = pressed.then_some(self.cursor);
                    self.tilt();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                self.touch();
                self.tilt();
            }
            WindowEvent::RedrawRequested => self.step(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl App {
    /// the id of the running game, none w/o one or w/ the test pattern
    fn program_id(&self) -> Option<u64> {
        Some(self.game.as_ref()?.system.title.as_ref()?.program_id())
    }

    /// the console's settings for a game, from the command line and the settings it plays w/
    fn config(&self, settings: &Settings) -> Config {
        let interpret = self.options.interpreter || !settings.recompiled;
        Config {
            new3ds: self.options.new3ds,
            data_dir: self.data_dir.clone(),
            language: settings.language.code(),
            recompiled: self.options.recompiled.clone().filter(|_| !interpret).map(Into::into),
            linked: self.linked.filter(|_| !interpret),
            find_recompiled: !interpret,
            hardware_renderer: self.options.hardware_rasterizer.unwrap_or(settings.hardware_rasterizer),
            texture_packs: settings.texture_packs,
            interpolate: settings.frame_interpolation,
            enhancements: settings
                .enhancements
                .iter()
                .filter_map(|(id, names)| Some((u64::from_str_radix(id, 16).ok()?, names.clone())))
                .collect(),
            resolution: settings.resolution,
            device: self.backend.as_ref().and_then(Backend::shared_device),
            username: self.nickname(),
            console_id: settings.console_id,
            slider_3d: settings.slider_3d(),
            ..Config::default()
        }
    }

    /// starts what the command line asks for, the test pattern or a game
    fn start(&mut self) {
        if self.options.test_pattern {
            // the test pattern has no CPU program to run, paused keeps just presenting what's there
            let system = paint_test_pattern(self.config(&self.settings));
            self.game = Some(Running::new(system, PathBuf::new(), "test pattern".to_owned()));
            self.paused = true;
        } else if let Some(rom) = self.options.rom.clone() {
            if let Err(error) = self.play(Path::new(&rom)) {
                eprintln!("zakuro: {error}");
                std::process::exit(1);
            }
        }
        if self.options.profile {
            if let Some(game) = &mut self.game {
                game.system.enable_profiler();
            }
        }
    }

    /// starts the game at path in place of any other, w/ what it has of its own in the settings
    fn play(&mut self, path: &Path) -> Result<(), String> {
        // the old game and what it holds on the GPU go first, its sound too, and the overlay's line about it
        self.game = None;
        self.overlay_corner = None;
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        let program_id = zakuro_fs::Title::load(path).ok().map(|t| t.program_id());
        self.active = self.settings.for_game(program_id);
        let started = self.start_game(path);
        if started.is_err() {
            self.active = self.settings.clone();
        }
        if let Some(game) = &mut self.game {
            game.system.gpu.set_view(self.active.top_shape.width(), self.settings.fov_scale);
            zakuro_core::native_bridge::set_hud_scale(self.settings.hud_scale);
        }
        self.apply_layout();
        started
    }

    /// loads the game at path, the settings in force are for it
    fn start_game(&mut self, path: &Path) -> Result<(), String> {
        let config = self.config(&self.active);
        let system = loader::load(path, config).map_err(|error| format!("could not open {}, {error}", path.display()))?;
        let outdated = system.recompiled.as_ref().is_some_and(|code| code.outdated());
        // the settings ask for the 3D on the GPU but Vulkan didn't start, only the log said so while the game ran slow
        let in_software = system.config.hardware_renderer && !system.gpu.draws_on_gpu();
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "A Link Between Worlds".to_owned());
        let game = Running::new(system, path.to_owned(), name);
        // both can be true, and the GPU's is what explains a slow game
        let notes: Vec<String> = [
            in_software.then(|| GPU_FALLBACK.to_owned()),
            outdated.then(|| {
                format!(
                    "{} was recompiled by an older version of Zakuro. Recompile it again from the library to get the newest improvements.",
                    game.name
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !notes.is_empty() {
            self.menus.message = Some(notes.join("\n\n"));
        }
        self.game = Some(game);
        // nothing left open holds the game back
        self.menus.menu_open = false;
        self.menus.settings_open = false;
        self.menus.game_settings = None;
        self.keyboard.release();
        self.paused = false;
        self.next_frame = Instant::now();
        Ok(())
    }

    fn back_to_library(&mut self) {
        self.game = None;
        self.overlay_corner = None;
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        self.menus.menu_open = false;
        self.keyboard.release();
        // what the game had of its own goes w/ it
        self.active = self.settings.clone();
        self.apply_layout();
    }

    /// arranges the screens another way, for the running game alone when it has an arrangement of its own
    fn set_layout(&mut self, layout: Screens) {
        let own = self.program_id().and_then(|id| self.settings.per_game.get_mut(&settings::game_key(id)));
        match own.filter(|own| own.layout.is_some()) {
            Some(own) => own.layout = Some(layout),
            None => self.settings.layout = layout,
        }
        self.settings.save();
        self.active = self.settings.for_game(self.program_id());
        self.apply_layout();
    }

    fn key(&mut self, event: KeyEvent, consumed: bool) {
        let pressed = event.state == ElementState::Pressed;
        let PhysicalKey::Code(code) = event.physical_key else { return };
        // Escape leaves a controller binding waiting for a button as it was
        if pressed && code == KeyCode::Escape && self.menus.rebinding_pad.take().is_some() {
            return;
        }
        // a binding waiting for a key takes it, Escape leaves it as it was
        if let (true, Some(index)) = (pressed, self.menus.rebinding) {
            self.menus.rebinding = None;
            if code != KeyCode::Escape {
                if let Some((_, key)) = self.settings.keys.all_mut().into_iter().nth(index) {
                    *key = code;
                }
                self.apply_settings();
            }
            return;
        }
        if code == KeyCode::Tab && !event.repeat && !self.settings.keys.binds(KeyCode::Tab) {
            // fast forward while it's held
            self.fast_forward_key = pressed && self.game.is_some();
        }
        if pressed && !event.repeat {
            match code {
                KeyCode::Escape if self.game.is_some() => {
                    self.menus.menu_open = !self.menus.menu_open;
                    self.keyboard.release();
                    return;
                }
                KeyCode::F1 => {
                    self.paused = !self.paused;
                    log::info!("{}", if self.paused { "paused" } else { "resumed" });
                }
                KeyCode::F9 => self.set_layout(self.active.layout.next()),
                KeyCode::F10 if self.active.layout.other_screen() != self.active.layout => self.set_layout(self.active.layout.other_screen()),
                KeyCode::F11 => self.toggle_fullscreen(),
                KeyCode::F5 if self.game.is_some() => self.save_state(),
                KeyCode::F8 if self.game.is_some() => self.load_state(),
                _ => {}
            }
        }
        let to_game = self.game.is_some()
            && !self.menus.over_game()
            && !consumed
            && !self.gui.as_ref().is_some_and(Gui::wants_keyboard);
        // letting go always gets thru, so no button stays held
        if to_game || !pressed {
            self.keyboard.key(event.physical_key, pressed);
        }
    }

    /// takes in the controllers' buttons, a binding waiting for one takes it, and Home opens and closes the menu over the game
    fn poll_gamepads(&mut self) {
        for button in self.gamepads.poll() {
            if let Some(index) = self.menus.rebinding_pad.take() {
                if let Some((_, bound)) = self.settings.pad.all_mut().into_iter().nth(index) {
                    *bound = button;
                }
                self.apply_settings();
            } else if button == gilrs::Button::Mode && self.game.is_some() {
                self.menus.menu_open = !self.menus.menu_open;
                self.keyboard.release();
            }
        }
    }

    /// is the pointer busy w/ something other than the game
    fn pointer_taken(&self, consumed: bool) -> bool {
        consumed || self.game.is_none() || self.menus.menu_open || self.gui.as_ref().is_some_and(Gui::wants_pointer) || self.imgui.as_ref().is_some_and(imgui_ui::ImGuiUi::wants_pointer)
    }

    /// touches the bottom screen while the button is down over it
    fn touch(&mut self) {
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (_, bottom) = layout(size.width, size.height, self.layout.screens(), self.orientation.screen_orientation(), self.top_shape.width(), self.active.integer_scale);
        // w/ the top screen alone there's nothing to touch
        let Some(bottom) = bottom else {
            self.keyboard.touch(None);
            return;
        };
        let x = self.cursor.0 - bottom.x;
        let y = self.cursor.1 - bottom.y;
        let inside = x >= 0.0 && y >= 0.0 && x < bottom.width && y < bottom.height;
        self.keyboard.touch((self.mouse_down && inside).then(|| {
            let (u, v) = (x / bottom.width, y / bottom.height);
            let (u, v) = match self.orientation {
                Orientation::North => (u, v),
                Orientation::East => (v, 1.0 - u),
                Orientation::South => (1.0 - u, 1.0 - v),
                Orientation::West => (1.0 - v, u),
            };
            (((u * 320.0) as u16).min(319), ((v * 240.0) as u16).min(239))
        }));
    }

    /// tilts the console while the right button's held, toward where the pointer went and by how far,
    /// like Citra's motion emulation, which titles were tried w/, letting go stands it upright again
    fn tilt(&mut self) {
        // radians a pixel, and the furthest it goes, a quarter turn
        const PER_PIXEL: f32 = 0.01;
        let tilt = match (self.tilting_from, &self.window) {
            (Some((x, y)), Some(window)) => {
                let scale = window.scale_factor() as f32;
                let (dx, dy) = ((self.cursor.0 - x) / scale, (self.cursor.1 - y) / scale);
                let distance = dx.hypot(dy);
                let angle = (distance * PER_PIXEL).min(std::f32::consts::FRAC_PI_2);
                if distance > 0.0 { [dx / distance * angle, dy / distance * angle] } else { [0.0; 2] }
            }
            _ => [0.0; 2],
        };
        self.keyboard.tilt(tilt);
    }

    fn toggle_fullscreen(&mut self) {
        if let Some(window) = &self.window {
            let full = window.fullscreen().is_some();
            if !full {
                self.maximized_before_full_screen = window.is_maximized();
                self.resize_after_full_screen = false;
            }
            window.set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
            // the layout may have changed meanwhile, full screen kept the window's size thru it
            // a window maximized b4 comes back maximized, resizing it would take it out of that
            if full && self.resize_after_full_screen && !self.maximized_before_full_screen {
                let _ = window.request_inner_size(self.window_size());
            }
        }
    }

    fn apply_volume(&self) {
        if let Some(audio) = &self.audio {
            audio.set_volume(if self.settings.mute { 0.0 } else { self.settings.volume });
        }
    }

    /// saves the settings and puts to use what can change right away
    fn apply_settings(&mut self) {
        self.settings.save();
        self.active = self.settings.for_game(self.program_id());
        if let Some(game) = &mut self.game {
            if game.system.gpu.interpolates() != self.active.frame_interpolation {
                let on = game.system.set_interpolation(self.active.frame_interpolation);
                log::info!("frame interpolation {}", if on { "on" } else { "off" });
            }
            // games read the 3D slider every frame
            let slider = self.active.slider_3d();
            if game.system.config.slider_3d != slider {
                game.system.set_slider_3d(slider);
            }
            game.system.gpu.set_view(self.active.top_shape.width(), self.settings.fov_scale);
        }
        self.keyboard.set_keys(self.settings.keys.clone());
        self.gamepads.set_buttons(self.settings.pad.clone());
        self.apply_volume();
        self.apply_layout();
    }

    /// puts the screens' arrangement and the window scale to use, sizing the window to them
    fn apply_layout(&mut self) {
        if let Some(backend) = &mut self.backend {
            backend.set_top_width(self.active.top_shape.width());
            backend.set_scaling(self.active.filter.screens(), self.active.integer_scale);
            backend.set_stereo(self.active.stereo());
        }
        let scale = if self.options.scale.is_none() { self.active.scale.max(1) } else { self.scale };
        if scale != self.scale || self.active.layout != self.layout || self.active.orientation != self.orientation || self.active.top_shape != self.top_shape {
            self.scale = scale;
            self.layout = self.active.layout;
            self.orientation = self.active.orientation;
            self.top_shape = self.active.top_shape;
            if let Some(backend) = &mut self.backend {
                backend.set_layout(self.layout.screens());
                backend.set_orientation(self.orientation.screen_orientation());
            }
            // a full screen or maximized window keeps its size, resizing it would take it out of that, full screen fits it on leaving
            match &self.window {
                Some(window) if window.fullscreen().is_some() => self.resize_after_full_screen = true,
                Some(window) if !window.is_maximized() => {
                    let _ = window.request_inner_size(self.window_size());
                }
                _ => {}
            }
        }
    }

    /// the window size that fits the screens at the chosen scale
    fn window_size(&self) -> winit::dpi::Size {
        let (width, height) = self.layout.screens().size(self.top_shape.width());
        let (width, height) = match self.orientation.screen_orientation().is_portrait() {
            true => (height, width),
            false => (width, height)
        };
        // scaled by whole numbers, a desktop scaled by 125% say would leave a border,
        // so the window gets whole multiples of the screens' pixels
        if let (true, Some(window)) = (self.active.integer_scale, &self.window) {
            let times = (self.scale as f64 * window.scale_factor()).round().max(1.0) as u32;
            return winit::dpi::PhysicalSize::new(width * times, height * times).into();
        }
        winit::dpi::LogicalSize::new(width * self.scale, height * self.scale).into()
    }

    fn act(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        match action {
            Action::Play(path) => {
                if let Err(error) = self.play(&path) {
                    self.menus.message = Some(error);
                }
            }
            Action::SaveState => self.save_state(),
            Action::LoadState => self.load_state(),
            Action::ChooseFolder | Action::ChooseBackground | Action::Rescan | Action::Library | Action::CopyReport | Action::Mods(_) => {}
            Action::Resume => {
                self.menus.menu_open = false;
                self.keyboard.release();
            }
            Action::Reset => {
                if let Some(path) = self.game.as_ref().map(|game| game.path.clone()) {
                    if let Err(error) = self.play(&path) {
                        self.menus.message = Some(error);
                    }
                }
            }
            Action::Fullscreen => self.toggle_fullscreen(),
            Action::Quit => event_loop.exit(),
            Action::Settings => self.apply_settings(),
            Action::Keyboard(text, button) => {
                if let Some(game) = &mut self.game {
                    game.system.answer_keyboard(&text, button);
                }
            }
        }
    }

    fn step(&mut self, event_loop: &ActiveEventLoop) {
        if self.stop {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        if now > self.next_frame + CATCH_UP_LIMIT {
            self.next_frame = now;
        }
        self.poll_gamepads();

        // a game waiting on its keyboard waits for the user, not running
        let typing = self.game.as_ref().is_some_and(|game| game.system.keyboard_request().is_some());
        let playing = self.game.is_some() && !self.paused && !self.menus.over_game() && !typing;
        // Tab or a controller's right trigger, or the right stick's click while ZR has the trigger, whichever no console button is on
        let free = |button: gilrs::Button| self.settings.pad.map().iter().all(|&(_, bound)| bound != button);
        let trigger = [gilrs::Button::RightTrigger2, gilrs::Button::RightThumb].into_iter().find(|&button| free(button));
        let fast = playing && (self.fast_forward_key || trigger.is_some_and(|trigger| self.gamepads.holding(trigger)));
        if fast {
            // the output waits out fast forward instead of running dry
            if let Some(audio) = &self.audio {
                audio.hold();
            }
        }
        if fast != self.fast_forwarding {
            self.fast_forwarding = fast;
            if !fast {
                // back at the console's pace, w/ no sound piled up
                if let Some(audio) = &self.audio {
                    audio.clear();
                }
                self.next_frame = Instant::now();
            }
        }
        if playing {
            let start = Instant::now();
            self.emulate();
            self.spent.emulating.add(start.elapsed());
        } else if let Some(audio) = &self.audio {
            // the sound stops w/ the game, which isn't falling behind
            audio.hold();
        }

        self.next_frame += FRAME_TIME;
        if fast {
            // nothing to wait for
            self.next_frame = Instant::now();
        }
        // behind the schedule, showing the frame would wait on the display, so it goes unshown, a few at most
        // a presenter that doesn't wait (mailbox mode) shows them all, a game that couldn't keep up showed a fifth of its frames
        // fast forward shows them at the console's frame rate, waiting on the display for no more
        let behind = Instant::now() > self.next_frame;
        let waits = self.backend.as_ref().is_some_and(Backend::waits_for_display);
        let unshown = if fast { Instant::now() < self.next_shown } else { skips_showing(playing, behind, waits, self.skipped) };
        if unshown {
            self.skipped += 1;
        } else {
            self.skipped = 0;
            let start = Instant::now();
            self.present(event_loop);
            // the next one a frame later, or right away after falling behind, at the console's rate on average
            self.next_shown = (self.next_shown + FRAME_TIME).max(Instant::now() - FRAME_TIME);
            self.spent.showing.add(start.elapsed());
            self.shown += 1;
        }

        if self.last_title_update.elapsed() >= Duration::from_millis(500) {
            let shown = self.shown as f32 / self.last_title_update.elapsed().as_secs_f32();
            self.shown = 0;
            self.last_title_update = Instant::now();
            if let Some(game) = &self.game {
                let Spent { emulating, showing } = std::mem::take(&mut self.spent);
                log::debug!(
                    target: "zakuro::fps",
                    "{:.1} frames emulated and {shown:.1} shown a second, emulating took {emulating}, showing {showing}",
                    game.fps
                );
            }
            if let Some(window) = &self.window {
                let title = match &self.game {
                    Some(game) => format!("Zakuro - {} - {}", game.name, game.system.status_line()),
                    None => "Zakuro".to_owned(),
                };
                window.set_title(&title);
            }
            let underruns = self.audio.as_ref().map_or(0, |audio| audio.take_underruns());
            if underruns > 0 && playing {
                log::warn!("the sound ran dry {underruns} times, the emulation is falling behind");
            }
        }

        let now = Instant::now();
        if now < self.next_frame {
            std::thread::sleep(self.next_frame - now);
        }
    }

    /// runs one frame of the game
    fn emulate(&mut self) {
        let Some(game) = &mut self.game else { return };
        let input = self.gamepads.apply(self.keyboard.state());
        game.system.set_input(input);
        let outcome = game.system.run_frame();
        // the overlay reads the game the program was built on, not another one its library opened
        let ours = self.linked.as_ref().is_none_or(|linked| game.system.title.as_ref().is_some_and(|title| title.program_id() == linked.program_id()));
        if let Some(overlay) = self.overlay.as_mut().filter(|_| ours) {
            let text = overlay.frame(&mut game.system.memory);
            self.overlay_corner = text.corner;
            if let Some(notice) = text.notice {
                self.menus.toast = Some((notice, Instant::now()));
            }
        }
        game.count_frame();
        let sound = game.system.take_audio();
        // fast forward plays no sound, there's too much of it too soon
        if let (Some(audio), false) = (&self.audio, self.fast_forwarding) {
            audio.push(&sound);
        }
        match outcome {
            FrameOutcome::Completed => {}
            FrameOutcome::Exited => {
                log::info!("the title exited");
                self.back_to_library();
            }
            FrameOutcome::Faulted => {
                log::error!("the title stopped on a fault");
                let errors = game.system.fatal_errors.join("; ");
                if !errors.is_empty() {
                    log::error!("{errors}");
                }
                let message = format!("The game stopped on a fault. {errors}");
                self.menus.message = Some(message);
                self.back_to_library();
            }
        }
    }

    /// the name the player goes by, the console's too
    fn nickname(&self) -> String {
        match self.settings.nickname.trim() {
            "" => "Player".into(),
            name => name.chars().take(10).collect(),
        }
    }

    /// the file of a slot of the running game's save states
    fn state_path(&self, slot: u8) -> Option<PathBuf> {
        let program_id = self.game.as_ref()?.system.title.as_ref()?.program_id();
        Some(self.data_dir.as_ref()?.join("states").join(format!("{program_id:016X}")).join(format!("{slot}.state")))
    }

    /// saves the running game's state to the slot chosen
    fn save_state(&mut self) {
        let slot = self.menus.slot;
        let Some(path) = self.state_path(slot) else { return };
        let Some(game) = &mut self.game else { return };
        let started = Instant::now();
        let saved = game.system.save_state().and_then(|bytes| {
            let parent = path.parent().expect("a state's file is in a folder");
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            // written beside it first, so a state cut short never replaces a whole one
            let partial = path.with_extension("state.new");
            std::fs::write(&partial, &bytes).and_then(|()| std::fs::rename(&partial, &path)).map_err(|error| error.to_string())
        });
        match saved {
            Ok(()) => {
                log::info!("saved the state to {} in {:?}", path.display(), started.elapsed());
                self.menus.toast = Some((format!("Saved to slot {slot}"), Instant::now()));
            }
            Err(error) => self.menus.message = Some(format!("The state could not be saved, {error}")),
        }
        // the time it took isn't the game's to make up
        self.next_frame = Instant::now();
    }

    /// loads the running game's state from the slot chosen
    fn load_state(&mut self) {
        let slot = self.menus.slot;
        let Some(path) = self.state_path(slot) else { return };
        let Some(game) = &mut self.game else { return };
        let loaded = std::fs::read(&path)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => format!("Slot {slot} has no state saved."),
                _ => format!("The state could not be read, {error}"),
            })
            .and_then(|bytes| game.system.load_state(&bytes).map_err(|error| format!("The state could not be loaded, {error}.")));
        match loaded {
            Ok(()) => {
                log::info!("loaded the state from {}", path.display());
                self.menus.toast = Some((format!("Loaded slot {slot}"), Instant::now()));
                self.menus.menu_open = false;
                // the overlay's line was about the memory b4, the next frame gives the one about the state
                self.overlay_corner = None;
                if let Some(audio) = &self.audio {
                    audio.clear();
                }
            }
            Err(error) => self.menus.message = Some(error),
        }
        self.next_frame = Instant::now();
    }

    /// the menus, and the actions they asked for, done
    fn interface(&mut self, event_loop: &ActiveEventLoop) -> Overlay {
        // when the chosen slot was saved to, while the menu shows it
        let saved = self
            .menus
            .menu_open
            .then(|| self.state_path(self.menus.slot))
            .flatten()
            .and_then(|path| std::fs::metadata(path).ok()?.modified().ok())
            .map(ago);
        let program_id = self.program_id();
        let (Some(gui), Some(window)) = (&mut self.gui, &self.window) else { return Overlay::default() };
        let show_fps = self.settings.show_fps;
        let game = self.game.as_ref().map(|game| (game.name.clone(), game.fps, game.system.recompiled.is_some()));
        let fast = self.fast_forwarding;
        let keyboard = self.game.as_ref().and_then(|game| game.system.keyboard_request().cloned());
        let corner = self.overlay_corner.clone().filter(|_| game.is_some());
        let (menus, settings) = (&mut self.menus, &mut self.settings);
        let mut actions = Vec::new();
        let draw_egui_pause = self.imgui.is_none();
        let mut overlay = gui.frame(window, |ui| {
            if let Some((name, fps, recompiled)) = &game {
                if draw_egui_pause {
                    actions.extend(menus.game(ui, name, program_id, show_fps.then_some(*fps), *recompiled, fast, saved.as_deref()));
                }
                if let Some(text) = &corner {
                    menus::corner(ui.ctx(), text);
                }
            }
            let own = program_id.is_some_and(|id| settings.has_own(id));
            actions.extend(menus.settings(ui.ctx(), settings, own));
            actions.extend(menus.game_settings(ui.ctx(), settings));
            if let Some(request) = &keyboard {
                actions.extend(menus.keyboard(ui.ctx(), request));
            }
            menus.message(ui.ctx());
        });

        let mut imgui_actions = Vec::new();
        if let Some(imgui) = &mut self.imgui {
            let layout_idx = settings::Screens::ALL.iter().position(|&s| s == settings.layout).unwrap_or(0) as i32;
            let shape_idx = settings::TopShape::ALL.iter().position(|&s| s == settings.top_shape).unwrap_or(0) as i32;
            let filter_idx = settings::Filter::ALL.iter().position(|&f| f == settings.filter).unwrap_or(0) as i32;
            let mut st = imgui_ui::ImGuiMenuState {
                display_w: 0.0,
                display_h: 0.0,
                dpi_scale: 1.0,
                delta_time: 1.0 / 60.0,
                menu_open: menus.menu_open && !menus.settings_open && menus.game_settings.is_none(),
                show_fps: settings.show_fps,
                current_fps: game.as_ref().map_or(0.0, |g| g.1),
                slot: menus.slot as i32,
                slot_has_save: saved.is_some(),
                resolution: settings.resolution as i32,
                scale: settings.scale as i32,
                layout: layout_idx,
                top_shape: shape_idx,
                fov_scale: settings.fov_scale,
                hud_scale: settings.hud_scale,
                filter: filter_idx,
                integer_scale: settings.integer_scale,
                hardware_rasterizer: settings.hardware_rasterizer,
                frame_interpolation: settings.frame_interpolation,
                volume: settings.volume,
                mute: settings.mute,
                actions: 0,
            };
            imgui.render(window, &mut st, &mut overlay);

            menus.slot = st.slot.clamp(1, 9) as u8;
            if (st.actions & (1 << 0)) != 0 {
                imgui_actions.push(Action::Resume);
            }
            if (st.actions & (1 << 1)) != 0 {
                imgui_actions.push(Action::Reset);
            }
            if (st.actions & (1 << 2)) != 0 {
                imgui_actions.push(Action::Fullscreen);
            }
            if (st.actions & (1 << 3)) != 0 {
                imgui_actions.push(Action::SaveState);
            }
            if (st.actions & (1 << 4)) != 0 {
                imgui_actions.push(Action::LoadState);
            }
            if (st.actions & (1 << 5)) != 0 {
                menus.settings_open = true;
            }
            if (st.actions & (1 << 6)) != 0 {
                imgui_actions.push(Action::Quit);
            }
            if (st.actions & (1 << 7)) != 0 {
                settings.hardware_rasterizer = st.hardware_rasterizer;
                settings.resolution = st.resolution.clamp(1, 6) as u32;
                settings.frame_interpolation = st.frame_interpolation;
                settings.scale = st.scale.clamp(1, 6) as u32;
                settings.layout = settings::Screens::ALL.get(st.layout as usize).copied().unwrap_or_default();
                settings.top_shape = settings::TopShape::ALL.get(st.top_shape as usize).copied().unwrap_or_default();
                settings.fov_scale = st.fov_scale.clamp(0.8, 1.5);
                settings.hud_scale = st.hud_scale.clamp(0.55, 1.0);
                zakuro_core::native_bridge::set_hud_scale(settings.hud_scale);
                settings.filter = settings::Filter::ALL.get(st.filter as usize).copied().unwrap_or_default();
                settings.integer_scale = st.integer_scale;
                settings.show_fps = st.show_fps;
                settings.volume = st.volume.clamp(0.0, 1.0);
                settings.mute = st.mute;
                imgui_actions.push(Action::Settings);
            }
        }
        actions.extend(imgui_actions);
        for action in actions {
            self.act(action, event_loop);
        }
        overlay
    }

    fn present(&mut self, event_loop: &ActiveEventLoop) {
        let overlay = self.interface(event_loop);
        let blank = |screen: Screen| ((std::sync::Arc::new(Vec::new()), screen.width(), screen.height()), None);
        // the right eye's picture only while the 3D shows
        let stereo = self.active.stereo != settings::Stereo3d::Off;
        let (top, bottom, right) = match &mut self.game {
            Some(game) => (
                shown(&mut game.system, Screen::Top),
                shown(&mut game.system, Screen::Bottom),
                stereo.then(|| shown_right(&mut game.system)).flatten(),
            ),
            None => (blank(Screen::Top), blank(Screen::Bottom), None),
        };
        let right = right.unwrap_or_else(|| blank(Screen::Top));
        let Some(backend) = &mut self.backend else { return };
        let (((top, top_width, top_height), top_gpu), ((bottom, bottom_width, bottom_height), bottom_gpu)) = (top, bottom);
        let ((right, right_width, right_height), right_gpu) = right;
        let result = backend.present(
            ScreenImage { width: top_width, height: top_height, pixels: &top, gpu: top_gpu },
            ScreenImage { width: bottom_width, height: bottom_height, pixels: &bottom, gpu: bottom_gpu },
            ScreenImage { width: right_width, height: right_height, pixels: &right, gpu: right_gpu },
            &overlay,
        );
        match result {
            Ok(()) | Err(PresentError::OutOfDate) => {}
            Err(error) => {
                log::error!("presentation failed: {error}");
                event_loop.exit();
            }
        }
    }
}

/// a screen as the presenter takes it, straight from the GPU when it shares the renderer's device and the GPU drew
/// what the screen shows, otherwise its pixels
fn shown(system: &mut System, screen: Screen) -> (zakuro_core::Screen, Option<GpuScreen>) {
    static SEEN_GPU_TOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Some(gpu) = system.gpu_screen(screen) {
        if matches!(screen, Screen::Top) {
            SEEN_GPU_TOP.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let scale = system.gpu.scale();
        return ((std::sync::Arc::new(Vec::new()), screen.width() * scale, screen.height() * scale), Some(gpu));
    }
    if matches!(screen, Screen::Top) && SEEN_GPU_TOP.load(std::sync::atomic::Ordering::Relaxed) {
        log::warn!("gpu_screen(Top) returned None, falling back to CPU read_screen_scaled");
    }
    (system.read_screen_scaled(screen), None)
}

/// the top screen's picture for the right eye, like shown gives the screens, when the game draws in 3D
fn shown_right(system: &mut System) -> Option<(zakuro_core::Screen, Option<GpuScreen>)> {
    if let Some(gpu) = system.gpu_right_eye() {
        let scale = system.gpu.scale();
        return Some(((std::sync::Arc::new(Vec::new()), Screen::Top.width() * scale, Screen::Top.height() * scale), Some(gpu)));
    }
    system.read_right_eye_scaled().map(|screen| (screen, None))
}

/// keeps the renderer kind referenced even when a backend feature is off
const _: RendererKind = RendererKind::Software;

#[cfg(test)]
mod tests {
    use super::*;

    /// a frame only goes unshown while the game runs behind w/ a presenter that waits for the display, a few in a row at most
    #[test]
    fn frames_go_unshown_only_when_showing_would_wait() {
        assert!(skips_showing(true, true, true, 0));
        assert!(skips_showing(true, true, true, MAX_SKIPPED - 1));
        assert!(!skips_showing(true, true, true, MAX_SKIPPED), "a few in a row at most");
        assert!(!skips_showing(true, true, false, 0), "mailbox shows every frame");
        assert!(!skips_showing(true, false, true, 0), "on time");
        assert!(!skips_showing(false, true, true, 0), "paused");
    }
}

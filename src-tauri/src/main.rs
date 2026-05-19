#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rand::Rng;
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size,
};

const BASE_PET_SIZE: u32 = 150;
const TICK_MS: u64 = 33;
const EDGE_PADDING: f64 = 8.0;
const PATROL_SPEED: f64 = 2.8;
const PATROL_ACCELERATION: f64 = 0.18;
const ESCAPE_FORCE: f64 = 5.2;
const MAX_PATROL_SPEED: f64 = 4.2;
const MAX_ESCAPE_SPEED: f64 = 13.5;
const FRICTION: f64 = 0.94;
const START_DURATION_MS: u64 = 900;
const STOP_DURATION_MS: u64 = 850;
const IDLE_MIN_MS: u64 = 5_000;
const IDLE_MAX_MS: u64 = 20_000;
const PATROL_MIN_MS: u64 = 3_000;
const PATROL_MAX_MS: u64 = 60_000;
const IDLE_SHAPESHIFT_CHANCE: f64 = 0.3;
const IDLE_SHAPESHIFT_INTERVAL_MS: u64 = 1_000;

type SharedState = Arc<Mutex<AppState>>;

#[derive(Clone, Copy, PartialEq)]
enum SizeProfile {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, PartialEq)]
enum SpeedProfile {
    Slow,
    Normal,
    Fast,
}

#[derive(Clone, Copy, PartialEq)]
enum Activity {
    Work,
    Slacking,
}

#[derive(Clone, Copy, PartialEq)]
enum Behavior {
    Idle,
    Starting,
    Patrol,
    Stopping,
}

#[derive(Clone, Copy)]
struct PetConfig {
    size: SizeProfile,
    speed: SpeedProfile,
    activity: Activity,
}

struct PetState {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    facing: &'static str,
    moving: bool,
}

struct AppState {
    config: PetConfig,
    pet: PetState,
    patrol_direction: f64,
    patrol_y: f64,
    was_escaping: bool,
    behavior: Behavior,
    behavior_started_at: Instant,
    next_behavior_change_at: Instant,
    idle_mood: &'static str,
    idle_shapeshift: bool,
    next_idle_mood_change_at: Instant,
}

#[derive(Clone, Serialize)]
struct RendererState {
    facing: &'static str,
    moving: bool,
    behavior: &'static str,
    #[serde(rename = "idleMood")]
    idle_mood: &'static str,
    #[serde(rename = "idleShapeshift")]
    idle_shapeshift: bool,
    activity: &'static str,
    size: u32,
    #[serde(rename = "baseSize")]
    base_size: u32,
    speed: f64,
}

#[derive(Clone, Copy)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn main() {
    let shared = Arc::new(Mutex::new(AppState::new()));

    tauri::Builder::default()
        .manage(shared.clone())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let window = app.get_webview_window("pet").expect("pet window exists");
            window.set_ignore_cursor_events(true)?;
            window.set_always_on_top(true)?;
            let _ = window.set_visible_on_all_workspaces(true);
            create_tray(app.handle(), shared.clone())?;
            initialize_window(app.handle(), &window, &shared)?;
            start_motion_loop(app.handle().clone(), window, shared.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running desktop pet");
}

impl AppState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            config: PetConfig {
                size: SizeProfile::Small,
                speed: SpeedProfile::Fast,
                activity: Activity::Work,
            },
            pet: PetState {
                x: 0.0,
                y: 0.0,
                vx: 2.0,
                vy: 0.0,
                facing: "right",
                moving: true,
            },
            patrol_direction: -1.0,
            patrol_y: 0.0,
            was_escaping: false,
            behavior: Behavior::Patrol,
            behavior_started_at: now,
            next_behavior_change_at: now + random_duration(PATROL_MIN_MS, PATROL_MAX_MS),
            idle_mood: "calm",
            idle_shapeshift: false,
            next_idle_mood_change_at: now + Duration::from_millis(IDLE_SHAPESHIFT_INTERVAL_MS),
        }
    }

    fn set_behavior(&mut self, behavior: Behavior) {
        let now = Instant::now();
        self.behavior = behavior;
        self.behavior_started_at = now;
        match behavior {
            Behavior::Idle => {
                self.idle_shapeshift = rand::thread_rng().gen_bool(IDLE_SHAPESHIFT_CHANCE);
                self.choose_idle_mood();
                self.next_idle_mood_change_at =
                    now + Duration::from_millis(IDLE_SHAPESHIFT_INTERVAL_MS);
                self.next_behavior_change_at = now + random_duration(IDLE_MIN_MS, IDLE_MAX_MS);
            }
            Behavior::Patrol => {
                self.idle_shapeshift = false;
                self.next_behavior_change_at = now + random_duration(PATROL_MIN_MS, PATROL_MAX_MS);
            }
            _ => {
                self.idle_shapeshift = false;
            }
        }
    }

    fn choose_idle_mood(&mut self) {
        self.idle_mood = match self.config.activity {
            Activity::Work => pick_weighted(&[
                ("happy", 1),
                ("calm", 9),
                ("angry", 60),
                ("sorrow", 30),
            ]),
            Activity::Slacking => pick_weighted(&[
                ("happy", 40),
                ("calm", 40),
                ("angry", 5),
                ("sorrow", 15),
            ]),
        };
    }
}

fn initialize_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    shared: &SharedState,
) -> tauri::Result<()> {
    let bounds = virtual_work_area(app).unwrap_or(Bounds {
        x: 0.0,
        y: 0.0,
        width: 1280.0,
        height: 720.0,
    });
    let mut state = shared.lock().expect("state lock");
    let pet_size = pet_size(state.config.size) as f64;
    state.pet.x = bounds.x + bounds.width - pet_size - 80.0;
    state.pet.y = bounds.y + bounds.height - pet_size - 60.0;
    state.patrol_y = state.pet.y;
    window.set_size(Size::Physical(PhysicalSize {
        width: pet_size as u32,
        height: pet_size as u32,
    }))?;
    window.set_position(Position::Physical(PhysicalPosition {
        x: state.pet.x.round() as i32,
        y: state.pet.y.round() as i32,
    }))?;
    window.show()?;
    window.set_always_on_top(true)?;
    Ok(())
}

fn start_motion_loop(app: tauri::AppHandle, window: tauri::WebviewWindow, shared: SharedState) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(TICK_MS));
            let cursor = app
                .cursor_position()
                .map(|position| (position.x, position.y))
                .unwrap_or((f64::INFINITY, f64::INFINITY));
            let bounds = virtual_work_area(&app).unwrap_or(Bounds {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 720.0,
            });

            let renderer_state = {
                let mut state = shared.lock().expect("state lock");
                update_motion(&mut state, bounds, cursor);
                let pet_size = pet_size(state.config.size);
                let _ = window.set_size(Size::Physical(PhysicalSize {
                    width: pet_size,
                    height: pet_size,
                }));
                let _ = window.set_position(Position::Physical(PhysicalPosition {
                    x: state.pet.x.round() as i32,
                    y: state.pet.y.round() as i32,
                }));
                let _ = window.set_always_on_top(true);
                build_renderer_state(&state)
            };

            let _ = window.emit("pet-state", renderer_state);
        }
    });
}

fn update_motion(state: &mut AppState, bounds: Bounds, cursor: (f64, f64)) {
    let now = Instant::now();
    let pet_size = pet_size(state.config.size) as f64;
    let speed_multiplier = speed_multiplier(state.config.speed);
    let avoid_radius = avoid_radius(state.config.size);
    let center_x = state.pet.x + pet_size / 2.0;
    let center_y = state.pet.y + pet_size / 2.0;
    let dx = center_x - cursor.0;
    let dy = center_y - cursor.1;
    let distance = (dx * dx + dy * dy).sqrt();
    let is_escaping = distance < avoid_radius;
    let just_stopped_escaping = state.was_escaping && !is_escaping;

    if is_escaping {
        let safe_distance = distance.max(1.0);
        let strength = (avoid_radius - safe_distance) / avoid_radius;
        state.pet.vx += dx / safe_distance * strength * ESCAPE_FORCE * speed_multiplier;
        state.pet.vy += dy / safe_distance * strength * ESCAPE_FORCE * speed_multiplier;
    } else {
        if just_stopped_escaping {
            state.patrol_y = state.pet.y;
            state.pet.vy = 0.0;
        }

        let mut elapsed = now.duration_since(state.behavior_started_at).as_millis() as f64;
        if state.behavior == Behavior::Idle && now > state.next_behavior_change_at {
            state.set_behavior(Behavior::Starting);
        } else if state.behavior == Behavior::Idle
            && state.idle_shapeshift
            && now > state.next_idle_mood_change_at
        {
            state.choose_idle_mood();
            state.next_idle_mood_change_at =
                now + Duration::from_millis(IDLE_SHAPESHIFT_INTERVAL_MS);
        } else if state.behavior == Behavior::Starting && elapsed > START_DURATION_MS as f64 {
            state.set_behavior(Behavior::Patrol);
        } else if state.behavior == Behavior::Patrol && now > state.next_behavior_change_at {
            state.set_behavior(Behavior::Stopping);
        } else if state.behavior == Behavior::Stopping && elapsed > STOP_DURATION_MS as f64 {
            state.set_behavior(Behavior::Idle);
        }

        elapsed = now.duration_since(state.behavior_started_at).as_millis() as f64;
        let patrol_force = match state.behavior {
            Behavior::Starting => {
                PATROL_ACCELERATION * speed_multiplier * ease_in_out(elapsed / START_DURATION_MS as f64)
            }
            Behavior::Patrol => PATROL_ACCELERATION * speed_multiplier,
            Behavior::Stopping => {
                let stop_progress = ease_in_out(elapsed / STOP_DURATION_MS as f64);
                state.pet.vx *= 1.0 - stop_progress * 0.09;
                PATROL_ACCELERATION * speed_multiplier * (1.0 - stop_progress) * 0.45
            }
            Behavior::Idle => {
                state.pet.vx *= 0.82;
                0.0
            }
        };

        state.pet.vx += state.patrol_direction * patrol_force;
        state.pet.vy += (state.patrol_y - state.pet.y) * 0.08;
    }

    state.pet.vx *= FRICTION;
    state.pet.vy *= if is_escaping { FRICTION } else { 0.72 };

    let speed = hypot(state.pet.vx, state.pet.vy);
    let behavior_elapsed = now.duration_since(state.behavior_started_at).as_millis() as f64;
    let max_speed = if is_escaping {
        MAX_ESCAPE_SPEED * speed_multiplier
    } else if state.behavior == Behavior::Starting {
        (PATROL_SPEED * speed_multiplier * 0.35)
            .max(MAX_PATROL_SPEED * speed_multiplier * ease_in_out(behavior_elapsed / START_DURATION_MS as f64))
    } else if state.behavior == Behavior::Stopping || state.behavior == Behavior::Idle {
        0.18_f64.max(
            MAX_PATROL_SPEED
                * speed_multiplier
                * (1.0 - ease_in_out(behavior_elapsed / STOP_DURATION_MS as f64)),
        )
    } else {
        MAX_PATROL_SPEED * speed_multiplier
    };

    if speed > max_speed {
        state.pet.vx = state.pet.vx / speed * max_speed;
        state.pet.vy = state.pet.vy / speed * max_speed;
    }

    state.pet.x += state.pet.vx;
    state.pet.y += state.pet.vy;
    clamp_patrol_y(state, bounds);
    clamp_to_bounds(state, bounds);

    if !is_escaping {
        let min_x = bounds.x + EDGE_PADDING;
        let max_x = bounds.x + bounds.width - pet_size - EDGE_PADDING;
        state.pet.y = state.patrol_y;
        state.pet.vy = 0.0;

        if state.pet.x <= min_x + 2.0 {
            state.patrol_direction = 1.0;
            state.pet.vx = (PATROL_SPEED * speed_multiplier).max(state.pet.vx.abs());
        } else if state.pet.x >= max_x - 2.0 {
            state.patrol_direction = -1.0;
            state.pet.vx = -(PATROL_SPEED * speed_multiplier).max(state.pet.vx.abs());
        }
    } else {
        state.patrol_direction = if state.pet.vx >= 0.0 { 1.0 } else { -1.0 };
    }

    if state.pet.vx.abs() > 0.25 {
        state.pet.facing = if state.pet.vx >= 0.0 { "right" } else { "left" };
    }
    state.pet.moving = is_escaping || hypot(state.pet.vx, state.pet.vy) > 0.8;
    state.was_escaping = is_escaping;
}

fn build_renderer_state(state: &AppState) -> RendererState {
    RendererState {
        facing: state.pet.facing,
        moving: state.pet.moving,
        behavior: if state.was_escaping {
            "escaping"
        } else {
            behavior_name(state.behavior)
        },
        idle_mood: state.idle_mood,
        idle_shapeshift: state.idle_shapeshift,
        activity: activity_name(state.config.activity),
        size: pet_size(state.config.size),
        base_size: BASE_PET_SIZE,
        speed: hypot(state.pet.vx, state.pet.vy),
    }
}

fn create_tray(app: &tauri::AppHandle, shared: SharedState) -> tauri::Result<()> {
    let menu = build_tray_menu(app, &shared)?;
    let icon = Image::from_bytes(include_bytes!("../tray-icon.png"))?;
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Desktop Pet")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            handle_menu_event(app, event.id().0.as_str(), &shared);
        })
        .build(app)?;
    Ok(())
}

fn build_tray_menu(app: &tauri::AppHandle, shared: &SharedState) -> tauri::Result<Menu<tauri::Wry>> {
    let config = shared.lock().expect("state lock").config;
    let show = MenuItem::with_id(app, "show", "显示宠物", true, None::<&str>)?;
    let small = CheckMenuItem::with_id(app, "size:small", "小", true, config.size == SizeProfile::Small, None::<&str>)?;
    let medium = CheckMenuItem::with_id(app, "size:medium", "中", true, config.size == SizeProfile::Medium, None::<&str>)?;
    let large = CheckMenuItem::with_id(app, "size:large", "大", true, config.size == SizeProfile::Large, None::<&str>)?;
    let size_menu = Submenu::with_items(app, "大小", true, &[&small, &medium, &large])?;

    let slow = CheckMenuItem::with_id(app, "speed:slow", "慢", true, config.speed == SpeedProfile::Slow, None::<&str>)?;
    let normal = CheckMenuItem::with_id(app, "speed:normal", "正常", true, config.speed == SpeedProfile::Normal, None::<&str>)?;
    let fast = CheckMenuItem::with_id(app, "speed:fast", "快", true, config.speed == SpeedProfile::Fast, None::<&str>)?;
    let speed_menu = Submenu::with_items(app, "运动速度", true, &[&slow, &normal, &fast])?;

    let work = CheckMenuItem::with_id(app, "activity:work", "工作", true, config.activity == Activity::Work, None::<&str>)?;
    let slacking = CheckMenuItem::with_id(app, "activity:slacking", "摸鱼", true, config.activity == Activity::Slacking, None::<&str>)?;
    let activity_label = format!("我在{}", activity_label(config.activity));
    let activity_menu = Submenu::with_items(app, activity_label, true, &[&work, &slacking])?;

    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &size_menu,
            &speed_menu,
            &activity_menu,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

fn handle_menu_event(app: &tauri::AppHandle, id: &str, shared: &SharedState) {
    match id {
        "quit" => app.exit(0),
        "show" => {
            if let Some(window) = app.get_webview_window("pet") {
                let _ = window.show();
                let _ = window.set_always_on_top(true);
            }
        }
        "size:small" | "size:medium" | "size:large" => {
            let next_size = match id {
                "size:small" => SizeProfile::Small,
                "size:medium" => SizeProfile::Medium,
                _ => SizeProfile::Large,
            };
            apply_size(app, shared, next_size);
            refresh_tray_menu(app, shared);
        }
        "speed:slow" | "speed:normal" | "speed:fast" => {
            let next_speed = match id {
                "speed:slow" => SpeedProfile::Slow,
                "speed:normal" => SpeedProfile::Normal,
                _ => SpeedProfile::Fast,
            };
            shared.lock().expect("state lock").config.speed = next_speed;
            refresh_tray_menu(app, shared);
        }
        "activity:work" | "activity:slacking" => {
            let mut state = shared.lock().expect("state lock");
            state.config.activity = if id == "activity:work" {
                Activity::Work
            } else {
                Activity::Slacking
            };
            if state.behavior == Behavior::Idle {
                state.choose_idle_mood();
            }
            drop(state);
            refresh_tray_menu(app, shared);
        }
        _ => {}
    }
}

fn apply_size(app: &tauri::AppHandle, shared: &SharedState, next_size: SizeProfile) {
    let mut state = shared.lock().expect("state lock");
    let previous = pet_size(state.config.size) as f64;
    let next = pet_size(next_size) as f64;
    state.config.size = next_size;
    let foot_offset = previous - next;
    state.pet.x += foot_offset / 2.0;
    state.pet.y += foot_offset;
    state.patrol_y += foot_offset;
    drop(state);

    if let Some(window) = app.get_webview_window("pet") {
        let _ = window.set_size(Size::Physical(PhysicalSize {
            width: next as u32,
            height: next as u32,
        }));
    }
}

fn refresh_tray_menu(app: &tauri::AppHandle, shared: &SharedState) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_tray_menu(app, shared)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn virtual_work_area(app: &tauri::AppHandle) -> Option<Bounds> {
    let monitors = app.available_monitors().ok()?;
    let mut areas = monitors.iter().map(|monitor| monitor.work_area());
    let first = areas.next()?;
    let mut min_x = first.position.x as f64;
    let mut min_y = first.position.y as f64;
    let mut max_x = first.position.x as f64 + first.size.width as f64;
    let mut max_y = first.position.y as f64 + first.size.height as f64;

    for area in areas {
        min_x = min_x.min(area.position.x as f64);
        min_y = min_y.min(area.position.y as f64);
        max_x = max_x.max(area.position.x as f64 + area.size.width as f64);
        max_y = max_y.max(area.position.y as f64 + area.size.height as f64);
    }

    Some(Bounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    })
}

fn clamp_to_bounds(state: &mut AppState, bounds: Bounds) {
    let pet_size = pet_size(state.config.size) as f64;
    let min_x = bounds.x + EDGE_PADDING;
    let min_y = bounds.y + EDGE_PADDING;
    let max_x = bounds.x + bounds.width - pet_size - EDGE_PADDING;
    let max_y = bounds.y + bounds.height - pet_size - EDGE_PADDING;

    if state.pet.x < min_x {
        state.pet.x = min_x;
        state.pet.vx = state.pet.vx.abs() + 1.0;
    }
    if state.pet.x > max_x {
        state.pet.x = max_x;
        state.pet.vx = -state.pet.vx.abs() - 1.0;
    }
    if state.pet.y < min_y {
        state.pet.y = min_y;
        state.pet.vy = state.pet.vy.abs() + 1.0;
    }
    if state.pet.y > max_y {
        state.pet.y = max_y;
        state.pet.vy = -state.pet.vy.abs() - 1.0;
    }
}

fn clamp_patrol_y(state: &mut AppState, bounds: Bounds) {
    let pet_size = pet_size(state.config.size) as f64;
    let min_y = bounds.y + EDGE_PADDING;
    let max_y = bounds.y + bounds.height - pet_size - EDGE_PADDING;
    state.patrol_y = state.patrol_y.max(min_y).min(max_y);
}

fn pet_size(size: SizeProfile) -> u32 {
    match size {
        SizeProfile::Small => 48,
        SizeProfile::Medium => 96,
        SizeProfile::Large => 192,
    }
}

fn speed_multiplier(speed: SpeedProfile) -> f64 {
    match speed {
        SpeedProfile::Slow => 0.6,
        SpeedProfile::Normal => 1.2,
        SpeedProfile::Fast => 2.4,
    }
}

fn avoid_radius(size: SizeProfile) -> f64 {
    match size {
        SizeProfile::Small => 96.0,
        SizeProfile::Medium => 170.0,
        SizeProfile::Large => 280.0,
    }
}

fn activity_name(activity: Activity) -> &'static str {
    match activity {
        Activity::Work => "work",
        Activity::Slacking => "slacking",
    }
}

fn activity_label(activity: Activity) -> &'static str {
    match activity {
        Activity::Work => "工作",
        Activity::Slacking => "摸鱼",
    }
}

fn behavior_name(behavior: Behavior) -> &'static str {
    match behavior {
        Behavior::Idle => "idle",
        Behavior::Starting => "starting",
        Behavior::Patrol => "patrol",
        Behavior::Stopping => "stopping",
    }
}

fn pick_weighted(items: &[(&'static str, u32)]) -> &'static str {
    let total: u32 = items.iter().map(|(_, weight)| *weight).sum();
    let mut target = rand::thread_rng().gen_range(0..total);
    for (name, weight) in items {
        if target < *weight {
            return name;
        }
        target -= *weight;
    }
    items.last().map(|(name, _)| *name).unwrap_or("calm")
}

fn random_duration(min_ms: u64, max_ms: u64) -> Duration {
    Duration::from_millis(rand::thread_rng().gen_range(min_ms..=max_ms))
}

fn ease_in_out(t: f64) -> f64 {
    let progress = t.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

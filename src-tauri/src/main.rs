#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rand::Rng;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    Emitter, LogicalPosition, LogicalSize, Manager, Position, Size, WebviewUrl,
    WebviewWindowBuilder,
};

const BASE_PET_SIZE: u32 = 150;
const SPEECH_TAIL_MIN_PERCENT: f64 = 12.0;
const SPEECH_TAIL_MAX_PERCENT: f64 = 88.0;
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
const STOPPED_MIN_MS: u64 = 5_000;
const STOPPED_MAX_MS: u64 = 20_000;
const PATROL_MIN_MS: u64 = 3_000;
const PATROL_MAX_MS: u64 = 60_000;
const STOPPED_SHAPESHIFT_CHANCE: f64 = 0.3;
const STOPPED_SHAPESHIFT_INTERVAL_MS: u64 = 1_000;
const SETTINGS_FILE_NAME: &str = "settings.json";
const I18N_JSON: &str = include_str!("../../app/assets/pet/i18n.json");

type SharedState = Arc<Mutex<AppState>>;

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum SizeProfile {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum SpeedProfile {
    Slow,
    Normal,
    Fast,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Activity {
    Work,
    Slacking,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Language {
    Zh,
    En,
}

#[derive(Clone, Copy, PartialEq)]
enum Behavior {
    Stopped,
    Starting,
    Patrol,
    Stopping,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct PetConfig {
    size: SizeProfile,
    speed: SpeedProfile,
    activity: Activity,
    #[serde(default)]
    language: Language,
    #[serde(default = "default_talk_when_stopped")]
    talk_when_stopped: bool,
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
    stopped_mood: &'static str,
    stopped_shapeshift: bool,
    next_stopped_mood_change_at: Instant,
}

#[derive(Clone, Serialize)]
struct RendererState {
    facing: &'static str,
    moving: bool,
    behavior: &'static str,
    #[serde(rename = "stoppedMood")]
    stopped_mood: &'static str,
    #[serde(rename = "stoppedShapeshift")]
    stopped_shapeshift: bool,
    activity: &'static str,
    size: u32,
    #[serde(rename = "baseSize")]
    base_size: u32,
    speed: f64,
    language: &'static str,
    #[serde(rename = "talkWhenStopped")]
    talk_when_stopped: bool,
    #[serde(rename = "speechPlacement")]
    speech_placement: &'static str,
    #[serde(rename = "speechTailPercent")]
    speech_tail_percent: f64,
    #[serde(rename = "speechScale")]
    speech_scale: &'static str,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct I18nText {
    settings_dialog_title: String,
    settings_load_failed: String,
    settings_save_failed: String,
    settings_path_unavailable: String,
    settings_invalid_data: String,
    size_menu: String,
    size_small: String,
    size_medium: String,
    size_large: String,
    speed_menu: String,
    speed_slow: String,
    speed_normal: String,
    speed_fast: String,
    activity_prefix: String,
    activity_work: String,
    activity_slacking: String,
    language_menu: String,
    language_zh: String,
    language_en: String,
    talk_when_stopped: String,
    version: String,
    quit: String,
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
            let speech_window = create_speech_window(app.handle())?;
            configure_overlay_window(&window)?;
            configure_overlay_window(&speech_window)?;

            shared.lock().expect("state lock").config = load_config_or_default(app.handle());

            create_tray(app.handle(), shared.clone())?;
            initialize_window(app.handle(), &window, &speech_window, &shared)?;
            start_motion_loop(app.handle().clone(), window, speech_window, shared.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running desktop pet");
}

impl AppState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            config: PetConfig::default(),
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
            stopped_mood: "calm",
            stopped_shapeshift: false,
            next_stopped_mood_change_at: now
                + Duration::from_millis(STOPPED_SHAPESHIFT_INTERVAL_MS),
        }
    }

    fn set_behavior(&mut self, behavior: Behavior) {
        let now = Instant::now();
        self.behavior = behavior;
        self.behavior_started_at = now;
        match behavior {
            Behavior::Stopped => {
                self.stopped_shapeshift = rand::thread_rng().gen_bool(STOPPED_SHAPESHIFT_CHANCE);
                self.choose_stopped_mood();
                self.next_stopped_mood_change_at =
                    now + Duration::from_millis(STOPPED_SHAPESHIFT_INTERVAL_MS);
                self.next_behavior_change_at =
                    now + random_duration(STOPPED_MIN_MS, STOPPED_MAX_MS);
            }
            Behavior::Patrol => {
                self.stopped_shapeshift = false;
                self.next_behavior_change_at = now + random_duration(PATROL_MIN_MS, PATROL_MAX_MS);
            }
            _ => {
                self.stopped_shapeshift = false;
            }
        }
    }

    fn choose_stopped_mood(&mut self) {
        self.stopped_mood = match self.config.activity {
            Activity::Work => {
                pick_weighted(&[("happy", 1), ("calm", 9), ("angry", 60), ("sorrow", 30)])
            }
            Activity::Slacking => {
                pick_weighted(&[("happy", 40), ("calm", 40), ("angry", 5), ("sorrow", 15)])
            }
        };
    }
}

impl Default for PetConfig {
    fn default() -> Self {
        Self {
            size: SizeProfile::Small,
            speed: SpeedProfile::Fast,
            activity: Activity::Work,
            language: Language::default(),
            talk_when_stopped: default_talk_when_stopped(),
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::Zh
    }
}

fn default_talk_when_stopped() -> bool {
    true
}

fn initialize_window(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    speech_window: &tauri::WebviewWindow,
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
    window.set_size(Size::Logical(pet_window_size(state.config.size)))?;
    window.set_position(Position::Logical(pet_window_position(&state)))?;
    let speech_layout = speech_layout(&state, bounds);
    speech_window.set_size(Size::Logical(speech_window_size(state.config.size)))?;
    speech_window.set_position(Position::Logical(speech_layout.position))?;
    window.show()?;
    speech_window.show()?;
    Ok(())
}

fn configure_overlay_window(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.set_ignore_cursor_events(true)?;
    window.set_always_on_top(true)?;
    let _ = window.set_visible_on_all_workspaces(true);
    Ok(())
}

fn create_speech_window(app: &tauri::AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window("speech") {
        return Ok(window);
    }

    WebviewWindowBuilder::new(app, "speech", WebviewUrl::App("speech.html".into()))
        .title("Desktop Pet Speech")
        .inner_size(160.0, 58.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .visible(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .build()
}

fn settings_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(SETTINGS_FILE_NAME))
}

fn load_config_or_default(app: &tauri::AppHandle) -> PetConfig {
    match load_config(app) {
        Ok(Some(config)) => config,
        Ok(None) => PetConfig::default(),
        Err(error) => {
            let text = i18n_text(Language::default());
            let reset = MessageDialog::new()
                .set_level(MessageLevel::Warning)
                .set_title(&text.settings_dialog_title)
                .set_description(text.settings_load_failed.replace("{error}", &error))
                .set_buttons(MessageButtons::YesNo)
                .show();

            let config = PetConfig::default();
            if reset == MessageDialogResult::Yes {
                save_config(app, config);
            }
            config
        }
    }
}

fn load_config(app: &tauri::AppHandle) -> Result<Option<PetConfig>, String> {
    let text = i18n_text(Language::default());
    let path = settings_path(app).ok_or(text.settings_path_unavailable)?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };

    serde_json::from_str(&content).map(Some).map_err(|error| {
        text.settings_invalid_data
            .replace("{path}", &path.display().to_string())
            .replace("{error}", &error.to_string())
    })
}

fn save_config(app: &tauri::AppHandle, config: PetConfig) {
    if let Err(error) = try_save_config(app, config) {
        let text = i18n_text(config.language);
        show_settings_error(
            text.settings_dialog_title,
            text.settings_save_failed.replace("{error}", &error),
        );
    }
}

fn try_save_config(app: &tauri::AppHandle, config: PetConfig) -> Result<(), String> {
    let text = i18n_text(config.language);
    let path = settings_path(app).ok_or(text.settings_path_unavailable)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?;
    fs::write(&path, content).map_err(|error| format!("{}: {error}", path.display()))
}

fn show_settings_error(title: String, description: String) {
    let _ = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title(title)
        .set_description(description)
        .set_buttons(MessageButtons::Ok)
        .show();
}

fn start_motion_loop(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    speech_window: tauri::WebviewWindow,
    shared: SharedState,
) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(TICK_MS));
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let cursor = app
            .cursor_position()
            .map(|position| {
                let logical = position.to_logical::<f64>(scale_factor);
                (logical.x, logical.y)
            })
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
            let speech_layout = speech_layout(&state, bounds);
            let _ = window.set_size(Size::Logical(pet_window_size(state.config.size)));
            let _ = window.set_position(Position::Logical(pet_window_position(&state)));
            let _ = speech_window.set_size(Size::Logical(speech_window_size(state.config.size)));
            let _ = speech_window.set_position(Position::Logical(speech_layout.position));
            let _ = window.set_always_on_top(true);
            let _ = speech_window.set_always_on_top(true);
            build_renderer_state(&state, speech_layout.placement, speech_layout.tail_percent)
        };

        let _ = window.emit("pet-state", renderer_state.clone());
        let _ = speech_window.emit("pet-state", renderer_state);
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
        if state.behavior == Behavior::Stopped && now > state.next_behavior_change_at {
            state.set_behavior(Behavior::Starting);
        } else if state.behavior == Behavior::Stopped
            && state.stopped_shapeshift
            && now > state.next_stopped_mood_change_at
        {
            state.choose_stopped_mood();
            state.next_stopped_mood_change_at =
                now + Duration::from_millis(STOPPED_SHAPESHIFT_INTERVAL_MS);
        } else if state.behavior == Behavior::Starting && elapsed > START_DURATION_MS as f64 {
            state.set_behavior(Behavior::Patrol);
        } else if state.behavior == Behavior::Patrol && now > state.next_behavior_change_at {
            state.set_behavior(Behavior::Stopping);
        } else if state.behavior == Behavior::Stopping && elapsed > STOP_DURATION_MS as f64 {
            state.set_behavior(Behavior::Stopped);
        }

        elapsed = now.duration_since(state.behavior_started_at).as_millis() as f64;
        let patrol_force = match state.behavior {
            Behavior::Starting => {
                PATROL_ACCELERATION
                    * speed_multiplier
                    * ease_in_out(elapsed / START_DURATION_MS as f64)
            }
            Behavior::Patrol => PATROL_ACCELERATION * speed_multiplier,
            Behavior::Stopping => {
                let stop_progress = ease_in_out(elapsed / STOP_DURATION_MS as f64);
                state.pet.vx *= 1.0 - stop_progress * 0.09;
                PATROL_ACCELERATION * speed_multiplier * (1.0 - stop_progress) * 0.45
            }
            Behavior::Stopped => {
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
        (PATROL_SPEED * speed_multiplier * 0.35).max(
            MAX_PATROL_SPEED
                * speed_multiplier
                * ease_in_out(behavior_elapsed / START_DURATION_MS as f64),
        )
    } else if state.behavior == Behavior::Stopping || state.behavior == Behavior::Stopped {
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

fn build_renderer_state(
    state: &AppState,
    speech_placement: &'static str,
    speech_tail_percent: f64,
) -> RendererState {
    RendererState {
        facing: state.pet.facing,
        moving: state.pet.moving,
        behavior: if state.was_escaping {
            "escaping"
        } else {
            behavior_name(state.behavior)
        },
        stopped_mood: state.stopped_mood,
        stopped_shapeshift: state.stopped_shapeshift,
        activity: activity_name(state.config.activity),
        size: pet_size(state.config.size),
        base_size: BASE_PET_SIZE,
        speed: hypot(state.pet.vx, state.pet.vy),
        language: language_name(state.config.language),
        talk_when_stopped: state.config.talk_when_stopped,
        speech_placement,
        speech_tail_percent,
        speech_scale: speech_scale_name(state.config.size),
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

fn build_tray_menu(
    app: &tauri::AppHandle,
    shared: &SharedState,
) -> tauri::Result<Menu<tauri::Wry>> {
    let config = shared.lock().expect("state lock").config;
    let text = i18n_text(config.language);
    let small = CheckMenuItem::with_id(
        app,
        "size:small",
        &text.size_small,
        true,
        config.size == SizeProfile::Small,
        None::<&str>,
    )?;
    let medium = CheckMenuItem::with_id(
        app,
        "size:medium",
        &text.size_medium,
        true,
        config.size == SizeProfile::Medium,
        None::<&str>,
    )?;
    let large = CheckMenuItem::with_id(
        app,
        "size:large",
        &text.size_large,
        true,
        config.size == SizeProfile::Large,
        None::<&str>,
    )?;
    let size_menu = Submenu::with_items(app, &text.size_menu, true, &[&small, &medium, &large])?;

    let slow = CheckMenuItem::with_id(
        app,
        "speed:slow",
        &text.speed_slow,
        true,
        config.speed == SpeedProfile::Slow,
        None::<&str>,
    )?;
    let normal = CheckMenuItem::with_id(
        app,
        "speed:normal",
        &text.speed_normal,
        true,
        config.speed == SpeedProfile::Normal,
        None::<&str>,
    )?;
    let fast = CheckMenuItem::with_id(
        app,
        "speed:fast",
        &text.speed_fast,
        true,
        config.speed == SpeedProfile::Fast,
        None::<&str>,
    )?;
    let speed_menu = Submenu::with_items(app, &text.speed_menu, true, &[&slow, &normal, &fast])?;

    let work = CheckMenuItem::with_id(
        app,
        "activity:work",
        &text.activity_work,
        true,
        config.activity == Activity::Work,
        None::<&str>,
    )?;
    let slacking = CheckMenuItem::with_id(
        app,
        "activity:slacking",
        &text.activity_slacking,
        true,
        config.activity == Activity::Slacking,
        None::<&str>,
    )?;
    let activity_label = text
        .activity_prefix
        .replace("{activity}", activity_label(config.activity, &text));
    let activity_menu = Submenu::with_items(app, activity_label, true, &[&work, &slacking])?;

    let zh = CheckMenuItem::with_id(
        app,
        "language:zh",
        &text.language_zh,
        true,
        config.language == Language::Zh,
        None::<&str>,
    )?;
    let en = CheckMenuItem::with_id(
        app,
        "language:en",
        &text.language_en,
        true,
        config.language == Language::En,
        None::<&str>,
    )?;
    let language_menu = Submenu::with_items(app, &text.language_menu, true, &[&zh, &en])?;

    let talk_when_stopped = CheckMenuItem::with_id(
        app,
        "talk:stopped",
        &text.talk_when_stopped,
        true,
        config.talk_when_stopped,
        None::<&str>,
    )?;
    let version = MenuItem::with_id(
        app,
        "version",
        format!("{} v{}", text.version, env!("CARGO_PKG_VERSION")),
        false,
        None::<&str>,
    )?;

    let quit = MenuItem::with_id(app, "quit", text.quit, true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &size_menu,
            &speed_menu,
            &activity_menu,
            &language_menu,
            &talk_when_stopped,
            &PredefinedMenuItem::separator(app)?,
            &version,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

fn handle_menu_event(app: &tauri::AppHandle, id: &str, shared: &SharedState) {
    match id {
        "quit" => app.exit(0),
        "size:small" | "size:medium" | "size:large" => {
            let next_size = match id {
                "size:small" => SizeProfile::Small,
                "size:medium" => SizeProfile::Medium,
                _ => SizeProfile::Large,
            };
            apply_size(app, shared, next_size);
            save_current_config(app, shared);
            refresh_tray_menu(app, shared);
        }
        "speed:slow" | "speed:normal" | "speed:fast" => {
            let next_speed = match id {
                "speed:slow" => SpeedProfile::Slow,
                "speed:normal" => SpeedProfile::Normal,
                _ => SpeedProfile::Fast,
            };
            shared.lock().expect("state lock").config.speed = next_speed;
            save_current_config(app, shared);
            refresh_tray_menu(app, shared);
        }
        "activity:work" | "activity:slacking" => {
            let mut state = shared.lock().expect("state lock");
            state.config.activity = if id == "activity:work" {
                Activity::Work
            } else {
                Activity::Slacking
            };
            if state.behavior == Behavior::Stopped {
                state.choose_stopped_mood();
            }
            drop(state);
            save_current_config(app, shared);
            refresh_tray_menu(app, shared);
        }
        "language:zh" | "language:en" => {
            shared.lock().expect("state lock").config.language = if id == "language:zh" {
                Language::Zh
            } else {
                Language::En
            };
            save_current_config(app, shared);
            refresh_tray_menu(app, shared);
        }
        "talk:stopped" => {
            let mut state = shared.lock().expect("state lock");
            state.config.talk_when_stopped = !state.config.talk_when_stopped;
            drop(state);
            save_current_config(app, shared);
            refresh_tray_menu(app, shared);
        }
        _ => {}
    }
}

fn save_current_config(app: &tauri::AppHandle, shared: &SharedState) {
    let config = shared.lock().expect("state lock").config;
    save_config(app, config);
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
        let _ = window.set_size(Size::Logical(pet_window_size(next_size)));
    }
}

fn pet_window_size(size: SizeProfile) -> LogicalSize<f64> {
    let pet_size = pet_size(size) as f64;
    LogicalSize {
        width: pet_size,
        height: pet_size,
    }
}

fn pet_window_position(state: &AppState) -> LogicalPosition<f64> {
    LogicalPosition {
        x: state.pet.x.round(),
        y: state.pet.y.round(),
    }
}

#[derive(Clone, Copy)]
struct SpeechLayout {
    position: LogicalPosition<f64>,
    placement: &'static str,
    tail_percent: f64,
}

fn speech_window_size(size: SizeProfile) -> LogicalSize<f64> {
    speech_metrics(size).window_size
}

struct SpeechMetrics {
    window_size: LogicalSize<f64>,
    gap: f64,
    facing_offset: f64,
}

fn speech_metrics(size: SizeProfile) -> SpeechMetrics {
    match size {
        SizeProfile::Small => SpeechMetrics {
            window_size: LogicalSize {
                width: 160.0,
                height: 58.0,
            },
            gap: -2.0,
            facing_offset: 24.0,
        },
        SizeProfile::Medium => SpeechMetrics {
            window_size: LogicalSize {
                width: 196.0,
                height: 68.0,
            },
            gap: 0.0,
            facing_offset: 42.0,
        },
        SizeProfile::Large => SpeechMetrics {
            window_size: LogicalSize {
                width: 236.0,
                height: 82.0,
            },
            gap: 2.0,
            facing_offset: 68.0,
        },
    }
}

fn speech_layout(state: &AppState, bounds: Bounds) -> SpeechLayout {
    let pet_size = pet_size(state.config.size) as f64;
    let metrics = speech_metrics(state.config.size);
    let speech_width = metrics.window_size.width;
    let speech_height = metrics.window_size.height;
    let facing_offset = if state.pet.facing == "right" {
        metrics.facing_offset
    } else {
        -metrics.facing_offset
    };
    let pet_center_x = state.pet.x + pet_size / 2.0;
    let preferred_x = pet_center_x + facing_offset - speech_width / 2.0;
    let min_x = bounds.x + EDGE_PADDING;
    let max_x = bounds.x + bounds.width - speech_width - EDGE_PADDING;
    let x = preferred_x.clamp(min_x, max_x);
    let above_y = state.pet.y - speech_height - metrics.gap;
    let below_y = state.pet.y + pet_size + metrics.gap;
    let tail_percent = ((pet_center_x - x) / speech_width * 100.0)
        .clamp(SPEECH_TAIL_MIN_PERCENT, SPEECH_TAIL_MAX_PERCENT);

    let (y, placement) = if above_y >= bounds.y + EDGE_PADDING {
        (above_y, "above")
    } else {
        (
            below_y.min(bounds.y + bounds.height - speech_height - EDGE_PADDING),
            "below",
        )
    };

    SpeechLayout {
        position: LogicalPosition {
            x: x.round(),
            y: y.round(),
        },
        placement,
        tail_percent,
    }
}

fn refresh_tray_menu(app: &tauri::AppHandle, shared: &SharedState) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_tray_menu(app, shared)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn virtual_work_area(app: &tauri::AppHandle) -> Option<Bounds> {
    let monitors = app.available_monitors().ok()?;
    let mut areas = monitors.iter().map(|monitor| {
        let scale_factor = monitor.scale_factor();
        let area = monitor.work_area();
        let position = area.position.to_logical::<f64>(scale_factor);
        let size = area.size.to_logical::<f64>(scale_factor);
        Bounds {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    });
    let first = areas.next()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width;
    let mut max_y = first.y + first.height;

    for area in areas {
        min_x = min_x.min(area.x);
        min_y = min_y.min(area.y);
        max_x = max_x.max(area.x + area.width);
        max_y = max_y.max(area.y + area.height);
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

fn activity_label<'a>(activity: Activity, text: &'a I18nText) -> &'a str {
    match activity {
        Activity::Work => &text.activity_work,
        Activity::Slacking => &text.activity_slacking,
    }
}

fn i18n_text(language: Language) -> I18nText {
    let parsed: serde_json::Value = serde_json::from_str(I18N_JSON).unwrap_or_else(|_| {
        serde_json::json!({
            "zh": fallback_i18n_text(Language::Zh),
            "en": fallback_i18n_text(Language::En)
        })
    });
    let key = language_name(language);
    parsed
        .get(key)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(|| fallback_i18n_text(language))
}

fn fallback_i18n_text(language: Language) -> I18nText {
    match language {
        Language::Zh => I18nText {
            settings_dialog_title: "Desktop Pet 设置".to_string(),
            settings_load_failed: "设置读取失败：\n{error}\n\n是否重置为默认设置？".to_string(),
            settings_save_failed: "设置保存失败：\n{error}".to_string(),
            settings_path_unavailable: "无法解析设置文件路径。".to_string(),
            settings_invalid_data: "{path} 包含无效设置数据：{error}".to_string(),
            size_menu: "大小".to_string(),
            size_small: "小".to_string(),
            size_medium: "中".to_string(),
            size_large: "大".to_string(),
            speed_menu: "运动速度".to_string(),
            speed_slow: "慢".to_string(),
            speed_normal: "正常".to_string(),
            speed_fast: "快".to_string(),
            activity_prefix: "我在{activity}".to_string(),
            activity_work: "工作".to_string(),
            activity_slacking: "摸鱼".to_string(),
            language_menu: "语言".to_string(),
            language_zh: "中文".to_string(),
            language_en: "English".to_string(),
            talk_when_stopped: "停止时说话".to_string(),
            version: "版本".to_string(),
            quit: "退出".to_string(),
        },
        Language::En => I18nText {
            settings_dialog_title: "Desktop Pet Settings".to_string(),
            settings_load_failed:
                "Settings could not be loaded:\n{error}\n\nReset settings to defaults?".to_string(),
            settings_save_failed: "Settings could not be saved:\n{error}".to_string(),
            settings_path_unavailable: "Could not resolve settings path.".to_string(),
            settings_invalid_data: "{path} contains invalid settings data: {error}".to_string(),
            size_menu: "Size".to_string(),
            size_small: "Small".to_string(),
            size_medium: "Medium".to_string(),
            size_large: "Large".to_string(),
            speed_menu: "Movement Speed".to_string(),
            speed_slow: "Slow".to_string(),
            speed_normal: "Normal".to_string(),
            speed_fast: "Fast".to_string(),
            activity_prefix: "Mode: {activity}".to_string(),
            activity_work: "Work".to_string(),
            activity_slacking: "Slacking".to_string(),
            language_menu: "Language".to_string(),
            language_zh: "中文".to_string(),
            language_en: "English".to_string(),
            talk_when_stopped: "Talk When Stopped".to_string(),
            version: "Version".to_string(),
            quit: "Quit".to_string(),
        },
    }
}

fn speech_scale_name(size: SizeProfile) -> &'static str {
    match size {
        SizeProfile::Small => "small",
        SizeProfile::Medium => "medium",
        SizeProfile::Large => "large",
    }
}

fn language_name(language: Language) -> &'static str {
    match language {
        Language::Zh => "zh",
        Language::En => "en",
    }
}

fn behavior_name(behavior: Behavior) -> &'static str {
    match behavior {
        Behavior::Stopped => "stopped",
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

use crate::config::{
    AppSettings, GamepadButton, GamepadConfig, ImageSizingMode, KeyConfig, LastPageAction,
    MangaAction, MouseButton, MouseConfig, MouseGesture, PageViewOptions, ResizeMethod, Shortcut,
    SourceMode, UiLanguage,
};
use crate::font;
use crate::localize::{set_language, tr};
use crate::utils::{
    action_label, gamepad_action_for_button, gamepad_button_index, gamepad_button_label,
    get_adjacent_directories, is_repeatable_gamepad_action, language_label, map_gamepad_button,
    mouse_button_index, mouse_button_to_pointer, render_mouse_action_dropdown, scan_folder,
    separator_pct, strip_adobe_app14_if_invalid, windows_natural_sort_strings,
};
#[path = "overlay.rs"]
mod overlay;
#[path = "pageview.rs"]
mod pageview;
use eframe::egui;
use egui::Rect;
use gilrs::{EventType, Gilrs};
use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, ImageFormat};
use pdfium_render::prelude::{PdfDocument, Pdfium, Pixels};
use std::env;
use std::fs::{self, File};
use std::io::Cursor;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

const LONG_CLICK_DURATION: Duration = Duration::from_millis(450);
const GAMEPAD_INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(800);
const MOUSE_DRAG_THRESHOLD: f32 = 6.0;
const SETTINGS_TOGGLE_FADE_DURATION: Duration = Duration::from_secs(1);
const AUTO_SCROLL_MIN_DELAY_MS: u64 = 200;
const AUTO_SCROLL_MAX_DELAY_MS: u64 = 30_000;
const ANIMATION_SCAN_AHEAD_PAGES: usize = 5;
const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 3.0;
const ZOOM_STEP: f32 = 1.1;
const ZOOM_RELOAD_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq)]
enum ControlProfile {
    Default,
    TopDown,
}

fn control_profile_header_suffix(profile: ControlProfile) -> &'static str {
    match profile {
        ControlProfile::Default => tr("settings.control.mode_default"),
        ControlProfile::TopDown => tr("settings.control.mode_top_down"),
    }
}

fn control_mapping_header(base_label: &'static str, profile: ControlProfile) -> String {
    format!(
        "{} ({})",
        tr(base_label),
        control_profile_header_suffix(profile)
    )
}

fn settings_path() -> PathBuf {
    if is_packaged_app() {
        let base = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("RustMangaReader");
        let _ = fs::create_dir_all(&dir);
        return dir.join("settings.json");
    }

    let mut exe_path = env::current_exe().expect("Failed to get current exe path");
    exe_path.pop();
    exe_path.push("settings.json");
    exe_path
}

fn is_packaged_app() -> bool {
    let mut length = 0;
    let result = unsafe { GetCurrentPackageFullName(&mut length, None) };
    result != APPMODEL_ERROR_NO_PACKAGE
}

#[derive(Clone, PartialEq)]
enum BindingTarget {
    Keyboard {
        profile: ControlProfile,
        action: String,
    },
}

#[derive(Clone)]
struct AnimatedFrame {
    image: egui::ColorImage,
    delay: Duration,
}

enum AnimationDecodeEvent {
    Frame(AnimatedFrame),
    Finished,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimatedImageFormat {
    WebP,
    Gif,
    Png,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimationScanStatus {
    Unknown,
    Animated(AnimatedImageFormat),
    NotAnimated,
}

enum AnimationScanEvent {
    Result {
        generation: u64,
        filename: String,
        status: AnimationScanStatus,
    },
    Finished {
        generation: u64,
    },
}

#[derive(Clone, Copy)]
struct ImageProcessingSpec {
    resize_method: ResizeMethod,
    image_sizing_mode: ImageSizingMode,
    transparency_support: bool,
    zoom_factor: f32,
    container_size: [f32; 2],
}

struct AnimatedPage {
    frames: Vec<AnimatedFrame>,
    current_frame: usize,
    next_frame_at: Instant,
    decode_complete: bool,
    decode_rx: Option<Receiver<AnimationDecodeEvent>>,
}

fn render_binding_button(
    ui: &mut egui::Ui,
    id: &str,
    shortcut: &mut Option<Shortcut>,
    binding: &mut Option<BindingTarget>,
    profile: ControlProfile,
    listening_text: &str,
) {
    let is_binding = matches!(
        binding,
        Some(BindingTarget::Keyboard { profile: binding_profile, action })
            if *binding_profile == profile && action == id
    );
    let text = if is_binding {
        listening_text.to_string()
    } else {
        shortcut
            .map(|shortcut| shortcut.format())
            .unwrap_or_else(|| tr("common.unassigned").to_owned())
    };

    if ui.button(text).clicked() {
        *binding = Some(BindingTarget::Keyboard {
            profile,
            action: id.to_string(),
        });
    }
}

fn gamepad_action_mut(config: &mut GamepadConfig, button: GamepadButton) -> &mut MangaAction {
    match button {
        GamepadButton::South => &mut config.south,
        GamepadButton::East => &mut config.east,
        GamepadButton::North => &mut config.north,
        GamepadButton::West => &mut config.west,
        GamepadButton::LeftTrigger => &mut config.left_trigger,
        GamepadButton::LeftTrigger2 => &mut config.left_trigger2,
        GamepadButton::RightTrigger => &mut config.right_trigger,
        GamepadButton::RightTrigger2 => &mut config.right_trigger2,
        GamepadButton::Select => &mut config.select,
        GamepadButton::Start => &mut config.start,
        GamepadButton::LeftThumb => &mut config.left_thumb,
        GamepadButton::RightThumb => &mut config.right_thumb,
        GamepadButton::DPadUp => &mut config.dpad_up,
        GamepadButton::DPadDown => &mut config.dpad_down,
        GamepadButton::DPadLeft => &mut config.dpad_left,
        GamepadButton::DPadRight => &mut config.dpad_right,
    }
}

fn render_control_mapping_settings(
    ui: &mut egui::Ui,
    profile: ControlProfile,
    keys: &mut KeyConfig,
    mouse: &mut MouseConfig,
    gamepad: &mut GamepadConfig,
    double_click_threshold_ms: &mut u64,
    binding_action: &mut Option<BindingTarget>,
) -> bool {
    let mut changed = false;
    let listening_text = tr("common.listening").to_owned();

    egui::CollapsingHeader::new(
        egui::RichText::new(control_mapping_header("settings.key_config", profile))
            .size(20.0)
            .strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        separator_pct(ui);
        egui::Grid::new(format!("key_grid_{profile:?}"))
            .num_columns(2)
            .spacing([20.0, 10.0])
            .show(ui, |ui| {
                for (label, id, shortcut) in [
                    (
                        tr("settings.key.slide_image_down"),
                        "Slide Image Down",
                        &mut keys.slide_image_down,
                    ),
                    (
                        tr("settings.key.slide_image_up"),
                        "Slide Image Up",
                        &mut keys.slide_image_up,
                    ),
                    (
                        tr("settings.key.toggle_auto_scroll"),
                        "Toggle Auto Scroll",
                        &mut keys.toggle_auto_scroll,
                    ),
                    (
                        tr("settings.key.reload_current_image"),
                        "Reload Current Image",
                        &mut keys.reload_current_image,
                    ),
                    (
                        tr("settings.key.next_page"),
                        "Next Page",
                        &mut keys.next_page,
                    ),
                    (
                        tr("settings.key.prev_page"),
                        "Previous Page",
                        &mut keys.prev_page,
                    ),
                    (
                        tr("settings.key.one_next_page"),
                        "1 Next Page",
                        &mut keys.one_next_page,
                    ),
                    (
                        tr("settings.key.one_prev_page"),
                        "1 Prev Page",
                        &mut keys.one_prev_page,
                    ),
                    (
                        tr("settings.key.first_page"),
                        "First Page",
                        &mut keys.first_page,
                    ),
                    (
                        tr("settings.key.last_page"),
                        "Last Page",
                        &mut keys.last_page,
                    ),
                    (
                        tr("settings.key.next_file"),
                        "Next File",
                        &mut keys.next_file,
                    ),
                    (
                        tr("settings.key.prev_file"),
                        "Previous File",
                        &mut keys.prev_file,
                    ),
                    (
                        tr("settings.key.next_folder"),
                        "Next Folder",
                        &mut keys.next_folder,
                    ),
                    (
                        tr("settings.key.prev_folder"),
                        "Previous Folder",
                        &mut keys.prev_folder,
                    ),
                    (
                        tr("settings.key.fullscreen"),
                        "Toggle Fullscreen",
                        &mut keys.fullscreen,
                    ),
                    (
                        tr("settings.key.view_mode"),
                        "View Mode",
                        &mut keys.view_mode,
                    ),
                    (
                        tr("settings.key.open_file"),
                        "Open File",
                        &mut keys.open_file,
                    ),
                    (tr("settings.key.quit_app"), "Quit App", &mut keys.quit_app),
                ] {
                    ui.label(label);
                    render_binding_button(
                        ui,
                        id,
                        shortcut,
                        binding_action,
                        profile,
                        &listening_text,
                    );
                    ui.end_row();
                }
            });
        separator_pct(ui);
    });

    ui.add_space(20.0);
    egui::CollapsingHeader::new(
        egui::RichText::new(control_mapping_header("settings.mouse_mapping", profile))
            .size(20.0)
            .strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        separator_pct(ui);
        ui.label(tr("settings.mouse_mapping.description"));
        ui.add_space(10.0);
        changed |= ui
            .add(
                egui::Slider::new(double_click_threshold_ms, 100..=1000)
                    .text(tr("settings.double_click_threshold")),
            )
            .changed();
        ui.add_space(10.0);
        let action_options: Vec<(MangaAction, String)> = MangaAction::ALL
            .into_iter()
            .map(|action| (action, action_label(action).to_owned()))
            .collect();
        let unassigned_label = tr("common.unassigned").to_owned();
        egui::Grid::new(format!("mouse_grid_{profile:?}"))
            .num_columns(2)
            .spacing([10.0, 10.0])
            .show(ui, |ui| {
                for (label, id, action) in [
                    (
                        tr("settings.mouse.scroll_up"),
                        "mouse_scroll_up",
                        &mut mouse.scroll_up,
                    ),
                    (
                        tr("settings.mouse.scroll_down"),
                        "mouse_scroll_down",
                        &mut mouse.scroll_down,
                    ),
                    (
                        tr("settings.mouse.button1_click"),
                        "mouse_button1_click",
                        &mut mouse.button1_click,
                    ),
                    (
                        tr("settings.mouse.button2_click"),
                        "mouse_button2_click",
                        &mut mouse.button2_click,
                    ),
                    (
                        tr("settings.mouse.button3_click"),
                        "mouse_button3_click",
                        &mut mouse.button3_click,
                    ),
                    (
                        tr("settings.mouse.button4_click"),
                        "mouse_button4_click",
                        &mut mouse.button4_click,
                    ),
                    (
                        tr("settings.mouse.button5_click"),
                        "mouse_button5_click",
                        &mut mouse.button5_click,
                    ),
                    (
                        tr("settings.mouse.button1_double_click"),
                        "mouse_button1_double_click",
                        &mut mouse.button1_double_click,
                    ),
                    (
                        tr("settings.mouse.button2_double_click"),
                        "mouse_button2_double_click",
                        &mut mouse.button2_double_click,
                    ),
                    (
                        tr("settings.mouse.button3_double_click"),
                        "mouse_button3_double_click",
                        &mut mouse.button3_double_click,
                    ),
                    (
                        tr("settings.mouse.button4_double_click"),
                        "mouse_button4_double_click",
                        &mut mouse.button4_double_click,
                    ),
                    (
                        tr("settings.mouse.button5_double_click"),
                        "mouse_button5_double_click",
                        &mut mouse.button5_double_click,
                    ),
                    (
                        tr("settings.mouse.button1_long_click"),
                        "mouse_button1_long_click",
                        &mut mouse.button1_long_click,
                    ),
                    (
                        tr("settings.mouse.button2_long_click"),
                        "mouse_button2_long_click",
                        &mut mouse.button2_long_click,
                    ),
                    (
                        tr("settings.mouse.button3_long_click"),
                        "mouse_button3_long_click",
                        &mut mouse.button3_long_click,
                    ),
                    (
                        tr("settings.mouse.button4_long_click"),
                        "mouse_button4_long_click",
                        &mut mouse.button4_long_click,
                    ),
                    (
                        tr("settings.mouse.button5_long_click"),
                        "mouse_button5_long_click",
                        &mut mouse.button5_long_click,
                    ),
                ] {
                    ui.label(label);
                    changed |= render_mouse_action_dropdown(
                        ui,
                        &format!("{id}_{profile:?}"),
                        action,
                        &action_options,
                        &unassigned_label,
                    );
                    ui.end_row();
                }
            });
        separator_pct(ui);
    });

    ui.add_space(20.0);
    egui::CollapsingHeader::new(
        egui::RichText::new(control_mapping_header("settings.gamepad_mapping", profile))
            .size(20.0)
            .strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        separator_pct(ui);
        ui.label(tr("settings.gamepad_mapping.description"));
        ui.add_space(10.0);
        let action_options: Vec<(MangaAction, String)> = MangaAction::ALL
            .into_iter()
            .map(|action| (action, action_label(action).to_owned()))
            .collect();
        let unassigned_label = tr("common.unassigned").to_owned();
        egui::Grid::new(format!("gamepad_grid_{profile:?}"))
            .num_columns(2)
            .spacing([10.0, 10.0])
            .show(ui, |ui| {
                for button in GamepadButton::ALL {
                    ui.label(gamepad_button_label(button));
                    changed |= render_mouse_action_dropdown(
                        ui,
                        &format!("gamepad_{profile:?}_{button:?}"),
                        gamepad_action_mut(gamepad, button),
                        &action_options,
                        &unassigned_label,
                    );
                    ui.end_row();
                }
            });
        separator_pct(ui);
    });

    changed
}

pub struct MangaReader {
    zip_path: Option<PathBuf>,
    image_files: Vec<String>,
    current_index: usize,
    textures: [Option<egui::TextureHandle>; 2],
    buffer_next: [Option<egui::TextureHandle>; 2],
    buffer_prev: [Option<egui::TextureHandle>; 2],
    top_down_textures: Vec<Option<egui::TextureHandle>>,
    top_down_scroll_offset: f32,
    last_buffered_index: Option<usize>,
    all_zips_in_folder: Vec<PathBuf>,
    error_msg: Option<(String, Instant)>,
    is_fullscreen: bool,
    can_scroll: bool,
    dialog_rx: Receiver<Option<PathBuf>>,
    dialog_tx: Sender<Option<PathBuf>>,
    page_indicator_time: Option<Instant>,
    last_dialog_time: std::time::Instant,
    is_dialog_open: bool,
    zip_name_display: Option<(String, Instant)>,
    is_shifted: bool,
    config: AppSettings,
    binding_action: Option<BindingTarget>,
    texture_cache: std::collections::HashMap<String, egui::TextureHandle>,
    active_animations: [Option<AnimatedPage>; 2],
    active_animation_keys: [Option<String>; 2],
    animated_image_byte_cache: std::collections::HashMap<String, Vec<u8>>,
    animated_image_first_frame_cache: std::collections::HashMap<String, AnimatedFrame>,
    animation_scan_anchor: Option<usize>,
    animation_scan_generation: u64,
    animation_scan_epoch: Arc<AtomicU64>,
    animation_scan_rx: Option<Receiver<AnimationScanEvent>>,
    animation_scan_status: std::collections::HashMap<String, AnimationScanStatus>,
    pdfium: Option<&'static Pdfium>,
    pdf_document: Option<PdfDocument<'static>>,
    pdf_document_path: Option<PathBuf>,
    initial_path: Option<PathBuf>,
    source_mode: SourceMode,
    last_image_switch_time: Instant,
    auto_scroll_enabled: bool,
    auto_scroll_next_tick: Option<Instant>,
    zoom_factor: f32,
    pan_offset: egui::Vec2,
    last_page_view_rect: Option<egui::Rect>,
    last_zoom_change_at: Option<Instant>,
    zoom_reload_pending: bool,
    is_scrubbing: bool,
    mouse_press_started: [Option<Instant>; 5],
    mouse_press_origin: [Option<egui::Pos2>; 5],
    mouse_long_triggered: [bool; 5],
    mouse_drag_suppressed: [bool; 5],
    pending_mouse_click: [Option<(Instant, MangaAction)>; 5],
    gamepad: Option<Gilrs>,
    gamepad_repeat_deadlines: [Option<Instant>; 16],
    selected_control_profile: ControlProfile,
    settings_toggle_last_visible_at: Instant,
}

impl MangaReader {
    pub fn new(_cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        font::setup_custom_fonts(&_cc.egui_ctx);
        let exe_path = settings_path();
        eprintln!("Loading setting from : {:?}", exe_path.to_str());
        let config: AppSettings = if let Ok(data) = std::fs::read_to_string(exe_path) {
            // add |_| here to accept the error argument but ignore it
            serde_json::from_str(&data).unwrap_or_else(|e| {
                eprintln!("Error is : {:?}", e);
                eprintln!("Failed to parse settings.json, using defaults.");
                AppSettings::default()
            })
        } else {
            AppSettings::default()
        };
        set_language(config.language);

        let (tx, rx) = channel();
        Self {
            initial_path,
            zip_path: None,
            image_files: Vec::new(),
            current_index: 0,
            textures: [None, None],
            buffer_next: [None, None],
            buffer_prev: [None, None],
            top_down_textures: Vec::new(),
            top_down_scroll_offset: 0.0,
            last_buffered_index: None,
            all_zips_in_folder: Vec::new(),
            error_msg: None,
            dialog_rx: rx,
            dialog_tx: tx,
            is_fullscreen: false,
            can_scroll: true,
            page_indicator_time: None,
            last_dialog_time: Instant::now(),
            is_dialog_open: false,
            zip_name_display: None,
            is_shifted: false,
            config, // Store the loaded config here
            binding_action: None,
            texture_cache: Default::default(),
            active_animations: std::array::from_fn(|_| None),
            active_animation_keys: std::array::from_fn(|_| None),
            animated_image_byte_cache: Default::default(),
            animated_image_first_frame_cache: Default::default(),
            animation_scan_anchor: None,
            animation_scan_generation: 0,
            animation_scan_epoch: Arc::new(AtomicU64::new(0)),
            animation_scan_rx: None,
            animation_scan_status: Default::default(),
            pdfium: None,
            pdf_document: None,
            pdf_document_path: None,
            source_mode: SourceMode::Zip,
            last_image_switch_time: Instant::now(),
            auto_scroll_enabled: false,
            auto_scroll_next_tick: None,
            zoom_factor: 1.0,
            pan_offset: egui::Vec2::ZERO,
            last_page_view_rect: None,
            last_zoom_change_at: None,
            zoom_reload_pending: false,
            is_scrubbing: false,
            mouse_press_started: [None; 5],
            mouse_press_origin: [None; 5],
            mouse_long_triggered: [false; 5],
            mouse_drag_suppressed: [false; 5],
            pending_mouse_click: [None; 5],
            gamepad: Gilrs::new().ok(),
            gamepad_repeat_deadlines: [None; 16],
            selected_control_profile: ControlProfile::Default,
            settings_toggle_last_visible_at: Instant::now(),
        }
    }

    fn save_settings(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.config) {
            // write the config file in executable directory
            let exe_path = settings_path();
            let _ = std::fs::write(exe_path, json);
        }
    }

    fn auto_scroll_delay_ms(&self) -> u64 {
        self.config
            .image_delay
            .clamp(AUTO_SCROLL_MIN_DELAY_MS, AUTO_SCROLL_MAX_DELAY_MS)
    }

    fn schedule_auto_scroll(&mut self, ctx: &egui::Context) {
        let delay = Duration::from_millis(self.auto_scroll_delay_ms());
        let next_tick = Instant::now() + delay;
        self.auto_scroll_next_tick = Some(next_tick);
        ctx.request_repaint_after(delay);
    }

    fn stop_auto_scroll(&mut self) {
        self.auto_scroll_enabled = false;
        self.auto_scroll_next_tick = None;
    }

    fn set_auto_scroll_enabled(&mut self, enabled: bool, ctx: &egui::Context) {
        if enabled {
            self.auto_scroll_enabled = true;
            self.schedule_auto_scroll(ctx);
        } else {
            self.stop_auto_scroll();
        }
    }

    fn toggle_auto_scroll(&mut self, ctx: &egui::Context) {
        let enabled = !self.auto_scroll_enabled;
        self.set_auto_scroll_enabled(enabled, ctx);
    }

    fn reload_current_image(&mut self, ctx: &egui::Context) {
        if self.image_files.is_empty() {
            return;
        }

        self.stop_auto_scroll();
        self.texture_cache.clear();
        self.clear_animated_image_cache();
        self.clear_animation_scan_state();
        self.clear_active_animations();
        self.reset_buffer();
        self.reset_pan();
        self.last_buffered_index = None;

        if self.config.page_view_options == PageViewOptions::TopDown {
            self.textures = [None, None];
            self.ensure_top_down_loaded_around(self.current_index, ctx);
            let preload_count = self.image_files.len().min(5);
            for index in 0..preload_count {
                self.ensure_top_down_texture_loaded(index, ctx);
            }
        } else {
            self.textures = self.load_pair(self.current_index, ctx);
        }

        self.page_indicator_time = Some(Instant::now());
        ctx.request_repaint();
    }

    fn clear_active_animations(&mut self) {
        self.active_animations = std::array::from_fn(|_| None);
        self.active_animation_keys = std::array::from_fn(|_| None);
    }

    fn clear_animated_image_cache(&mut self) {
        self.animated_image_byte_cache.clear();
        self.animated_image_first_frame_cache.clear();
    }

    fn cancel_animation_scan(&mut self) {
        self.animation_scan_generation = self.animation_scan_generation.wrapping_add(1);
        self.animation_scan_epoch
            .store(self.animation_scan_generation, Ordering::Relaxed);
        self.animation_scan_rx = None;
        self.animation_scan_anchor = None;
    }

    fn clear_animation_scan_state(&mut self) {
        self.cancel_animation_scan();
        self.animation_scan_status.clear();
    }

    fn clear_pdf_document_cache(&mut self) {
        self.pdf_document = None;
        self.pdf_document_path = None;
    }

    fn pdfium_instance(&mut self) -> Option<&'static Pdfium> {
        if let Some(pdfium) = self.pdfium {
            return Some(pdfium);
        }

        let pdfium = Box::leak(Box::new(Pdfium::default()));
        self.pdfium = Some(pdfium);
        Some(pdfium)
    }

    fn ensure_pdf_document_loaded(&mut self, path: &Path) -> Option<usize> {
        if self.pdf_document_path.as_deref() == Some(path) {
            return self
                .pdf_document
                .as_ref()
                .map(|doc| usize::from(doc.pages().len()));
        }

        self.clear_pdf_document_cache();

        let doc = self
            .pdfium_instance()?
            .load_pdf_from_file(path, None)
            .ok()?;
        let page_count = usize::from(doc.pages().len());

        self.pdf_document = Some(doc);
        self.pdf_document_path = Some(path.to_path_buf());

        Some(page_count)
    }

    fn open_file_dialog(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_dialog_time) > std::time::Duration::from_millis(500) {
            self.last_dialog_time = now;
            if !self.is_dialog_open {
                self.is_dialog_open = true;
                let sender = self.dialog_tx.clone();
                let filter_label = tr("dialog.manga_files").to_owned();

                std::thread::spawn(move || {
                    let file = rfd::FileDialog::new()
                        .add_filter(
                            &filter_label,
                            &[
                                "zip", "cbz", "cbr", "rar", "png", "jpg", "jpeg", "bmp", "webp",
                                "gif", "tiff", "tga", "avif", "pdf",
                            ],
                        )
                        .pick_file();

                    let _ = sender.send(file);
                });
            }
        }
    }

    fn image_panel_background_color(&self) -> egui::Color32 {
        let [r, g, b, a] = self.config.image_panel_background;
        egui::Color32::from_rgba_unmultiplied(r, g, b, a)
    }

    fn reset_pan(&mut self) {
        self.pan_offset = egui::Vec2::ZERO;
    }

    fn reload_zoom_textures(&mut self, ctx: &egui::Context) {
        self.reset_buffer();
        self.texture_cache.clear();
        self.textures = self.load_pair(self.current_index, ctx);
    }

    fn schedule_zoom_texture_reload(&mut self, ctx: &egui::Context) {
        self.last_zoom_change_at = Some(Instant::now());
        self.zoom_reload_pending = true;
        ctx.request_repaint_after(ZOOM_RELOAD_DELAY);
    }

    fn reload_zoom_textures_if_settled(&mut self, ctx: &egui::Context) {
        if !self.zoom_reload_pending {
            return;
        }
        if self.zip_path.is_none() || self.image_files.is_empty() {
            self.zoom_reload_pending = false;
            self.last_zoom_change_at = None;
            return;
        }

        if let Some(last_zoom_change_at) = self.last_zoom_change_at {
            let elapsed = last_zoom_change_at.elapsed();
            if elapsed >= ZOOM_RELOAD_DELAY {
                self.reload_zoom_textures(ctx);
                self.zoom_reload_pending = false;
                self.last_zoom_change_at = None;
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(ZOOM_RELOAD_DELAY - elapsed);
            }
        }
    }

    fn apply_zoom_factor(
        &mut self,
        new_zoom_factor: f32,
        ctx: &egui::Context,
        anchor_to_pointer: bool,
    ) {
        let old_zoom_factor = self.zoom_factor;
        let new_zoom_factor = new_zoom_factor.clamp(ZOOM_MIN, ZOOM_MAX);
        if (new_zoom_factor - old_zoom_factor).abs() <= f32::EPSILON {
            return;
        }

        let rect = self
            .last_page_view_rect
            .unwrap_or_else(|| ctx.content_rect());
        if anchor_to_pointer
            && let Some(pointer_pos) = ctx.pointer_hover_pos()
            && rect.contains(pointer_pos)
        {
            let scale_ratio = new_zoom_factor / old_zoom_factor;
            if self.config.page_view_options == PageViewOptions::TopDown {
                let relative_y = pointer_pos.y - rect.center().y - self.top_down_scroll_offset;
                self.top_down_scroll_offset += relative_y * (1.0 - scale_ratio);
                self.clamp_top_down_scroll_offset();
            } else {
                let relative_pos = pointer_pos - rect.center() - self.pan_offset;
                self.pan_offset += relative_pos * (1.0 - scale_ratio);
            }
        } else if (new_zoom_factor - 1.0).abs() <= 0.01 {
            self.reset_pan();
        }

        self.zoom_factor = new_zoom_factor;
        self.schedule_zoom_texture_reload(ctx);
        ctx.request_repaint();
    }

    fn zoom_in(&mut self, ctx: &egui::Context) {
        self.apply_zoom_factor(self.zoom_factor * ZOOM_STEP, ctx, true);
    }

    fn zoom_out(&mut self, ctx: &egui::Context) {
        self.apply_zoom_factor(self.zoom_factor / ZOOM_STEP, ctx, true);
    }

    fn effective_image_container_size(&self, ctx: &egui::Context) -> egui::Vec2 {
        let content_rect = ctx.content_rect();
        if self.is_single_page() || (self.is_shifted && self.current_index == 0) {
            content_rect.size()
        } else {
            egui::vec2(
                ((content_rect.width() * 0.5) - self.config.spread_center_offset.abs()).max(1.0),
                content_rect.height(),
            )
        }
    }

    fn image_draw_size(
        &self,
        tex: &egui::TextureHandle,
        container_size: egui::Vec2,
        zoom_factor: f32,
    ) -> egui::Vec2 {
        let tex_size = tex.size_vec2();
        let aspect_ratio = tex_size.x / tex_size.y;

        match self.config.image_sizing_mode {
            ImageSizingMode::FitHeight => {
                let height = container_size.y * zoom_factor;
                egui::vec2(height * aspect_ratio, height)
            }
            ImageSizingMode::FitWidth => {
                let width = container_size.x * zoom_factor;
                egui::vec2(width, width / aspect_ratio)
            }
            ImageSizingMode::FitBoth => {
                let scale = (container_size.x / tex_size.x).min(container_size.y / tex_size.y);
                tex_size * scale * zoom_factor
            }
            ImageSizingMode::OriginalSize => tex_size * zoom_factor,
        }
    }

    fn top_down_image_draw_size(
        &self,
        tex: &egui::TextureHandle,
        container_size: egui::Vec2,
    ) -> egui::Vec2 {
        let tex_size = tex.size_vec2();
        let aspect_ratio = tex_size.x / tex_size.y;
        match self.config.image_sizing_mode {
            ImageSizingMode::FitHeight => {
                let height = container_size.y.max(1.0) * self.zoom_factor;
                egui::vec2(height * aspect_ratio, height)
            }
            ImageSizingMode::FitWidth => {
                let width = container_size.x.max(1.0) * self.zoom_factor;
                egui::vec2(width, width / aspect_ratio)
            }
            ImageSizingMode::FitBoth => {
                let scale = (container_size.x.max(1.0) / tex_size.x)
                    .min(container_size.y.max(1.0) / tex_size.y);
                tex_size * scale * self.zoom_factor
            }
            ImageSizingMode::OriginalSize => tex_size * self.zoom_factor,
        }
    }

    fn reset_top_down_buffer(&mut self) {
        self.top_down_textures = vec![None; self.image_files.len()];
        self.top_down_scroll_offset = 0.0;
    }

    fn ensure_top_down_texture_loaded(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.image_files.len() {
            return;
        }
        if self.top_down_textures.len() != self.image_files.len() {
            self.reset_top_down_buffer();
        }
        if self.top_down_textures[index].is_some() {
            return;
        }

        let pair = self.load_pair(index, ctx);
        self.top_down_textures[index] = pair[0].clone();
        if index + 1 < self.top_down_textures.len() {
            self.top_down_textures[index + 1] = pair[1].clone();
        }
    }

    pub(super) fn ensure_top_down_loaded_around(&mut self, index: usize, ctx: &egui::Context) {
        if self.config.page_view_options != PageViewOptions::TopDown {
            return;
        }
        if self.image_files.is_empty() {
            return;
        }
        let first = index.saturating_sub(2);
        let last = index
            .saturating_add(2)
            .min(self.image_files.len().saturating_sub(1));
        for page_index in first..=last {
            self.ensure_top_down_texture_loaded(page_index, ctx);
        }
    }

    pub(super) fn top_down_texture(&self, index: usize) -> Option<&egui::TextureHandle> {
        self.top_down_textures.get(index)?.as_ref()
    }

    pub(super) fn clamp_top_down_scroll_offset(&mut self) {
        if self.current_index == 0 && self.top_down_scroll_offset > 0.0 {
            self.top_down_scroll_offset = 0.0;
        }
        if self.current_index + 1 >= self.image_files.len() && self.top_down_scroll_offset < 0.0 {
            self.top_down_scroll_offset = 0.0;
        }
    }

    fn double_page_needs_drag(&self, rect: Rect) -> bool {
        let page_size = egui::vec2(rect.width() * 0.5, rect.height());
        self.textures[1]
            .as_ref()
            .map(|tex| self.image_draw_size(tex, page_size, self.zoom_factor))
            .is_some_and(|size| size.x > page_size.x || size.y > page_size.y)
            || self.textures[0]
                .as_ref()
                .map(|tex| self.image_draw_size(tex, page_size, self.zoom_factor))
                .is_some_and(|size| size.x > page_size.x || size.y > page_size.y)
    }

    fn update_buffers(&mut self, ctx: &egui::Context) {
        let idx = self.current_index;

        if self.is_scrubbing {
            return;
        }

        if self.zip_path.is_none() {
            return;
        }

        if self.zoom_factor != 1.0 {
            return;
        }

        // Only update if we moved OR if the buffers were recently consumed
        if self.last_buffered_index == Some(idx) {
            return;
        }

        let step = if self.is_single_page() { 1 } else { 2 };

        // Preload Next (2 pages ahead)
        if self.buffer_next[0].is_none() {
            let mut next_index_to_load = idx + step;
            if idx == 0 && self.is_shifted {
                next_index_to_load = 1;
            }
            self.buffer_next = self.load_pair(next_index_to_load, ctx);
        }

        // Preload Prev (2 pages behind)
        if idx >= step && self.buffer_prev[0].is_none() {
            self.buffer_prev = self.load_pair(idx - step, ctx);
        }

        // Preload prev buffer case for when using cover mode
        if self.is_shifted && idx >= 1 && self.buffer_prev[0].is_none() {
            self.buffer_prev = self.load_pair(0, ctx);
        }

        self.last_buffered_index = Some(idx);
    }

    fn animated_image_format(&self, filename: &str) -> Option<AnimatedImageFormat> {
        if !self.config.enable_animated_images {
            return None;
        }
        Self::animated_image_format_unchecked(filename)
    }

    fn animated_image_format_unchecked(filename: &str) -> Option<AnimatedImageFormat> {
        let extension = Path::new(filename)
            .extension()?
            .to_string_lossy()
            .to_ascii_lowercase();
        match extension.as_str() {
            "webp" => Some(AnimatedImageFormat::WebP),
            "gif" => Some(AnimatedImageFormat::Gif),
            "png" => Some(AnimatedImageFormat::Png),
            _ => None,
        }
    }

    fn read_source_bytes_from(
        source_mode: SourceMode,
        source_path: &Path,
        filename: &str,
    ) -> Option<Vec<u8>> {
        if source_mode == SourceMode::Folder {
            return fs::read(filename).ok();
        }
        if source_mode == SourceMode::Zip {
            let file = File::open(source_path).ok()?;
            let mut archive = zip::ZipArchive::new(file).ok()?;
            let mut entry = archive.by_name(filename).ok()?;
            let mut buffer = Vec::new();
            entry.read_to_end(&mut buffer).ok()?;
            return Some(buffer);
        }
        if source_mode == SourceMode::Rar {
            return unrar::Archive::new(source_path)
                .open_for_processing()
                .ok()
                .and_then(|rar_archive| {
                    let mut cursor = rar_archive.read_header().ok().flatten();
                    loop {
                        match cursor {
                            Some(e) => {
                                let current_name = e.entry().filename.to_str();
                                if let Some(name_str) = current_name {
                                    if name_str == filename {
                                        break e.read().ok().map(|(bytes, _arc)| bytes);
                                    }
                                    cursor = e
                                        .skip()
                                        .ok()
                                        .and_then(|arc| arc.read_header().ok().flatten());
                                } else {
                                    cursor = e
                                        .skip()
                                        .ok()
                                        .and_then(|arc| arc.read_header().ok().flatten());
                                }
                            }
                            None => break None,
                        }
                    }
                });
        }
        None
    }

    fn frame_delay(frame: &image::Frame) -> Duration {
        let delay = Duration::from(frame.delay());
        if delay.is_zero() {
            Duration::from_millis(100)
        } else {
            delay
        }
    }

    fn current_image_processing_spec(&self, ctx: &egui::Context) -> ImageProcessingSpec {
        let container_size = self.effective_image_container_size(ctx);
        ImageProcessingSpec {
            resize_method: self.config.resize_method,
            image_sizing_mode: self.config.image_sizing_mode,
            transparency_support: self.config.transparency_support,
            zoom_factor: self.zoom_factor,
            container_size: [container_size.x, container_size.y],
        }
    }

    fn color_image_from_dynamic_with_spec(
        img: DynamicImage,
        spec: ImageProcessingSpec,
    ) -> egui::ColorImage {
        let resize_start = Instant::now();
        let filter = spec.resize_method.to_filter();
        let processed_img = if let Some(filter_type) = filter {
            if spec.image_sizing_mode == ImageSizingMode::OriginalSize {
                img
            } else {
                let container_size = egui::vec2(spec.container_size[0], spec.container_size[1]);
                let aspect_ratio = img.width() as f32 / img.height() as f32;
                let factor = if (spec.zoom_factor - 1.0).abs() > 0.01 {
                    3
                } else {
                    1
                };
                let (target_w, target_h) = match spec.image_sizing_mode {
                    ImageSizingMode::FitHeight => {
                        let target_h = container_size.y.max(1.0) as u32;
                        let target_w = (target_h as f32 * aspect_ratio) as u32;
                        (target_w, target_h)
                    }
                    ImageSizingMode::FitWidth => {
                        let target_w = container_size.x.max(1.0) as u32;
                        let target_h = (target_w as f32 / aspect_ratio) as u32;
                        (target_w, target_h)
                    }
                    ImageSizingMode::FitBoth => {
                        let width_scale = container_size.x.max(1.0) / img.width() as f32;
                        let height_scale = container_size.y.max(1.0) / img.height() as f32;
                        let scale = width_scale.min(height_scale);
                        let target_w = (img.width() as f32 * scale) as u32;
                        let target_h = (img.height() as f32 * scale) as u32;
                        (target_w, target_h)
                    }
                    ImageSizingMode::OriginalSize => unreachable!(),
                };
                img.resize(
                    target_w.max(1) * factor,
                    target_h.max(1) * factor,
                    filter_type,
                )
            }
        } else {
            img
        };

        let _resize_time = resize_start.elapsed();
        let process_start = Instant::now();
        let size = [processed_img.width() as _, processed_img.height() as _];
        let color_img = if spec.transparency_support {
            egui::ColorImage::from_rgba_unmultiplied(
                size,
                processed_img.to_rgba8().as_flat_samples().as_slice(),
            )
        } else {
            egui::ColorImage::from_rgb(size, processed_img.to_rgb8().as_raw())
        };
        let _process_time = process_start.elapsed();

        #[cfg(debug_assertions)]
        {
            println!("----------------------------------");
            println!("resize_time: {:?}", _resize_time);
            println!("process_time: {:?}", _process_time);
            println!("total: {:?}", _process_time + _resize_time);
            println!("filter: {:?}", filter);
            println!("----------------------------------");
        }

        color_img
    }

    fn color_image_from_dynamic(&self, img: DynamicImage, ctx: &egui::Context) -> egui::ColorImage {
        Self::color_image_from_dynamic_with_spec(img, self.current_image_processing_spec(ctx))
    }

    fn read_source_bytes(&self, filename: &str) -> Option<Vec<u8>> {
        let source_path = self.zip_path.as_ref()?;
        Self::read_source_bytes_from(self.source_mode, source_path, filename)
    }

    fn detect_animation_scan_status(
        format: AnimatedImageFormat,
        bytes: &[u8],
    ) -> Option<AnimationScanStatus> {
        match format {
            AnimatedImageFormat::WebP => {
                let decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
                if decoder.has_animation() {
                    Some(AnimationScanStatus::Animated(AnimatedImageFormat::WebP))
                } else {
                    Some(AnimationScanStatus::NotAnimated)
                }
            }
            AnimatedImageFormat::Gif => {
                let mut frames = GifDecoder::new(Cursor::new(bytes)).ok()?.into_frames();
                let _ = frames.next()?.ok()?;
                match frames.next() {
                    Some(Ok(_)) => Some(AnimationScanStatus::Animated(AnimatedImageFormat::Gif)),
                    Some(Err(_)) => None,
                    None => Some(AnimationScanStatus::NotAnimated),
                }
            }
            AnimatedImageFormat::Png => {
                let decoder = PngDecoder::new(Cursor::new(bytes)).ok()?;
                if decoder.is_apng().ok()? {
                    Some(AnimationScanStatus::Animated(AnimatedImageFormat::Png))
                } else {
                    Some(AnimationScanStatus::NotAnimated)
                }
            }
        }
    }

    fn spawn_animation_scan_worker(
        source_mode: SourceMode,
        source_path: PathBuf,
        files: Vec<(String, AnimatedImageFormat)>,
        generation: u64,
        epoch: Arc<AtomicU64>,
    ) -> Receiver<AnimationScanEvent> {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for (filename, format) in files {
                if epoch.load(Ordering::Relaxed) != generation {
                    return;
                }
                let Some(bytes) =
                    Self::read_source_bytes_from(source_mode, &source_path, &filename)
                else {
                    continue;
                };
                let Some(status) = Self::detect_animation_scan_status(format, &bytes) else {
                    continue;
                };
                if tx
                    .send(AnimationScanEvent::Result {
                        generation,
                        filename,
                        status,
                    })
                    .is_err()
                {
                    return;
                }
            }

            if epoch.load(Ordering::Relaxed) == generation {
                let _ = tx.send(AnimationScanEvent::Finished { generation });
            }
        });
        rx
    }

    fn restart_animation_scan_for_current_window(&mut self) {
        self.cancel_animation_scan();

        if !self.config.enable_animated_images
            || self.image_files.is_empty()
            || self.source_mode == SourceMode::Pdf
        {
            return;
        }

        let Some(source_path) = self.zip_path.clone() else {
            return;
        };

        let files: Vec<(String, AnimatedImageFormat)> = self
            .image_files
            .iter()
            .skip(self.current_index)
            .take(ANIMATION_SCAN_AHEAD_PAGES + 1)
            .filter_map(|filename| {
                if !matches!(
                    self.animation_scan_status.get(filename).copied(),
                    Some(AnimationScanStatus::Animated(_)) | Some(AnimationScanStatus::NotAnimated)
                ) {
                    Self::animated_image_format_unchecked(filename)
                        .map(|format| (filename.clone(), format))
                } else {
                    None
                }
            })
            .collect();

        self.animation_scan_anchor = Some(self.current_index);
        if files.is_empty() {
            return;
        }

        let generation = self.animation_scan_generation;
        self.animation_scan_rx = Some(Self::spawn_animation_scan_worker(
            self.source_mode,
            source_path,
            files,
            generation,
            Arc::clone(&self.animation_scan_epoch),
        ));
    }

    fn poll_animation_scan(&mut self, ctx: &egui::Context) {
        let mut received_any = false;
        let mut finished = false;

        if let Some(rx) = self.animation_scan_rx.as_ref() {
            loop {
                match rx.try_recv() {
                    Ok(AnimationScanEvent::Result {
                        generation,
                        filename,
                        status,
                    }) => {
                        if generation == self.animation_scan_generation {
                            self.animation_scan_status.insert(filename, status);
                            received_any = true;
                        }
                    }
                    Ok(AnimationScanEvent::Finished { generation }) => {
                        if generation == self.animation_scan_generation {
                            finished = true;
                        }
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
        }

        if finished {
            self.animation_scan_rx = None;
        }

        if received_any {
            ctx.request_repaint();
        }
    }

    fn sync_animation_scan(&mut self, ctx: &egui::Context) {
        self.poll_animation_scan(ctx);

        if !self.config.enable_animated_images || self.image_files.is_empty() {
            self.cancel_animation_scan();
            return;
        }

        if self.animation_scan_anchor != Some(self.current_index) {
            self.restart_animation_scan_for_current_window();
        }
    }

    fn decode_animated_image_first_frame(
        &self,
        format: AnimatedImageFormat,
        buffer: Vec<u8>,
        ctx: &egui::Context,
    ) -> Option<(DynamicImage, Option<AnimatedFrame>, Option<Vec<u8>>)> {
        match format {
            AnimatedImageFormat::WebP => {
                let decoder = WebPDecoder::new(Cursor::new(buffer.clone())).ok()?;
                if decoder.has_animation() {
                    let mut frames = decoder.into_frames();
                    let first_frame = frames.next()?.ok()?;
                    let animated_frame = AnimatedFrame {
                        delay: Self::frame_delay(&first_frame),
                        image: self.color_image_from_dynamic(
                            DynamicImage::ImageRgba8(first_frame.clone().into_buffer()),
                            ctx,
                        ),
                    };
                    Some((
                        DynamicImage::ImageRgba8(first_frame.into_buffer()),
                        Some(animated_frame),
                        Some(buffer),
                    ))
                } else {
                    image::load_from_memory_with_format(&buffer, ImageFormat::WebP)
                        .ok()
                        .map(|img| (img, None, None))
                }
            }
            AnimatedImageFormat::Gif => {
                let mut frames = GifDecoder::new(Cursor::new(buffer.clone()))
                    .ok()?
                    .into_frames();
                let first_frame = frames.next()?.ok()?;
                let animated_frame = AnimatedFrame {
                    delay: Self::frame_delay(&first_frame),
                    image: self.color_image_from_dynamic(
                        DynamicImage::ImageRgba8(first_frame.clone().into_buffer()),
                        ctx,
                    ),
                };
                Some((
                    DynamicImage::ImageRgba8(first_frame.into_buffer()),
                    Some(animated_frame),
                    Some(buffer),
                ))
            }
            AnimatedImageFormat::Png => {
                let decoder = PngDecoder::new(Cursor::new(buffer.clone())).ok()?;
                if decoder.is_apng().ok()? {
                    let mut frames = decoder.apng().ok()?.into_frames();
                    let first_frame = frames.next()?.ok()?;
                    let animated_frame = AnimatedFrame {
                        delay: Self::frame_delay(&first_frame),
                        image: self.color_image_from_dynamic(
                            DynamicImage::ImageRgba8(first_frame.clone().into_buffer()),
                            ctx,
                        ),
                    };
                    Some((
                        DynamicImage::ImageRgba8(first_frame.into_buffer()),
                        Some(animated_frame),
                        Some(buffer),
                    ))
                } else {
                    image::load_from_memory_with_format(&buffer, ImageFormat::Png)
                        .ok()
                        .map(|img| (img, None, None))
                }
            }
        }
    }

    fn spawn_animated_image_worker(
        format: AnimatedImageFormat,
        bytes: Vec<u8>,
        spec: ImageProcessingSpec,
    ) -> Receiver<AnimationDecodeEvent> {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let process_frame = |frame: image::Frame| -> bool {
                let event = AnimationDecodeEvent::Frame(AnimatedFrame {
                    delay: Self::frame_delay(&frame),
                    image: Self::color_image_from_dynamic_with_spec(
                        DynamicImage::ImageRgba8(frame.into_buffer()),
                        spec,
                    ),
                });
                tx.send(event).is_ok()
            };

            match format {
                AnimatedImageFormat::WebP => {
                    let Ok(decoder) = WebPDecoder::new(Cursor::new(bytes)) else {
                        let _ = tx.send(AnimationDecodeEvent::Failed);
                        return;
                    };
                    if !decoder.has_animation() {
                        let _ = tx.send(AnimationDecodeEvent::Finished);
                        return;
                    }

                    let mut frames = decoder.into_frames();
                    let _ = frames.next();
                    for frame_result in frames {
                        let Ok(frame) = frame_result else {
                            let _ = tx.send(AnimationDecodeEvent::Failed);
                            return;
                        };
                        if !process_frame(frame) {
                            return;
                        }
                    }
                }
                AnimatedImageFormat::Gif => {
                    let Ok(decoder) = GifDecoder::new(Cursor::new(bytes)) else {
                        let _ = tx.send(AnimationDecodeEvent::Failed);
                        return;
                    };
                    let mut frames = decoder.into_frames();
                    let _ = frames.next();
                    for frame_result in frames {
                        let Ok(frame) = frame_result else {
                            let _ = tx.send(AnimationDecodeEvent::Failed);
                            return;
                        };
                        if !process_frame(frame) {
                            return;
                        }
                    }
                }
                AnimatedImageFormat::Png => {
                    let Ok(decoder) = PngDecoder::new(Cursor::new(bytes)) else {
                        let _ = tx.send(AnimationDecodeEvent::Failed);
                        return;
                    };
                    let Ok(is_apng) = decoder.is_apng() else {
                        let _ = tx.send(AnimationDecodeEvent::Failed);
                        return;
                    };
                    if !is_apng {
                        let _ = tx.send(AnimationDecodeEvent::Finished);
                        return;
                    }
                    let Ok(apng) = decoder.apng() else {
                        let _ = tx.send(AnimationDecodeEvent::Failed);
                        return;
                    };
                    let mut frames = apng.into_frames();
                    let _ = frames.next();
                    for frame_result in frames {
                        let Ok(frame) = frame_result else {
                            let _ = tx.send(AnimationDecodeEvent::Failed);
                            return;
                        };
                        if !process_frame(frame) {
                            return;
                        }
                    }
                }
            }

            let _ = tx.send(AnimationDecodeEvent::Finished);
        });
        rx
    }

    fn cached_or_load_animated_image_bytes(&mut self, filename: &str) -> Option<Vec<u8>> {
        if let Some(bytes) = self.animated_image_byte_cache.get(filename) {
            return Some(bytes.clone());
        }
        let bytes = self.read_source_bytes(filename)?;
        self.animated_image_byte_cache
            .insert(filename.to_owned(), bytes.clone());
        Some(bytes)
    }

    fn poll_animation_decode(animation: &mut AnimatedPage) -> bool {
        let mut received_any = false;
        let Some(rx) = animation.decode_rx.as_ref() else {
            return false;
        };
        loop {
            match rx.try_recv() {
                Ok(AnimationDecodeEvent::Frame(frame)) => {
                    animation.frames.push(frame);
                    received_any = true;
                }
                Ok(AnimationDecodeEvent::Finished) => {
                    animation.decode_complete = true;
                    animation.decode_rx = None;
                    return true;
                }
                Ok(AnimationDecodeEvent::Failed) => {
                    animation.decode_complete = true;
                    animation.decode_rx = None;
                    return received_any;
                }
                Err(TryRecvError::Empty) => return received_any,
                Err(TryRecvError::Disconnected) => {
                    animation.decode_complete = true;
                    animation.decode_rx = None;
                    return received_any;
                }
            }
        }
    }

    fn ensure_active_animations(&mut self, ctx: &egui::Context) {
        if self.config.page_view_options == PageViewOptions::TopDown {
            self.clear_active_animations();
            return;
        }

        if !self.config.enable_animated_images {
            self.clear_active_animations();
            return;
        }

        for slot in 0..2 {
            let desired_key = if self.textures[slot].is_some() {
                self.image_files
                    .get(self.current_index + slot)
                    .filter(|filename| Self::animated_image_format_unchecked(filename).is_some())
                    .cloned()
            } else {
                None
            };

            let desired_status = desired_key
                .as_ref()
                .and_then(|filename| self.animation_scan_status.get(filename).copied())
                .unwrap_or(AnimationScanStatus::Unknown);

            if self.active_animation_keys[slot] == desired_key
                && !(matches!(desired_status, AnimationScanStatus::Animated(_))
                    && self.active_animations[slot].is_none())
            {
                continue;
            }

            self.active_animation_keys[slot] = desired_key.clone();
            self.active_animations[slot] = None;

            let Some(filename) = desired_key else {
                continue;
            };
            let AnimationScanStatus::Animated(format) = desired_status else {
                continue;
            };
            let first_frame =
                if let Some(frame) = self.animated_image_first_frame_cache.get(&filename) {
                    frame.clone()
                } else {
                    let Some(bytes) = self.cached_or_load_animated_image_bytes(&filename) else {
                        continue;
                    };
                    let Some((_, maybe_first_frame, maybe_cached_bytes)) =
                        self.decode_animated_image_first_frame(format, bytes, ctx)
                    else {
                        continue;
                    };
                    if let Some(cached_bytes) = maybe_cached_bytes {
                        self.animated_image_byte_cache
                            .insert(filename.clone(), cached_bytes);
                    }
                    let Some(frame) = maybe_first_frame else {
                        continue;
                    };
                    self.animated_image_first_frame_cache
                        .insert(filename.clone(), frame.clone());
                    frame
                };
            let Some(bytes) = self.cached_or_load_animated_image_bytes(&filename) else {
                continue;
            };
            let animation = AnimatedPage {
                frames: vec![first_frame.clone()],
                current_frame: 0,
                next_frame_at: Instant::now() + first_frame.delay,
                decode_complete: false,
                decode_rx: Some(Self::spawn_animated_image_worker(
                    format,
                    bytes,
                    self.current_image_processing_spec(ctx),
                )),
            };
            if let Some(texture) = self.textures[slot].as_mut() {
                texture.set(first_frame.image.clone(), egui::TextureOptions::LINEAR);
                self.active_animations[slot] = Some(animation);
            }
        }
    }

    fn update_active_animations(&mut self, ctx: &egui::Context) {
        self.ensure_active_animations(ctx);

        let now = Instant::now();
        let mut next_repaint_after: Option<Duration> = None;

        for slot in 0..2 {
            if self.textures[slot].is_none() {
                self.active_animations[slot] = None;
                self.active_animation_keys[slot] = None;
                continue;
            }

            let Some(animation) = self.active_animations[slot].as_mut() else {
                continue;
            };
            let Some(texture) = self.textures[slot].as_mut() else {
                continue;
            };
            let received_new_frames = Self::poll_animation_decode(animation);

            if animation.decode_complete && animation.frames.len() <= 1 {
                continue;
            }

            while now >= animation.next_frame_at {
                let next_index = animation.current_frame + 1;
                if next_index < animation.frames.len() {
                    animation.current_frame = next_index;
                } else if animation.decode_complete && !animation.frames.is_empty() {
                    animation.current_frame = 0;
                } else {
                    break;
                }

                let frame = &animation.frames[animation.current_frame];
                texture.set(frame.image.clone(), egui::TextureOptions::LINEAR);
                animation.next_frame_at = now + frame.delay;
            }

            let repaint_after = if animation.current_frame + 1 < animation.frames.len()
                || animation.decode_complete
            {
                animation.next_frame_at.saturating_duration_since(now)
            } else {
                Duration::from_millis(if received_new_frames { 1 } else { 16 })
            };
            next_repaint_after = Some(
                next_repaint_after
                    .map(|current| current.min(repaint_after))
                    .unwrap_or(repaint_after),
            );
        }

        if let Some(repaint_after) = next_repaint_after {
            ctx.request_repaint_after(repaint_after.max(Duration::from_millis(1)));
        }
    }

    fn load_pair(
        &mut self,
        start_idx: usize,
        ctx: &egui::Context,
    ) -> [Option<egui::TextureHandle>; 2] {
        // no source path set yet
        if self.zip_path == None {
            {
                return [None, None];
            }
        }

        let mut pair: [Option<egui::TextureHandle>; 2] = [None, None];
        let source_path = self.zip_path.clone().unwrap();

        let mut archive = if self.source_mode == SourceMode::Zip {
            let path = source_path.clone();
            File::open(path)
                .ok()
                .and_then(|f| zip::ZipArchive::new(f).ok())
        } else {
            None
        };

        for i in 0..2 {
            let current_target = start_idx + i;
            if let Some(filename) = self.image_files.get(current_target) {
                if let Some(handle) = self.texture_cache.get(filename) {
                    pair[i] = Some(handle.clone());
                    continue;
                }

                if self.source_mode == SourceMode::Pdf {
                    let dynamic_image = self.render_pdf_page(current_target, ctx);
                    if let Some(img) = dynamic_image {
                        pair[i] = self.load_texture(img, filename.clone(), ctx);
                    }
                } else {
                    let bytes = if self.source_mode == SourceMode::Folder {
                        fs::read(filename).ok() // Load directly from path
                    } else if let Some(ref mut arc) = archive {
                        arc.by_name(filename).ok().and_then(|mut f| {
                            let mut b = Vec::new();
                            f.read_to_end(&mut b).ok().map(|_| b)
                        })
                    } else if self.source_mode == SourceMode::Rar {
                        unrar::Archive::new(&source_path)
                            .open_for_processing()
                            .ok()
                            .and_then(|rar_achive| {
                                let mut cursor = rar_achive.read_header().ok().flatten();
                                loop {
                                    match cursor {
                                        Some(e) => {
                                            // Use .entry() before reference filename
                                            let current_name = e.entry().filename.to_str();

                                            if let Some(name_str) = current_name {
                                                if name_str == filename {
                                                    break e.read().ok().map(|(bytes, _arc)| bytes);
                                                } else {
                                                    cursor = e.skip().ok().and_then(|arc| {
                                                        arc.read_header().ok().flatten()
                                                    });
                                                }
                                            } else {
                                                // Filename wasn't valid UTF-8, skip it
                                                cursor = e.skip().ok().and_then(|arc| {
                                                    arc.read_header().ok().flatten()
                                                });
                                            }
                                        }
                                        None => break None,
                                    }
                                }
                            })
                    } else {
                        None
                    };

                    if let Some(mut buffer) = bytes {
                        if self.config.enable_auto_image_byte_fix {
                            buffer = strip_adobe_app14_if_invalid(&buffer);
                        }
                        let scan_status = self
                            .animation_scan_status
                            .get(filename)
                            .copied()
                            .unwrap_or(AnimationScanStatus::Unknown);
                        let animated_format = match scan_status {
                            AnimationScanStatus::Animated(format) => Some(format),
                            AnimationScanStatus::NotAnimated => None,
                            AnimationScanStatus::Unknown => self.animated_image_format(filename),
                        };

                        if let Some(format) = animated_format {
                            if let Some((img, first_frame, animated_bytes)) =
                                self.decode_animated_image_first_frame(format, buffer, ctx)
                            {
                                if let Some(frame) = first_frame {
                                    self.animation_scan_status.insert(
                                        filename.clone(),
                                        AnimationScanStatus::Animated(format),
                                    );
                                    self.animated_image_first_frame_cache
                                        .insert(filename.clone(), frame);
                                } else {
                                    self.animation_scan_status
                                        .insert(filename.clone(), AnimationScanStatus::NotAnimated);
                                }
                                if let Some(animated_bytes) = animated_bytes {
                                    self.animated_image_byte_cache
                                        .insert(filename.clone(), animated_bytes);
                                }
                                pair[i] = self.load_texture(img, filename.clone(), ctx);
                            }
                        } else {
                            match image::guess_format(&buffer) {
                                Ok(format) => {
                                    if let Ok(img) =
                                        image::load_from_memory_with_format(&buffer, format)
                                    {
                                        pair[i] = self.load_texture(img, filename.clone(), ctx);
                                    }
                                }
                                Err(_) => {
                                    // Fallback: If guessing fails, try loading as TGA
                                    // since TGA is often the one that fails detection.
                                    if let Ok(img) = image::load_from_memory_with_format(
                                        &buffer,
                                        ImageFormat::Tga,
                                    ) {
                                        pair[i] = self.load_texture(img, filename.clone(), ctx);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        pair
    }

    /// Helper to render a specific page
    fn render_pdf_page(&self, index: usize, ctx: &egui::Context) -> Option<DynamicImage> {
        let source_path = self.zip_path.as_ref()?;
        let doc = match self.pdf_document.as_ref() {
            Some(doc) if self.pdf_document_path.as_deref() == Some(source_path.as_path()) => doc,
            _ => return None,
        };

        let page = match doc.pages().get(index as u16) {
            Ok(page) => page,
            Err(_) => return None,
        };
        let width_inch = page.width().value;
        let height_inch = page.height().value;

        let screen_size = ctx.content_rect();
        let target_h = screen_size.height();
        let h_ratio = target_h / height_inch;
        let target_w = width_inch * h_ratio;

        // Render at 300 DPI or based on screen height for clarity
        let bitmap = page
            .render(target_w as Pixels, target_h as Pixels, None)
            .ok()?;
        Some(bitmap.as_image()) // pdfium-render integrates with the 'image' crate
    }

    fn load_texture(
        &mut self,
        img: DynamicImage,
        cache_name: String,
        ctx: &egui::Context,
    ) -> Option<egui::TextureHandle> {
        let color_img = self.color_image_from_dynamic(img, ctx);
        let handle = ctx.load_texture(
            &cache_name.clone(),
            color_img,
            egui::TextureOptions::LINEAR, // Smooth scaling
        );
        if self.config.enable_single_file_caching {
            self.texture_cache
                .insert(cache_name.clone(), handle.clone());
        }
        Some(handle)
    }

    fn load_source(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.stop_auto_scroll();
        self.clear_pdf_document_cache();
        let mut target_path = path.clone();
        let mut start_at_filename: Option<String> = None;

        let mut images = Vec::new();
        let exts = [
            "png", "jpg", "jpeg", "bmp", "webp", "gif", "tiff", "tga", "avif",
        ];
        let extension = path
            .extension()
            .map_or("".to_string(), |ext| ext.to_string_lossy().to_lowercase());
        match extension.as_str() {
            "zip" | "cbz" => {
                let file = match File::open(&target_path) {
                    Ok(f) => f,
                    Err(_) => return,
                };
                if let Ok(mut archive) = zip::ZipArchive::new(file) {
                    for i in 0..archive.len() {
                        if let Ok(f) = archive.by_index(i) {
                            let name = f.name().to_lowercase();
                            if exts.iter().any(|&e| name.ends_with(&format!(".{}", e))) {
                                images.push(f.name().to_string());
                            }
                        }
                    }
                    self.source_mode = SourceMode::Zip;
                }
                self.source_mode = SourceMode::Zip;
                // ... existing zip logic
            }
            "cbr" | "rar" => {
                self.source_mode = SourceMode::Rar;
                if let Ok(archive) = unrar::Archive::new(&path).open_for_listing() {
                    for entry in archive {
                        if let Ok(e) = entry {
                            // Convert Option<&str> to String safely
                            if let Some(name_str) = e.filename.to_str() {
                                let name = name_str.to_string();
                                // Check if it's an image extension
                                if exts
                                    .iter()
                                    .any(|&e_ext| name.to_lowercase().ends_with(e_ext))
                                {
                                    images.push(name);
                                }
                            }
                        }
                    }
                }
            }
            "pdf" => {
                // --- PDF MODE ---
                match self.ensure_pdf_document_loaded(&path) {
                    Some(page_count) => {
                        for i in 0..page_count {
                            // We use a virtual naming scheme for PDF pages in our image_files list
                            images.push(format!("pdf_page_{}", i));
                        }
                    }
                    None => {}
                }
                self.source_mode = SourceMode::Pdf;
            }
            _ => {
                if path.is_file() {
                    // Pivot: Use the folder containing this image as the source
                    if let Some(parent) = path.parent() {
                        start_at_filename = Some(path.to_string_lossy().to_string());
                        target_path = parent.to_path_buf();
                    }
                }

                if target_path.is_dir() {
                    // --- FOLDER MODE ---
                    if let Ok(entries) = fs::read_dir(&target_path) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.is_file()
                                && exts.iter().any(|&e| {
                                    p.extension().map_or(false, |ext| {
                                        ext.to_string_lossy().to_lowercase() == e
                                    })
                                })
                            {
                                images.push(p.to_string_lossy().to_string());
                            }
                        }
                    }
                }
                self.source_mode = SourceMode::Folder;
            }
        }

        windows_natural_sort_strings(&mut images);

        if images.is_empty() {
            let msg = tr("error.no_images_found").to_owned();
            self.show_fading_error(&msg);
        } else {
            self.reset_buffer();
            self.texture_cache.clear();
            self.clear_animated_image_cache();
            self.clear_animation_scan_state();
            self.clear_active_animations();
            self.reset_pan();

            // If we opened a specific image, find its index in the sorted list
            self.current_index = if let Some(target_name) = start_at_filename {
                images.iter().position(|r| r == &target_name).unwrap_or(0)
            } else {
                0
            };

            self.zip_path = Some(target_path.clone());
            self.image_files = images;

            // Scan parent for Next/Prev file navigation
            self.all_zips_in_folder = scan_folder(&target_path.parent().unwrap_or(Path::new("")));

            if let Some(file_name) = target_path.file_name() {
                self.zip_name_display =
                    Some((file_name.to_string_lossy().to_string(), Instant::now()));
            }

            self.restart_animation_scan_for_current_window();
            self.textures = self.load_pair(self.current_index, ctx);
            self.reset_top_down_buffer();
            if self.config.page_view_options == PageViewOptions::TopDown {
                self.ensure_top_down_loaded_around(self.current_index, ctx);
                for index in 0..self.image_files.len().min(5) {
                    self.ensure_top_down_texture_loaded(index, ctx);
                }
            }
        }

        self.page_indicator_time = Some(Instant::now());
    }

    fn show_fading_error(&mut self, msg: &str) {
        self.error_msg = Some((msg.to_string(), Instant::now()));
    }

    fn next_page(&mut self, ctx: &egui::Context, force_step_one: bool) {
        if self.last_image_switch_time + Duration::from_millis(self.config.image_delay)
            > Instant::now()
        {
            return;
        } else {
            self.last_image_switch_time = Instant::now();
        }
        let step = if force_step_one {
            // reset the buffer when force step 1 to prevent 2 images copy from buffer to active
            self.buffer_next = [None, None];
            self.buffer_prev = [None, None];
            1
        } else if self.is_single_page() || (self.is_shifted && self.current_index == 0) {
            1
        } else {
            2
        };

        if self.current_index + step < self.image_files.len() {
            self.current_index += step;
            self.reset_pan();
            self.top_down_scroll_offset = 0.0;
            if self.config.page_view_options == PageViewOptions::TopDown {
                self.ensure_top_down_loaded_around(self.current_index, ctx);
            } else if self.buffer_next[0].is_some() {
                // If the next pages are already in the buffer, swap them in
                // Take the textures from the buffer and put them in the active slot
                // also slide the current texture to prev buffer
                self.buffer_prev = std::mem::take(&mut self.textures);
                self.textures = std::mem::take(&mut self.buffer_next);
            } else {
                // Fallback if buffer wasn't ready (e.g., very fast clicking)
                self.textures = self.load_pair(self.current_index, ctx);
            }
            self.buffer_next = [None, None];
        } else {
            // End of Zip list reached, do the last page action
            match self.config.last_page_action {
                LastPageAction::GotoNextFile => self.next_zip(ctx),
                LastPageAction::ToFirstPage => self.go_to_first_page(ctx),
                LastPageAction::Nothing => {
                    let msg = tr("error.no_more_pages").to_owned();
                    self.show_fading_error(&msg);
                }
            }
        }
        self.page_indicator_time = Some(Instant::now());
    }

    fn prev_page(&mut self, ctx: &egui::Context, force_step_one: bool) {
        if self.last_image_switch_time + Duration::from_millis(self.config.image_delay)
            > Instant::now()
        {
            return;
        } else {
            self.last_image_switch_time = Instant::now();
        }
        let step = if force_step_one {
            // reset the buffer when force step 1 to prevent 2 images copy from buffer to active
            self.buffer_next = [None, None];
            self.buffer_prev = [None, None];
            1
        } else if self.is_single_page() || (self.is_shifted && self.current_index == 1) {
            1
        } else {
            2
        };

        if self.current_index >= step {
            self.current_index -= step;
            self.reset_pan();
            self.top_down_scroll_offset = 0.0;
            if self.config.page_view_options == PageViewOptions::TopDown {
                self.ensure_top_down_loaded_around(self.current_index, ctx);
            } else if self.buffer_prev[0].is_some() {
                // Use the previous buffer textures
                self.buffer_next = std::mem::take(&mut self.textures);
                self.textures = std::mem::take(&mut self.buffer_prev);
            } else {
                self.textures = self.load_pair(self.current_index, ctx);
            }
            self.buffer_prev = [None, None];
        } else {
            // we are at the start of the Zip, do the last page action
            match self.config.last_page_action {
                LastPageAction::GotoNextFile => self.prev_zip(ctx),
                LastPageAction::ToFirstPage => self.go_to_last_page(ctx),
                LastPageAction::Nothing => {
                    let msg = tr("error.first_page").to_owned();
                    self.show_fading_error(&msg)
                }
            }
        }
        self.page_indicator_time = Some(Instant::now());
    }

    fn next_zip(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self
            .all_zips_in_folder
            .iter()
            .position(|p| Some(p) == self.zip_path.as_ref())
        {
            if pos + 1 < self.all_zips_in_folder.len() {
                // There is a next file
                let next_path = self.all_zips_in_folder[pos + 1].clone();
                self.load_source(next_path, ctx);
            } else {
                // NO MORE FILES - This is the fix
                let msg = tr("error.no_more_zip_files").to_owned();
                self.show_fading_error(&msg);
            }
        }
    }

    fn prev_zip(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self
            .all_zips_in_folder
            .iter()
            .position(|p| Some(p) == self.zip_path.as_ref())
        {
            if pos > 0 {
                let prev_path = self.all_zips_in_folder[pos - 1].clone();
                // We pass 'true' to load_zip so it knows to start at the end of the new file
                self.load_source(prev_path, ctx);
            } else {
                let msg = tr("error.no_previous_zip_files").to_owned();
                self.show_fading_error(&msg);
            }
        }
    }

    fn next_folder(&mut self, ctx: &egui::Context) {
        let (_, next_dir) = get_adjacent_directories(self.zip_path.clone());

        // Check if next_dir actually exists
        if let Some(dir) = next_dir {
            let zips = scan_folder(&*dir);
            if zips.is_empty() {
                let msg = tr("error.no_archive_next_folder").to_owned();
                self.show_fading_error(&msg);
            } else {
                self.load_source(zips[0].clone(), ctx);
            }
        } else {
            let msg = tr("error.no_next_directory").to_owned();
            self.show_fading_error(&msg);
        }
    }

    fn prev_folder(&mut self, ctx: &egui::Context) {
        let (prev_dir, _) = get_adjacent_directories(self.zip_path.clone());

        // Check if next_dir actually exists
        if let Some(dir) = prev_dir {
            let zips = scan_folder(&*dir);
            if zips.is_empty() {
                let msg = tr("error.no_archive_prev_folder").to_owned();
                self.show_fading_error(&msg);
            } else {
                self.load_source(zips[0].clone(), ctx);
            }
        } else {
            let msg = tr("error.no_previous_directory").to_owned();
            self.show_fading_error(&msg);
        }
    }

    fn go_to_first_page(&mut self, ctx: &egui::Context) {
        if !self.image_files.is_empty() && self.current_index != 0 {
            self.reset_buffer();
            self.current_index = 0;
            self.textures = self.load_pair(self.current_index, ctx);
            self.page_indicator_time = Some(Instant::now());
        }
    }

    fn go_to_last_page(&mut self, ctx: &egui::Context) {
        if !self.image_files.is_empty() {
            // Find the last possible pair start (must be an even index)
            let last_idx = (self.image_files.len().saturating_sub(1) / 2) * 2;
            if self.current_index != last_idx {
                self.reset_buffer();
                self.current_index = last_idx;
                self.textures = self.load_pair(self.current_index, ctx);
                self.page_indicator_time = Some(Instant::now());
            }
        }
    }

    fn reset_buffer(&mut self) {
        self.buffer_prev = [None, None];
        self.buffer_next = [None, None];
        self.top_down_textures.clear();
        self.top_down_scroll_offset = 0.0;
        self.clear_active_animations();
    }

    fn get_mouse_action(&self, gesture: MouseGesture) -> MangaAction {
        let mouse = if self.config.page_view_options == PageViewOptions::TopDown {
            self.config.top_down_mouse
        } else {
            self.config.mouse
        };
        match gesture {
            MouseGesture::Unassigned => MangaAction::None,
            MouseGesture::ScrollUp => mouse.scroll_up,
            MouseGesture::ScrollDown => mouse.scroll_down,
            MouseGesture::Click(MouseButton::Button1) => mouse.button1_click,
            MouseGesture::Click(MouseButton::Button2) => mouse.button2_click,
            MouseGesture::Click(MouseButton::Button3) => mouse.button3_click,
            MouseGesture::Click(MouseButton::Button4) => mouse.button4_click,
            MouseGesture::Click(MouseButton::Button5) => mouse.button5_click,
            MouseGesture::DoubleClick(MouseButton::Button1) => mouse.button1_double_click,
            MouseGesture::DoubleClick(MouseButton::Button2) => mouse.button2_double_click,
            MouseGesture::DoubleClick(MouseButton::Button3) => mouse.button3_double_click,
            MouseGesture::DoubleClick(MouseButton::Button4) => mouse.button4_double_click,
            MouseGesture::DoubleClick(MouseButton::Button5) => mouse.button5_double_click,
            MouseGesture::LongClick(MouseButton::Button1) => mouse.button1_long_click,
            MouseGesture::LongClick(MouseButton::Button2) => mouse.button2_long_click,
            MouseGesture::LongClick(MouseButton::Button3) => mouse.button3_long_click,
            MouseGesture::LongClick(MouseButton::Button4) => mouse.button4_long_click,
            MouseGesture::LongClick(MouseButton::Button5) => mouse.button5_long_click,
        }
    }

    fn gamepad_repeat_interval(&self) -> Duration {
        Duration::from_millis(self.config.image_delay.max(80))
    }

    fn collect_gamepad_action(&mut self, ctx: &egui::Context) -> Option<MangaAction> {
        let gamepad_config = if self.config.page_view_options == PageViewOptions::TopDown {
            self.config.top_down_gamepad
        } else {
            self.config.gamepad
        };
        let repeat_interval = self.gamepad_repeat_interval();
        let gilrs = self.gamepad.as_mut()?;
        let mut action_to_run = None;
        let now = Instant::now();

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(mapped_button) = map_gamepad_button(button) {
                        let action = gamepad_action_for_button(gamepad_config, mapped_button);
                        let index = gamepad_button_index(mapped_button);
                        if is_repeatable_gamepad_action(action) {
                            self.gamepad_repeat_deadlines[index] =
                                Some(now + GAMEPAD_INITIAL_REPEAT_DELAY);
                            ctx.request_repaint_after(GAMEPAD_INITIAL_REPEAT_DELAY);
                        } else {
                            self.gamepad_repeat_deadlines[index] = None;
                        }
                        if action != MangaAction::None {
                            action_to_run = Some(action);
                        }
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(mapped_button) = map_gamepad_button(button) {
                        let index = gamepad_button_index(mapped_button);
                        self.gamepad_repeat_deadlines[index] = None;
                    }
                }
                _ => {}
            }
        }

        let now = Instant::now();
        for button in GamepadButton::ALL {
            let index = gamepad_button_index(button);
            if let Some(deadline) = self.gamepad_repeat_deadlines[index] {
                if now >= deadline {
                    let action = gamepad_action_for_button(gamepad_config, button);
                    if is_repeatable_gamepad_action(action) {
                        self.gamepad_repeat_deadlines[index] = Some(now + repeat_interval);
                        ctx.request_repaint_after(repeat_interval);
                        if action != MangaAction::None {
                            action_to_run = Some(action);
                        }
                    } else {
                        self.gamepad_repeat_deadlines[index] = None;
                    }
                } else {
                    ctx.request_repaint_after(deadline - now);
                }
            }
        }

        action_to_run
    }

    fn double_click_threshold(&self) -> Duration {
        let threshold_ms = if self.config.page_view_options == PageViewOptions::TopDown {
            self.config.top_down_double_click_threshold_ms
        } else {
            self.config.double_click_threshold_ms
        };
        Duration::from_millis(threshold_ms)
    }

    fn collect_pending_mouse_click_action(&mut self, ctx: &egui::Context) -> Option<MangaAction> {
        let now = Instant::now();
        let threshold = self.double_click_threshold();

        for pending in &self.pending_mouse_click {
            if let Some((deadline, _)) = pending {
                if *deadline > now {
                    ctx.request_repaint_after(*deadline - now);
                }
            }
        }

        for pending in &mut self.pending_mouse_click {
            if let Some((deadline, action)) = *pending {
                if now >= deadline {
                    *pending = None;
                    if action != MangaAction::None {
                        return Some(action);
                    }
                } else if deadline.duration_since(now) <= threshold {
                    ctx.request_repaint();
                }
            }
        }

        None
    }

    fn collect_mouse_action(
        &mut self,
        response: &egui::Response,
        ctx: &egui::Context,
    ) -> Option<MangaAction> {
        if let Some(action) = self.collect_pending_mouse_click_action(ctx) {
            return Some(action);
        }

        if !response.hovered() {
            for button in MouseButton::ALL {
                let index = mouse_button_index(button);
                if !ctx.input(|i| i.pointer.button_down(mouse_button_to_pointer(button))) {
                    self.mouse_press_started[index] = None;
                    self.mouse_press_origin[index] = None;
                    self.mouse_long_triggered[index] = false;
                    self.mouse_drag_suppressed[index] = false;
                }
            }
            return None;
        }

        for button in MouseButton::ALL {
            let pointer_button = mouse_button_to_pointer(button);
            let button_index = mouse_button_index(button);
            let now = Instant::now();
            let is_down = ctx.input(|i| i.pointer.button_down(pointer_button));

            if is_down {
                if self.mouse_press_started[button_index].is_none() {
                    self.mouse_press_started[button_index] = Some(now);
                    self.mouse_press_origin[button_index] = ctx.input(|i| i.pointer.interact_pos());
                    self.mouse_long_triggered[button_index] = false;
                    self.mouse_drag_suppressed[button_index] = false;
                    ctx.request_repaint();
                } else if !self.mouse_drag_suppressed[button_index] {
                    if let (Some(origin), Some(current)) = (
                        self.mouse_press_origin[button_index],
                        ctx.input(|i| i.pointer.interact_pos()),
                    ) {
                        if origin.distance(current) > MOUSE_DRAG_THRESHOLD {
                            self.mouse_drag_suppressed[button_index] = true;
                            self.pending_mouse_click[button_index] = None;
                        }
                    }
                }

                if !self.mouse_drag_suppressed[button_index]
                    && !self.mouse_long_triggered[button_index]
                    && self.mouse_press_started[button_index]
                        .map(|started| now.duration_since(started) >= LONG_CLICK_DURATION)
                        .unwrap_or(false)
                {
                    self.mouse_long_triggered[button_index] = true;
                    self.pending_mouse_click[button_index] = None;
                    let action = self.get_mouse_action(MouseGesture::LongClick(button));
                    if action != MangaAction::None {
                        return Some(action);
                    }
                } else if !self.mouse_long_triggered[button_index]
                    && !self.mouse_drag_suppressed[button_index]
                {
                    ctx.request_repaint();
                }
            }

            let was_long_click = self.mouse_long_triggered[button_index];
            let was_dragging = self.mouse_drag_suppressed[button_index];
            if response.clicked_by(pointer_button) && !was_long_click && !was_dragging {
                self.mouse_press_started[button_index] = None;
                self.mouse_press_origin[button_index] = None;
                self.mouse_long_triggered[button_index] = false;
                self.mouse_drag_suppressed[button_index] = false;
                let now = Instant::now();
                let double_action = self.get_mouse_action(MouseGesture::DoubleClick(button));
                let click_action = self.get_mouse_action(MouseGesture::Click(button));

                if let Some((deadline, _)) = self.pending_mouse_click[button_index] {
                    if now <= deadline {
                        self.pending_mouse_click[button_index] = None;
                        if double_action != MangaAction::None {
                            return Some(double_action);
                        }
                        if click_action != MangaAction::None {
                            return Some(click_action);
                        }
                    } else {
                        self.pending_mouse_click[button_index] = None;
                    }
                } else if double_action != MangaAction::None {
                    self.pending_mouse_click[button_index] =
                        Some((now + self.double_click_threshold(), click_action));
                    ctx.request_repaint_after(self.double_click_threshold());
                } else if click_action != MangaAction::None {
                    return Some(click_action);
                }
            }

            if !is_down {
                self.mouse_press_started[button_index] = None;
                self.mouse_press_origin[button_index] = None;
                self.mouse_long_triggered[button_index] = false;
                self.mouse_drag_suppressed[button_index] = false;
            }
        }

        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta);
        let scroll_threshold = 2.0;
        if self.config.page_view_options == PageViewOptions::TopDown {
            if scroll_delta.y > scroll_threshold || scroll_delta.x > scroll_threshold {
                let action = self.get_mouse_action(MouseGesture::ScrollUp);
                if action != MangaAction::None {
                    return Some(action);
                }
            } else if scroll_delta.y < -scroll_threshold || scroll_delta.x < -scroll_threshold {
                let action = self.get_mouse_action(MouseGesture::ScrollDown);
                if action != MangaAction::None {
                    return Some(action);
                }
            }
            return None;
        }

        if self.can_scroll {
            if scroll_delta.y > scroll_threshold || scroll_delta.x > scroll_threshold {
                self.can_scroll = false;
                let action = self.get_mouse_action(MouseGesture::ScrollUp);
                if action != MangaAction::None {
                    return Some(action);
                }
            } else if scroll_delta.y < -scroll_threshold || scroll_delta.x < -scroll_threshold {
                self.can_scroll = false;
                let action = self.get_mouse_action(MouseGesture::ScrollDown);
                if action != MangaAction::None {
                    return Some(action);
                }
            }
        } else if scroll_delta.y.abs() <= scroll_threshold
            && scroll_delta.x.abs() <= scroll_threshold
        {
            self.can_scroll = true;
        }

        None
    }

    fn execute_action(&mut self, action: MangaAction, ctx: &egui::Context) {
        match action {
            MangaAction::SlideImageDown => {
                if self.config.page_view_options == PageViewOptions::TopDown {
                    self.top_down_scroll_offset -=
                        ctx.content_rect().height() * self.config.top_down_image_slide_speed;
                    self.clamp_top_down_scroll_offset();
                }
            }
            MangaAction::SlideImageUp => {
                if self.config.page_view_options == PageViewOptions::TopDown {
                    self.top_down_scroll_offset +=
                        ctx.content_rect().height() * self.config.top_down_image_slide_speed;
                    self.clamp_top_down_scroll_offset();
                }
            }
            MangaAction::ToggleAutoScroll => self.toggle_auto_scroll(ctx),
            MangaAction::ReloadCurrentImage => self.reload_current_image(ctx),
            MangaAction::ZoomIn => self.zoom_in(ctx),
            MangaAction::ZoomOut => self.zoom_out(ctx),
            MangaAction::NextPage => self.next_page(ctx, false),
            MangaAction::PrevPage => self.prev_page(ctx, false),
            MangaAction::OneNextPage => self.next_page(ctx, true),
            MangaAction::OnePrevPage => self.prev_page(ctx, true),
            MangaAction::FirstPage => self.go_to_first_page(ctx),
            MangaAction::LastPage => self.go_to_last_page(ctx),
            MangaAction::NextFile => self.next_zip(ctx),
            MangaAction::PrevFile => self.prev_zip(ctx),
            MangaAction::NextFolder => self.next_folder(ctx),
            MangaAction::PrevFolder => self.prev_folder(ctx),
            MangaAction::FullScreen => {
                self.is_fullscreen = !self.is_fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.is_fullscreen));
            }
            MangaAction::ViewMode => self.change_shifted_mode(ctx),
            MangaAction::OpenFile => self.open_file_dialog(),
            MangaAction::QuitApp => {
                self.save_settings();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            MangaAction::None => {}
        }
    }

    fn create_image_rect(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        visible_rect: Rect,
        hit_id: &str,
        tex_index: usize,
        align: egui::Align,
        image_size: egui::Vec2,
    ) -> egui::Response {
        ui.allocate_ui_at_rect(rect, |ui| {
            let resp = ui.interact(visible_rect, ui.id().with(hit_id), egui::Sense::click());

            if let Some(tex) = &self.textures[tex_index] {
                let previous_clip_rect = ui.clip_rect();
                ui.set_clip_rect(previous_clip_rect.intersect(visible_rect));
                if self.config.image_sizing_mode == ImageSizingMode::FitBoth {
                    let image_x = match align {
                        egui::Align::RIGHT => rect.right() - image_size.x,
                        egui::Align::Center => rect.center().x - image_size.x * 0.5,
                        egui::Align::LEFT => rect.left(),
                    };
                    let image_y = rect.center().y - image_size.y * 0.5;
                    ui.painter().image(
                        tex.id(),
                        egui::Rect::from_min_size(egui::pos2(image_x, image_y), image_size),
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    let layout = egui::Layout::top_down(align);
                    ui.with_layout(layout, |ui| {
                        ui.add(
                            egui::Image::new(tex)
                                .fit_to_exact_size(image_size)
                                .maintain_aspect_ratio(true),
                        );
                    });
                }
                ui.set_clip_rect(previous_clip_rect);
            }
            resp
        })
        .inner
    }

    fn paint_image_clipped(
        &self,
        ui: &egui::Ui,
        tex: &egui::TextureHandle,
        image_rect: Rect,
        visible_rect: Rect,
    ) {
        ui.painter()
            .with_clip_rect(ui.clip_rect().intersect(visible_rect))
            .image(
                tex.id(),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
    }

    fn is_single_page(&self) -> bool {
        matches!(
            self.config.page_view_options,
            PageViewOptions::Single | PageViewOptions::TopDown
        )
    }

    fn change_shifted_mode(&mut self, ctx: &egui::Context) {
        self.is_shifted = !self.is_shifted;

        // Adjust current_index to keep the view consistent
        if self.is_shifted {
            if self.current_index == 0 {
                self.current_index = 0;
            } else if self.current_index % 2 == 0 {
                self.current_index += 1;
            }
        } else {
            // Return to even index alignment
            self.current_index = self.current_index.saturating_sub(1);
            if self.current_index % 2 != 0 {
                self.current_index = self.current_index.saturating_sub(1);
            }
        }

        self.reset_buffer();
        self.texture_cache.clear();
        self.reset_pan();
        self.textures = self.load_pair(self.current_index, ctx);
        let msg = if self.is_shifted {
            tr("mode.odd_page")
        } else {
            tr("mode.even_page")
        };
        self.show_fading_error(msg);
    }
}

impl eframe::App for MangaReader {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        set_language(self.config.language);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(tr("app.title").to_owned()));
        self.sync_animation_scan(ctx);
        self.update_active_animations(ctx);

        // load file if it is dropped on screen
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(df) = dropped_files.first() {
            if let Some(path) = &df.path {
                self.load_source(path.clone(), ctx);
            } else if let Some(_bytes) = &df.bytes {
                self.show_fading_error(tr("error.dropped_bytes_only"));
            }
        }

        let mut action_to_run = MangaAction::None;

        // REBINDING LOGIC
        if let Some(binding_target) = self.binding_action.clone() {
            match binding_target {
                BindingTarget::Keyboard { profile, action } => {
                    ctx.input(|i| {
                        for key in egui::Key::ALL {
                            if i.key_pressed(*key) {
                                let new_shortcut = Shortcut {
                                    key: *key,
                                    ctrl: i.modifiers.ctrl,
                                    alt: i.modifiers.alt,
                                    shift: i.modifiers.shift,
                                };

                                let keys = match profile {
                                    ControlProfile::Default => &mut self.config.keys,
                                    ControlProfile::TopDown => &mut self.config.top_down_keys,
                                };
                                match action.as_str() {
                                    "Slide Image Down" => {
                                        keys.slide_image_down = Some(new_shortcut)
                                    }
                                    "Slide Image Up" => keys.slide_image_up = Some(new_shortcut),
                                    "Toggle Auto Scroll" => {
                                        keys.toggle_auto_scroll = Some(new_shortcut)
                                    }
                                    "Reload Current Image" => {
                                        keys.reload_current_image = Some(new_shortcut)
                                    }
                                    "Next Page" => keys.next_page = Some(new_shortcut),
                                    "Previous Page" => keys.prev_page = Some(new_shortcut),
                                    "1 Next Page" => keys.one_next_page = Some(new_shortcut),
                                    "1 Prev Page" => keys.one_prev_page = Some(new_shortcut),
                                    "First Page" => keys.first_page = Some(new_shortcut),
                                    "Last Page" => keys.last_page = Some(new_shortcut),
                                    "Next File" => keys.next_file = Some(new_shortcut),
                                    "Previous File" => keys.prev_file = Some(new_shortcut),
                                    "Next Folder" => keys.next_folder = Some(new_shortcut),
                                    "Previous Folder" => keys.prev_folder = Some(new_shortcut),
                                    "Toggle Fullscreen" => keys.fullscreen = Some(new_shortcut),
                                    "View Mode" => keys.view_mode = Some(new_shortcut),
                                    "Open File" => keys.open_file = Some(new_shortcut),
                                    "Quit App" => keys.quit_app = Some(new_shortcut),
                                    _ => {}
                                }
                                self.binding_action = None;
                                self.save_settings();
                            }
                        }
                    });
                }
            }
        }
        // PART B: EXECUTION LOGIC
        else {
            ctx.input(|i| {
                let keys = if self.config.page_view_options == PageViewOptions::TopDown {
                    self.config.top_down_keys
                } else {
                    self.config.keys
                };

                // Helper to check if a shortcut is triggered
                let is_triggered = |s: &Option<Shortcut>| {
                    s.is_some_and(|s| {
                        i.key_pressed(s.key)
                            && i.modifiers.ctrl == s.ctrl
                            && i.modifiers.alt == s.alt
                            && i.modifiers.shift == s.shift
                    })
                };

                if is_triggered(&keys.slide_image_down) {
                    action_to_run = MangaAction::SlideImageDown;
                }
                if is_triggered(&keys.slide_image_up) {
                    action_to_run = MangaAction::SlideImageUp;
                }
                if is_triggered(&keys.toggle_auto_scroll) {
                    action_to_run = MangaAction::ToggleAutoScroll;
                }
                if is_triggered(&keys.reload_current_image) {
                    action_to_run = MangaAction::ReloadCurrentImage;
                }
                if is_triggered(&keys.next_page) {
                    action_to_run = MangaAction::NextPage;
                }
                if is_triggered(&keys.prev_page) {
                    action_to_run = MangaAction::PrevPage;
                }
                if is_triggered(&keys.one_next_page) {
                    action_to_run = MangaAction::OneNextPage;
                }
                if is_triggered(&keys.one_prev_page) {
                    action_to_run = MangaAction::OnePrevPage;
                }
                if is_triggered(&keys.first_page) {
                    action_to_run = MangaAction::FirstPage;
                }
                if is_triggered(&keys.last_page) {
                    action_to_run = MangaAction::LastPage;
                }
                if is_triggered(&keys.next_file) {
                    action_to_run = MangaAction::NextFile;
                }
                if is_triggered(&keys.prev_file) {
                    action_to_run = MangaAction::PrevFile;
                }
                if is_triggered(&keys.next_folder) {
                    action_to_run = MangaAction::NextFolder;
                }
                if is_triggered(&keys.prev_folder) {
                    action_to_run = MangaAction::PrevFolder;
                }
                if is_triggered(&keys.fullscreen) {
                    action_to_run = MangaAction::FullScreen;
                }
                if is_triggered(&keys.view_mode) {
                    action_to_run = MangaAction::ViewMode;
                }
                if is_triggered(&keys.open_file) {
                    action_to_run = MangaAction::OpenFile;
                }
                if is_triggered(&keys.quit_app) {
                    action_to_run = MangaAction::QuitApp;
                }
            });
        }

        if self.binding_action.is_none() && action_to_run == MangaAction::None {
            if let Some(gamepad_action) = self.collect_gamepad_action(ctx) {
                action_to_run = gamepad_action;
            }
        }

        // Load file if passed as program parameter
        if let Some(p) = self.initial_path.as_ref() {
            self.load_source(p.clone(), ctx);
            self.initial_path = None;
        }

        // File Dialog Result
        if let Ok(result) = self.dialog_rx.try_recv() {
            self.is_dialog_open = false;
            if let Some(path) = result {
                self.load_source(path, ctx);
            }
        }
        self.reload_zoom_textures_if_settled(ctx);

        if self.config.show_settings {
            egui::SidePanel::right("settings_panel")
                .resizable(true) // Enable mouse dragging
                .default_width(self.config.settings_width)
                .width_range(150.0..=500.0) // Constraints
                .frame({
                    let v = ctx.style().visuals.clone();

                    // Use theme colors instead of fixed gray.
                    // Slight tint so it stands out from the main background in both modes.
                    let fill = if v.dark_mode {
                        v.window_fill.gamma_multiply(1.15)
                    } else {
                        v.window_fill.gamma_multiply(0.97)
                    };

                    egui::Frame::NONE
                        .fill(fill)
                        .stroke(v.window_stroke)
                        .inner_margin(egui::Margin::same(10))
                })
                .show(ctx, |ui| {
                    // Update the stored width based on user dragging
                    self.config.settings_width = ui.available_width();

                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.heading(egui::RichText::new(tr("settings.title")).strong());
                    });
                    ui.add_space(10.0);
                    separator_pct(ui);

                    // The rest becomes scrollable
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2]) // don't shrink content; enable scrolling
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.add_space(10.0);

                                // [Open File] Button
                                if ui
                                    .add_sized(
                                        [ui.available_width() * 0.94, 30.0],
                                        egui::Button::new(tr("settings.open_file")),
                                    )
                                    .clicked()
                                {
                                    self.open_file_dialog();
                                }
                                ui.add_space(20.0);
                                ui.label(
                                    egui::RichText::new(tr("language.label"))
                                        .size(20.0)
                                        .strong(),
                                );
                                separator_pct(ui);
                                let mut language_changed = false;
                                let english_label = language_label(UiLanguage::English).to_owned();
                                let japanese_label =
                                    language_label(UiLanguage::Japanese).to_owned();
                                egui::ComboBox::from_id_salt("language_select")
                                    .selected_text(language_label(self.config.language))
                                    .show_ui(ui, |ui| {
                                        language_changed |= ui
                                            .selectable_value(
                                                &mut self.config.language,
                                                UiLanguage::English,
                                                &english_label,
                                            )
                                            .changed();
                                        language_changed |= ui
                                            .selectable_value(
                                                &mut self.config.language,
                                                UiLanguage::Japanese,
                                                &japanese_label,
                                            )
                                            .changed();
                                    });
                                if language_changed {
                                    self.save_settings();
                                }

                                ui.add_space(20.0);
                                ui.label(
                                    egui::RichText::new(tr("settings.image_scaling"))
                                        .size(20.0)
                                        .strong(),
                                );
                                separator_pct(ui);

                                let visuals = ui.visuals_mut();
                                //visuals.selection.bg_fill = egui::Color32::BLACK;
                                visuals.override_text_color = None;

                                {
                                    let mut changed = false;
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.resize_method,
                                            ResizeMethod::None,
                                            egui::RichText::new(tr("settings.scaling.none")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.resize_method,
                                            ResizeMethod::Nearest,
                                            egui::RichText::new(tr("settings.scaling.nearest")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.resize_method,
                                            ResizeMethod::Triangle,
                                            egui::RichText::new(tr("settings.scaling.triangle")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.resize_method,
                                            ResizeMethod::CatmullRom,
                                            egui::RichText::new(tr("settings.scaling.catmullrom")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.resize_method,
                                            ResizeMethod::Lanczos3,
                                            egui::RichText::new(tr("settings.scaling.lanczos3")),
                                        )
                                        .clicked();

                                    if changed {
                                        self.reset_buffer();
                                        self.texture_cache.clear();
                                        self.textures = self.load_pair(self.current_index, ctx);
                                        self.save_settings(); // Save when algorithm changes
                                    }
                                }

                                ui.add_space(20.0);
                                ui.label(
                                    egui::RichText::new(tr("settings.page_view"))
                                        .size(20.0)
                                        .strong(),
                                );
                                separator_pct(ui);

                                {
                                    let previous_page_view = self.config.page_view_options;
                                    let mut changed = false;
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.page_view_options,
                                            PageViewOptions::Single,
                                            egui::RichText::new(tr("settings.page_view.single")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.page_view_options,
                                            PageViewOptions::DoubleRL,
                                            egui::RichText::new(tr("settings.page_view.double_rl")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.page_view_options,
                                            PageViewOptions::DoubleLR,
                                            egui::RichText::new(tr("settings.page_view.double_lr")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.page_view_options,
                                            PageViewOptions::TopDown,
                                            egui::RichText::new(tr("settings.page_view.top_down")),
                                        )
                                        .clicked();
                                    separator_pct(ui);
                                    ui.label(
                                        egui::RichText::new(tr("settings.page_view.center_offset"))
                                            .size(20.0)
                                            .strong(),
                                    );
                                    let slider_width = ui.available_width() * 0.98;
                                    let previous_slider_width = ui.spacing().slider_width;
                                    ui.spacing_mut().slider_width = slider_width - 120.0;
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.config.spread_center_offset,
                                                -150.0..=150.0,
                                            )
                                            .step_by(1.0),
                                        )
                                        .on_hover_text(tr(
                                            "settings.page_view.center_offset.tooltip",
                                        ))
                                        .changed();
                                    ui.spacing_mut().slider_width = previous_slider_width - 40.0;

                                    if changed {
                                        self.reset_buffer();
                                        self.texture_cache.clear();
                                        self.reset_pan();
                                        if self.config.page_view_options == PageViewOptions::TopDown
                                        {
                                            self.textures = [None, None];
                                            self.top_down_textures =
                                                vec![None; self.image_files.len()];
                                            self.top_down_scroll_offset = 0.0;
                                            self.ensure_top_down_loaded_around(
                                                self.current_index,
                                                ctx,
                                            );
                                        } else {
                                            if previous_page_view == PageViewOptions::TopDown {
                                                self.top_down_textures.clear();
                                            }
                                            self.textures = self.load_pair(self.current_index, ctx);
                                        }
                                        self.save_settings();
                                    }
                                }

                                ui.add_space(20.0);
                                ui.label(
                                    egui::RichText::new(tr("settings.last_page"))
                                        .size(20.0)
                                        .strong(),
                                );
                                separator_pct(ui);

                                {
                                    let mut changed = false;
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.last_page_action,
                                            LastPageAction::GotoNextFile,
                                            egui::RichText::new(tr("settings.last_page.next_file")),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.last_page_action,
                                            LastPageAction::ToFirstPage,
                                            egui::RichText::new(tr(
                                                "settings.last_page.first_page",
                                            )),
                                        )
                                        .clicked();
                                    changed |= ui
                                        .radio_value(
                                            &mut self.config.last_page_action,
                                            LastPageAction::Nothing,
                                            egui::RichText::new(tr("settings.last_page.nothing")),
                                        )
                                        .clicked();

                                    if changed {
                                        self.reset_buffer();
                                        self.textures = self.load_pair(self.current_index, ctx);
                                        self.save_settings();
                                    }
                                }

                                ui.add_space(20.0);
                                ui.label(
                                    egui::RichText::new(tr("settings.zoom")).size(20.0).strong(),
                                );
                                separator_pct(ui);

                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 160.0;
                                let mut zoom_factor = self.zoom_factor;
                                let zoom_slider = ui.add(
                                    egui::Slider::new(&mut zoom_factor, ZOOM_MIN..=ZOOM_MAX)
                                        .text(tr("settings.zoom.slider")),
                                );
                                ui.spacing_mut().slider_width = previous_slider_width;
                                if zoom_slider.changed() {
                                    self.apply_zoom_factor(zoom_factor, ctx, false);
                                }

                                if ui
                                    .button(egui::RichText::new(tr("settings.zoom.reset")))
                                    .clicked()
                                {
                                    self.reset_pan();
                                    self.apply_zoom_factor(1.0, ctx, false);
                                }
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new(tr("settings.image_sizing"))
                                        .size(18.0)
                                        .strong(),
                                );
                                let mut image_sizing_changed = false;
                                image_sizing_changed |= ui
                                    .radio_value(
                                        &mut self.config.image_sizing_mode,
                                        ImageSizingMode::FitHeight,
                                        egui::RichText::new(tr("settings.image_sizing.fit_height")),
                                    )
                                    .clicked();
                                image_sizing_changed |= ui
                                    .radio_value(
                                        &mut self.config.image_sizing_mode,
                                        ImageSizingMode::FitWidth,
                                        egui::RichText::new(tr("settings.image_sizing.fit_width")),
                                    )
                                    .clicked();
                                image_sizing_changed |= ui
                                    .radio_value(
                                        &mut self.config.image_sizing_mode,
                                        ImageSizingMode::FitBoth,
                                        egui::RichText::new(tr("settings.image_sizing.fit_both")),
                                    )
                                    .clicked();
                                image_sizing_changed |= ui
                                    .radio_value(
                                        &mut self.config.image_sizing_mode,
                                        ImageSizingMode::OriginalSize,
                                        egui::RichText::new(tr("settings.image_sizing.original")),
                                    )
                                    .clicked();
                                if image_sizing_changed {
                                    self.reset_buffer();
                                    self.texture_cache.clear();
                                    self.reset_pan();
                                    self.textures = self.load_pair(self.current_index, ctx);
                                    self.save_settings();
                                }
                                separator_pct(ui);

                                ui.add_space(20.0);
                                ui.label(
                                    egui::RichText::new(tr("settings.others"))
                                        .size(20.0)
                                        .strong(),
                                );
                                separator_pct(ui);
                                ui.horizontal(|ui| {
                                    ui.label(tr("settings.image_panel_background"));
                                    let mut background_color = self.image_panel_background_color();
                                    if egui::color_picker::color_edit_button_srgba(
                                        ui,
                                        &mut background_color,
                                        egui::color_picker::Alpha::Opaque,
                                    )
                                    .changed()
                                    {
                                        self.config.image_panel_background = [
                                            background_color.r(),
                                            background_color.g(),
                                            background_color.b(),
                                            background_color.a(),
                                        ];
                                        self.save_settings();
                                    }
                                });
                                ui.checkbox(
                                    &mut self.config.show_top_bar,
                                    tr("settings.show_toolbar"),
                                );
                                // setting for auto hide the setting button
                                if ui
                                    .checkbox(
                                        &mut self.config.auto_hide_settings_button,
                                        tr("settings.auto_hide_settings_button"),
                                    )
                                    .changed()
                                {
                                    self.save_settings();
                                }
                                ui.checkbox(
                                    &mut self.config.transparency_support,
                                    tr("settings.transparency"),
                                )
                                .on_hover_text(tr("settings.transparency.tooltip"));
                                ui.checkbox(
                                    &mut self.config.enable_auto_image_byte_fix,
                                    tr("settings.auto_image_fix"),
                                )
                                .on_hover_text(tr("settings.auto_image_fix.tooltip"));
                                ui.checkbox(
                                    &mut self.config.enable_single_file_caching,
                                    tr("settings.single_file_cache"),
                                )
                                .on_hover_text(tr("settings.single_file_cache.tooltip"));
                                if ui
                                    .checkbox(
                                        &mut self.config.enable_animated_images,
                                        tr("settings.animated_images"),
                                    )
                                    .on_hover_text(tr("settings.animated_images.tooltip"))
                                    .changed()
                                {
                                    self.clear_animated_image_cache();
                                    self.clear_animation_scan_state();
                                    self.clear_active_animations();
                                    if self.config.enable_animated_images {
                                        self.restart_animation_scan_for_current_window();
                                    }
                                    self.save_settings();
                                }
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.config.image_delay,
                                            0..=AUTO_SCROLL_MAX_DELAY_MS,
                                        )
                                        .text(tr("settings.image_delay")),
                                    )
                                    .on_hover_text(tr("settings.image_delay.tooltip"))
                                    .changed()
                                {
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                let mut slide_speed_slider =
                                    (self.config.top_down_image_slide_speed.max(0.005) / 0.05)
                                        .log10()
                                        * 10.0;
                                slide_speed_slider = slide_speed_slider.clamp(-10.0, 10.0);
                                if ui
                                    .add(
                                        egui::Slider::new(&mut slide_speed_slider, -10.0..=10.0)
                                            .text(tr("settings.top_down_slide_speed")),
                                    )
                                    .on_hover_text(tr("settings.top_down_slide_speed.tooltip"))
                                    .changed()
                                {
                                    self.config.top_down_image_slide_speed =
                                        0.05 * 10.0_f32.powf(slide_speed_slider / 10.0);
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.config.top_down_image_drag_speed,
                                            0.1..=5.0,
                                        )
                                        .text(tr("settings.top_down_drag_speed")),
                                    )
                                    .on_hover_text(tr("settings.top_down_drag_speed.tooltip"))
                                    .changed()
                                {
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.config.settings_button_width,
                                            3.0..=30.0,
                                        )
                                        .text(tr("settings.settings_button_width")),
                                    )
                                    .on_hover_text(tr("settings.settings_button_width.tooltip"))
                                    .changed()
                                {
                                    self.config.settings_button_width =
                                        self.config.settings_button_width.clamp(3.0, 30.0);
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.config.settings_button_x_offset,
                                            0.0..=20.0,
                                        )
                                        .text(tr("settings.settings_button_x_position")),
                                    )
                                    .on_hover_text(tr(
                                        "settings.settings_button_x_position.tooltip",
                                    ))
                                    .changed()
                                {
                                    self.config.settings_button_x_offset =
                                        self.config.settings_button_x_offset.clamp(0.0, 20.0);
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.config.settings_button_y_offset,
                                            -1.0..=1.0,
                                        )
                                        .text(tr("settings.settings_button_y_position")),
                                    )
                                    .on_hover_text(tr(
                                        "settings.settings_button_y_position.tooltip",
                                    ))
                                    .changed()
                                {
                                    self.config.settings_button_y_offset =
                                        self.config.settings_button_y_offset.clamp(-1.0, 1.0);
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width =
                                    self.config.settings_width * 0.9 - 190.0;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.config.top_toolbar_scale,
                                            1.0..=3.0,
                                        )
                                        .text(tr("settings.top_toolbar_scale")),
                                    )
                                    .on_hover_text(tr("settings.top_toolbar_scale.tooltip"))
                                    .changed()
                                {
                                    self.config.top_toolbar_scale =
                                        self.config.top_toolbar_scale.clamp(1.0, 3.0);
                                    self.save_settings();
                                }
                                ui.spacing_mut().slider_width = previous_slider_width;
                                ui.add_space(20.0);
                                ui.horizontal(|ui| {
                                    ui.selectable_value(
                                        &mut self.selected_control_profile,
                                        ControlProfile::Default,
                                        tr("settings.control.default_view"),
                                    );
                                    ui.selectable_value(
                                        &mut self.selected_control_profile,
                                        ControlProfile::TopDown,
                                        tr("settings.control.top_down_view"),
                                    );
                                });
                                separator_pct(ui);

                                let controls_changed = match self.selected_control_profile {
                                    ControlProfile::Default => render_control_mapping_settings(
                                        ui,
                                        ControlProfile::Default,
                                        &mut self.config.keys,
                                        &mut self.config.mouse,
                                        &mut self.config.gamepad,
                                        &mut self.config.double_click_threshold_ms,
                                        &mut self.binding_action,
                                    ),
                                    ControlProfile::TopDown => render_control_mapping_settings(
                                        ui,
                                        ControlProfile::TopDown,
                                        &mut self.config.top_down_keys,
                                        &mut self.config.top_down_mouse,
                                        &mut self.config.top_down_gamepad,
                                        &mut self.config.top_down_double_click_threshold_ms,
                                        &mut self.binding_action,
                                    ),
                                };
                                if controls_changed {
                                    self.save_settings();
                                }

                                ui.add_space(50.0);
                            });
                        });
                });
        }

        // This allows opening/closing the settings
        let screen_rect = ctx.content_rect();
        let button_width = self.config.settings_button_width.clamp(3.0, 30.0);
        let button_height = 140.0;

        // Calculate X position based on whether panel is open
        let base_x_pos = if self.config.show_settings {
            screen_rect.max.x - self.config.settings_width - button_width - 20.0
        } else {
            screen_rect.max.x
        };
        let x_pos = base_x_pos + self.config.settings_button_x_offset.clamp(0.0, 20.0);

        // Calculate Y position to center the 200px button vertically
        let centered_y_pos = screen_rect.center().y - (button_height / 2.0);
        let min_y_pos = screen_rect.min.y;
        let max_y_pos = screen_rect.max.y - button_height;
        let y_pos =
            centered_y_pos + self.config.settings_button_y_offset * ((max_y_pos - min_y_pos) / 2.0);
        let y_pos = y_pos.clamp(min_y_pos, max_y_pos);

        // check whether we should auto hide the setting button
        let settings_toggle_reveal_width = 60.0
            + if self.config.show_settings {
                self.config.settings_width
            } else {
                0.0
            };
        let is_pointer_near_right_edge = ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|pos| screen_rect.max.x - pos.x <= settings_toggle_reveal_width);
        let settings_toggle_pinned = self.config.show_settings
            || !self.config.auto_hide_settings_button
            || self.image_files.is_empty();
        let should_refresh_settings_toggle = settings_toggle_pinned || is_pointer_near_right_edge;
        if should_refresh_settings_toggle {
            self.settings_toggle_last_visible_at = Instant::now();
        }
        let fade_elapsed =
            Instant::now().saturating_duration_since(self.settings_toggle_last_visible_at);
        let settings_toggle_opacity = if should_refresh_settings_toggle {
            1.0
        } else {
            (1.0 - fade_elapsed.as_secs_f32() / SETTINGS_TOGGLE_FADE_DURATION.as_secs_f32())
                .clamp(0.0, 1.0)
        };

        if settings_toggle_opacity > 0.0 {
            if settings_toggle_opacity < 1.0 {
                ctx.request_repaint();
            }
            egui::Area::new(egui::Id::new("settings_toggle"))
                .fixed_pos([x_pos, y_pos])
                .show(ctx, |ui| {
                    let text = if self.config.show_settings {
                        "▶"
                    } else {
                        "◀"
                    };

                    ui.scope(|ui| {
                        let mut visuals = ui.style().visuals.clone();
                        visuals.widgets.inactive.bg_fill = visuals
                            .widgets
                            .inactive
                            .bg_fill
                            .gamma_multiply(settings_toggle_opacity);
                        visuals.widgets.inactive.weak_bg_fill = visuals
                            .widgets
                            .inactive
                            .weak_bg_fill
                            .gamma_multiply(settings_toggle_opacity);
                        visuals.widgets.inactive.bg_stroke.color = visuals
                            .widgets
                            .inactive
                            .bg_stroke
                            .color
                            .gamma_multiply(settings_toggle_opacity);
                        visuals.widgets.inactive.fg_stroke.color = visuals
                            .widgets
                            .inactive
                            .fg_stroke
                            .color
                            .gamma_multiply(settings_toggle_opacity);
                        visuals.override_text_color = Some(
                            ui.visuals()
                                .text_color()
                                .gamma_multiply(settings_toggle_opacity),
                        );
                        ui.style_mut().visuals = visuals;

                        let toggle_btn = egui::Button::new(
                            egui::RichText::new(text)
                                .size(if button_width < 16.0 { 0.0 } else { 16.0 })
                                .color(ui.visuals().text_color()),
                        );
                        if ui
                            .add_sized([button_width, button_height], toggle_btn)
                            .clicked()
                        {
                            self.config.show_settings = !self.config.show_settings;
                            self.settings_toggle_last_visible_at = Instant::now();
                        }
                    });
                });
        }

        if self.config.show_top_bar && !self.is_fullscreen {
            egui::TopBottomPanel::top("top_toolbar")
                .frame(
                    egui::Frame::NONE
                        .fill(egui::Color32::from_gray(30))
                        .inner_margin(4.0),
                )
                .show(ctx, |ui| {
                    ui.scope(|ui| {
                        let toolbar_scale = self.config.top_toolbar_scale.clamp(1.0, 3.0);
                        let style = ui.style_mut();
                        style.spacing.interact_size.y *= toolbar_scale;
                        style.spacing.button_padding *= toolbar_scale;
                        style.spacing.item_spacing *= toolbar_scale;
                        if let Some(font_id) = style.text_styles.get_mut(&egui::TextStyle::Button) {
                            font_id.size *= toolbar_scale;
                        }
                        if let Some(font_id) = style.text_styles.get_mut(&egui::TextStyle::Body) {
                            font_id.size *= toolbar_scale;
                        }

                        ui.horizontal(|ui| {
                            // --- Folder Navigation ---
                            if ui
                                .button("📁⏮")
                                .on_hover_text(tr("toolbar.prev_folder"))
                                .clicked()
                            {
                                self.prev_folder(ctx);
                            }
                            if ui
                                .button("⏭📁")
                                .on_hover_text(tr("toolbar.next_folder"))
                                .clicked()
                            {
                                self.next_folder(ctx);
                            }
                            ui.separator();

                            // --- File Navigation ---
                            if ui
                                .button("📦⏮")
                                .on_hover_text(tr("toolbar.prev_file"))
                                .clicked()
                            {
                                self.prev_zip(ctx);
                            }
                            if ui
                                .button("⏭📦")
                                .on_hover_text(tr("toolbar.next_file"))
                                .clicked()
                            {
                                self.next_zip(ctx);
                            }
                            ui.separator();

                            // --- Page Navigation ---
                            if ui
                                .button("⏮")
                                .on_hover_text(tr("toolbar.first_page"))
                                .clicked()
                            {
                                self.go_to_first_page(ctx);
                            }
                            if ui
                                .button("◀")
                                .on_hover_text(tr("toolbar.prev_page"))
                                .clicked()
                            {
                                self.prev_page(ctx, false);
                            }

                            // Page Indicator in middle
                            ui.label(format!(
                                "{} / {}",
                                self.current_index + 1,
                                self.image_files.len()
                            ));

                            if ui
                                .button("▶")
                                .on_hover_text(tr("toolbar.next_page"))
                                .clicked()
                            {
                                self.next_page(ctx, false);
                            }
                            if ui
                                .button("⏭")
                                .on_hover_text(tr("toolbar.last_page"))
                                .clicked()
                            {
                                self.go_to_last_page(ctx);
                            }
                            ui.separator();

                            if ui
                                .button("1◀")
                                .on_hover_text(tr("toolbar.prev_page"))
                                .clicked()
                            {
                                self.prev_page(ctx, true);
                            }

                            if ui
                                .button("▶1")
                                .on_hover_text(tr("toolbar.next_page"))
                                .clicked()
                            {
                                self.next_page(ctx, true);
                            }
                            ui.separator();

                            // --- View Toggles ---
                            let shift_label = if self.is_shifted {
                                tr("toolbar.odd")
                            } else {
                                tr("toolbar.even")
                            };
                            let shift_tooltip = if self.is_shifted {
                                tr("state.odd_page")
                            } else {
                                tr("state.even_page")
                            };
                            if ui
                                .button(shift_label)
                                .on_hover_text(shift_tooltip)
                                .clicked()
                            {
                                self.change_shifted_mode(ctx);
                            }

                            if ui
                                .button(if self.is_fullscreen { "🗗" } else { "⛶" })
                                .on_hover_text(tr("toolbar.fullscreen"))
                                .clicked()
                            {
                                self.is_fullscreen = !self.is_fullscreen;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                    self.is_fullscreen,
                                ));
                            }
                            ui.separator();
                            if ui
                                .button("📂")
                                .on_hover_text(tr("toolbar.open_file"))
                                .clicked()
                            {
                                self.open_file_dialog();
                            }

                            ui.separator();
                            let auto_scroll_label = if self.auto_scroll_enabled {
                                "⏸"
                            } else {
                                "▶"
                            };
                            let auto_scroll_tooltip = if self.auto_scroll_enabled {
                                tr("toolbar.auto_scroll_stop")
                            } else {
                                tr("toolbar.auto_scroll_start")
                            };
                            if ui
                                .add_enabled(
                                    !self.image_files.is_empty(),
                                    egui::Button::new(auto_scroll_label),
                                )
                                .on_hover_text(auto_scroll_tooltip)
                                .clicked()
                            {
                                self.toggle_auto_scroll(ctx);
                            }

                            let mut auto_scroll_delay_ms = self.auto_scroll_delay_ms();
                            let delay_slider = egui::Slider::new(
                                &mut auto_scroll_delay_ms,
                                AUTO_SCROLL_MIN_DELAY_MS..=AUTO_SCROLL_MAX_DELAY_MS,
                            )
                            .text(tr("toolbar.auto_scroll_delay"));
                            let delay_response = ui.add_sized(
                                [180.0 * toolbar_scale, ui.spacing().interact_size.y],
                                delay_slider,
                            );
                            if delay_response.changed() {
                                self.config.image_delay = auto_scroll_delay_ms;
                                self.save_settings();
                                if self.auto_scroll_enabled {
                                    self.schedule_auto_scroll(ctx);
                                }
                            }
                            delay_response.on_hover_text(tr("toolbar.auto_scroll_delay.tooltip"));

                            if ui
                                .add_enabled(!self.image_files.is_empty(), egui::Button::new("↻"))
                                .on_hover_text(tr("toolbar.reload_current_image.tooltip"))
                                .clicked()
                            {
                                self.reload_current_image(ctx);
                            }

                            ui.separator();
                            // --- THE SLIDER ---
                            // We use a 1-based slider for better user experience
                            let mut page_val = self.current_index + 1;
                            let max_pages = self.image_files.len().max(1);

                            // ui.available_width() ensures the slider stretches to fill the gap
                            let slider_width = ui.available_width() / 3.0; // Reserve space for right-side buttons

                            ui.style_mut().spacing.slider_width = slider_width;

                            let slider = ui.add(
                                egui::Slider::new(&mut page_val, 1..=max_pages)
                                    .show_value(true)
                                    .text(format!("/ {}", max_pages)),
                            );
                            self.is_scrubbing = slider.dragged();
                            if slider.changed() {
                                self.current_index = page_val - 1;
                                self.reset_buffer();
                                self.reset_pan();
                                self.textures = self.load_pair(self.current_index, ctx);
                            }

                            // --- Hide Button ---
                            // --- Hide Button ---
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("❌").on_hover_text(tr("toolbar.hide")).clicked()
                                    {
                                        self.config.show_top_bar = false;
                                    }
                                    if ui.button("⚙").on_hover_text(tr("settings.title")).clicked()
                                    {
                                        self.config.show_settings = !self.config.show_settings;
                                    }
                                },
                            );
                        });
                    });
                });
        }

        if self.auto_scroll_enabled {
            if self.image_files.is_empty() {
                self.stop_auto_scroll();
            } else if action_to_run == MangaAction::None {
                if let Some(next_tick) = self.auto_scroll_next_tick {
                    let now = Instant::now();
                    if now >= next_tick {
                        let before_path = self.zip_path.clone();
                        let before_index = self.current_index;
                        self.next_page(ctx, false);
                        let page_advanced =
                            self.current_index != before_index || self.zip_path != before_path;
                        if page_advanced {
                            self.schedule_auto_scroll(ctx);
                        } else {
                            self.stop_auto_scroll();
                        }
                    } else {
                        ctx.request_repaint_after(next_tick - now);
                    }
                } else {
                    self.schedule_auto_scroll(ctx);
                }
            } else {
                self.schedule_auto_scroll(ctx);
            }
        }

        self.render_page_view(ctx, &mut action_to_run);

        if action_to_run != MangaAction::None {
            self.execute_action(action_to_run, ctx);
        }

        // Keep preloading buffers
        self.update_buffers(ctx);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

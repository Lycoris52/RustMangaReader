use crate::config::{
    AppSettings, GamepadButton, GamepadConfig, ImageSizingMode, LastPageAction, MangaAction,
    MouseButton, MouseGesture, PageViewOptions, ResizeMethod, Shortcut, SourceMode, UiLanguage,
};
use crate::font;
use crate::localize::{set_language, tr};
use crate::utils::{windows_natural_sort, windows_natural_sort_strings};
use eframe::egui;
use egui::{Align, Direction, PointerButton, Rect};
use gilrs::{Button as GilrsButton, EventType, Gilrs};
use image::{DynamicImage, ImageFormat};
use pdfium_render::prelude::Pixels;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const LONG_CLICK_DURATION: Duration = Duration::from_millis(450);
const GAMEPAD_INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(800);
const MOUSE_DRAG_THRESHOLD: f32 = 6.0;

#[derive(Clone, PartialEq)]
enum BindingTarget {
    Keyboard(String),
}

pub struct MangaReader {
    zip_path: Option<PathBuf>,
    image_files: Vec<String>,
    current_index: usize,
    textures: [Option<egui::TextureHandle>; 2],
    buffer_next: [Option<egui::TextureHandle>; 2],
    buffer_prev: [Option<egui::TextureHandle>; 2],
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
    initial_path: Option<PathBuf>,
    source_mode: SourceMode,
    last_image_switch_time: Instant,
    zoom_factor: f32,
    pan_offset: egui::Vec2,
    is_scrubbing: bool,
    mouse_press_started: [Option<Instant>; 5],
    mouse_press_origin: [Option<egui::Pos2>; 5],
    mouse_long_triggered: [bool; 5],
    mouse_drag_suppressed: [bool; 5],
    pending_mouse_click: [Option<(Instant, MangaAction)>; 5],
    gamepad: Option<Gilrs>,
    gamepad_repeat_deadlines: [Option<Instant>; 16],
}

impl MangaReader {
    pub fn new(_cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        font::setup_custom_fonts(&_cc.egui_ctx);
        let mut exe_path = env::current_exe().expect("Failed to get current exe path");
        exe_path.pop();
        exe_path.push("settings.json");
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
            source_mode: SourceMode::Zip,
            last_image_switch_time: Instant::now(),
            zoom_factor: 1.0,
            pan_offset: egui::Vec2::ZERO,
            is_scrubbing: false,
            mouse_press_started: [None; 5],
            mouse_press_origin: [None; 5],
            mouse_long_triggered: [false; 5],
            mouse_drag_suppressed: [false; 5],
            pending_mouse_click: [None; 5],
            gamepad: Gilrs::new().ok(),
            gamepad_repeat_deadlines: [None; 16],
        }
    }

    fn save_settings(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.config) {
            // write the config file in executable directory
            let mut exe_path = env::current_exe().expect("Failed to get current exe path");
            exe_path.pop();
            exe_path.push("settings.json");
            let _ = std::fs::write(exe_path, json);
        }
    }

    fn language_label(&self, language: UiLanguage) -> &str {
        match language {
            UiLanguage::English => tr("language.english"),
            UiLanguage::Japanese => tr("language.japanese"),
        }
    }

    fn action_label(&self, action: MangaAction) -> &str {
        match action {
            MangaAction::None => tr("action.none"),
            MangaAction::NextPage => tr("action.next_page"),
            MangaAction::PrevPage => tr("action.prev_page"),
            MangaAction::OneNextPage => tr("action.one_next_page"),
            MangaAction::OnePrevPage => tr("action.one_prev_page"),
            MangaAction::FirstPage => tr("action.first_page"),
            MangaAction::LastPage => tr("action.last_page"),
            MangaAction::NextFile => tr("action.next_file"),
            MangaAction::PrevFile => tr("action.prev_file"),
            MangaAction::NextFolder => tr("action.next_folder"),
            MangaAction::PrevFolder => tr("action.prev_folder"),
            MangaAction::FullScreen => tr("action.fullscreen"),
            MangaAction::ViewMode => tr("action.view_mode"),
            MangaAction::OpenFile => tr("action.open_file"),
            MangaAction::QuitApp => tr("action.quit_app"),
        }
    }

    fn gamepad_button_label(&self, button: GamepadButton) -> &'static str {
        match button {
            GamepadButton::South => tr("settings.gamepad.south"),
            GamepadButton::East => tr("settings.gamepad.east"),
            GamepadButton::North => tr("settings.gamepad.north"),
            GamepadButton::West => tr("settings.gamepad.west"),
            GamepadButton::LeftTrigger => tr("settings.gamepad.left_trigger"),
            GamepadButton::LeftTrigger2 => tr("settings.gamepad.left_trigger2"),
            GamepadButton::RightTrigger => tr("settings.gamepad.right_trigger"),
            GamepadButton::RightTrigger2 => tr("settings.gamepad.right_trigger2"),
            GamepadButton::Select => tr("settings.gamepad.select"),
            GamepadButton::Start => tr("settings.gamepad.start"),
            GamepadButton::LeftThumb => tr("settings.gamepad.left_thumb"),
            GamepadButton::RightThumb => tr("settings.gamepad.right_thumb"),
            GamepadButton::DPadUp => tr("settings.gamepad.dpad_up"),
            GamepadButton::DPadDown => tr("settings.gamepad.dpad_down"),
            GamepadButton::DPadLeft => tr("settings.gamepad.dpad_left"),
            GamepadButton::DPadRight => tr("settings.gamepad.dpad_right"),
        }
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
            ImageSizingMode::OriginalSize => tex_size * zoom_factor,
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

    fn scan_folder(&mut self, current_parent: &Path) -> Vec<PathBuf> {
        let mut items = Vec::new();
        if let Ok(entries) = fs::read_dir(current_parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_zip = path.extension().map_or(false, |ext| {
                    ext == "zip" || ext == "pdf" || ext == "cbz" || ext == "cbr" || ext == "rar"
                });
                // Treat non-hidden directories as readable manga sources
                let is_dir = path.is_dir()
                    && !path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .starts_with('.');

                if is_zip || is_dir {
                    items.push(path);
                }
            }
        }
        windows_natural_sort(&mut items);
        items
    }

    fn get_adjacent_directories(
        path_with_file_name: Option<PathBuf>,
    ) -> (Option<PathBuf>, Option<PathBuf>) {
        // Unwrap the Option to get the actual PathBuf
        let path = match path_with_file_name {
            Some(p) => p,
            None => return (None, None),
        };

        let path = match path.parent() {
            Some(p) => p,
            None => return (None, None),
        };

        let root_dir = match path.parent() {
            Some(p) => p,
            None => return (None, None),
        };

        // Collect all valid sibling directories
        let mut dirs: Vec<PathBuf> = fs::read_dir(root_dir)
            .ok()
            .map(|read_dir| {
                read_dir
                    .filter_map(|entry| {
                        let p = entry.ok()?.path();
                        // Ensure it's a directory and not a hidden file
                        if p.is_dir() && !p.file_name()?.to_str()?.starts_with('.') {
                            Some(p)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Sort using Windows natural alphanumeric order (test2 before test10)
        // If you haven't added a crate, dirs.sort() works for simple cases
        windows_natural_sort(&mut *dirs);

        // Find where we are
        let current_pos = dirs.iter().position(|p| *p == path);

        match current_pos {
            Some(pos) => {
                let prev = if pos > 0 {
                    Some(dirs[pos - 1].clone())
                } else {
                    None
                };
                let next = if pos + 1 < dirs.len() {
                    Some(dirs[pos + 1].clone())
                } else {
                    None
                };
                (prev, next)
            }
            None => (None, None),
        }
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
                            buffer = self.strip_adobe_app14_if_invalid(&buffer);
                        }
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
                                if let Ok(img) =
                                    image::load_from_memory_with_format(&buffer, ImageFormat::Tga)
                                {
                                    pair[i] = self.load_texture(img, filename.clone(), ctx);
                                }
                            }
                        }
                    }
                }
            }
        }
        pair
    }

    fn strip_adobe_app14_if_invalid(&self, bytes: &[u8]) -> Vec<u8> {
        let mut i = 2;
        let mut out = Vec::with_capacity(bytes.len());

        // Copy SOI first (must be first two bytes)
        if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
            return bytes.to_vec(); // not a valid jpeg
        }

        out.extend_from_slice(&bytes[0..2]);

        while i < bytes.len() {
            if i + 1 >= bytes.len() {
                break;
            }

            // Every marker must start with FF
            if bytes[i] != 0xFF {
                // Start of entropy data (after SOS)
                out.extend_from_slice(&bytes[i..]);
                break;
            }

            let marker = bytes[i + 1];

            // Standalone markers (no length)
            if marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
                out.push(0xFF);
                out.push(marker);
                i += 2;
                continue;
            }

            // SOS marker copy rest of file and stop parsing
            if marker == 0xDA {
                out.extend_from_slice(&bytes[i..]);
                break;
            }

            if i + 4 > bytes.len() {
                break;
            }

            let length = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;

            let segment_end = i + 2 + length;

            if segment_end > bytes.len() {
                break;
            }

            // If APP14 (FF EE)
            if marker == 0xEE && length >= 14 && &bytes[i + 4..i + 9] == b"Adobe" {
                println!("Stripping Adobe APP14 segment");
                // Skip this segment entirely
                i = segment_end;
                continue;
            }

            // Otherwise copy full segment
            out.extend_from_slice(&bytes[i..segment_end]);
            i = segment_end;
        }

        out
    }

    /// Helper to render a specific page
    fn render_pdf_page(&self, index: usize, ctx: &egui::Context) -> Option<DynamicImage> {
        // You would typically store a reference to the loaded document
        // and render the page here.
        let pdfium = pdfium_render::prelude::Pdfium::default();
        let doc = pdfium
            .load_pdf_from_file(self.zip_path.as_ref()?, None)
            .ok()?;
        let page = doc.pages().get(index as u16).ok()?;
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
        let resize_start = Instant::now();
        let filter = self.config.resize_method.to_filter();
        let processed_img = if let Some(filter_type) = filter {
            if self.config.image_sizing_mode == ImageSizingMode::OriginalSize {
                img
            } else {
                let container_size = self.effective_image_container_size(ctx);
                let aspect_ratio = img.width() as f32 / img.height() as f32;
                let factor = if self.zoom_factor != 1.0 { 3 } else { 1 };
                let (target_w, target_h) = match self.config.image_sizing_mode {
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
                    ImageSizingMode::OriginalSize => unreachable!(),
                };
                img.resize(
                    target_w.max(1) * factor,
                    target_h.max(1) * factor,
                    filter_type,
                )
            }
        } else {
            img // No resizing needed, return original
        };

        let _resize_time = resize_start.elapsed();
        let process_start = Instant::now();

        let size = [processed_img.width() as _, processed_img.height() as _];
        let color_img = if self.config.transparency_support {
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
                // Initialize Pdfium (you may need to bundle the dll/so/dylib)
                let pdfium = pdfium_render::prelude::Pdfium::default();
                if let Ok(doc) = pdfium.load_pdf_from_file(&path, None) {
                    let page_count = doc.pages().len();
                    for i in 0..page_count {
                        // We use a virtual naming scheme for PDF pages in our image_files list
                        images.push(format!("pdf_page_{}", i));
                    }
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
            self.all_zips_in_folder =
                self.scan_folder(&target_path.parent().unwrap_or(Path::new("")));

            if let Some(file_name) = target_path.file_name() {
                self.zip_name_display =
                    Some((file_name.to_string_lossy().to_string(), Instant::now()));
            }

            self.textures = self.load_pair(self.current_index, ctx);
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
            // If the next pages are already in the buffer, swap them in
            if self.buffer_next[0].is_some() {
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
            // Use the previous buffer textures
            if self.buffer_prev[0].is_some() {
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
        let (_, next_dir) = Self::get_adjacent_directories(self.zip_path.clone());

        // Check if next_dir actually exists
        if let Some(dir) = next_dir {
            let zips = self.scan_folder(&*dir);
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
        let (prev_dir, _) = Self::get_adjacent_directories(self.zip_path.clone());

        // Check if next_dir actually exists
        if let Some(dir) = prev_dir {
            let zips = self.scan_folder(&*dir);
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
    }

    fn mouse_button_to_pointer(button: MouseButton) -> PointerButton {
        match button {
            MouseButton::Button1 => PointerButton::Primary,
            MouseButton::Button2 => PointerButton::Secondary,
            MouseButton::Button3 => PointerButton::Middle,
            MouseButton::Button4 => PointerButton::Extra1,
            MouseButton::Button5 => PointerButton::Extra2,
        }
    }

    fn mouse_button_index(button: MouseButton) -> usize {
        match button {
            MouseButton::Button1 => 0,
            MouseButton::Button2 => 1,
            MouseButton::Button3 => 2,
            MouseButton::Button4 => 3,
            MouseButton::Button5 => 4,
        }
    }

    fn get_mouse_action(&self, gesture: MouseGesture) -> MangaAction {
        match gesture {
            MouseGesture::Unassigned => MangaAction::None,
            MouseGesture::ScrollUp => self.config.mouse.scroll_up,
            MouseGesture::ScrollDown => self.config.mouse.scroll_down,
            MouseGesture::Click(MouseButton::Button1) => self.config.mouse.button1_click,
            MouseGesture::Click(MouseButton::Button2) => self.config.mouse.button2_click,
            MouseGesture::Click(MouseButton::Button3) => self.config.mouse.button3_click,
            MouseGesture::Click(MouseButton::Button4) => self.config.mouse.button4_click,
            MouseGesture::Click(MouseButton::Button5) => self.config.mouse.button5_click,
            MouseGesture::DoubleClick(MouseButton::Button1) => {
                self.config.mouse.button1_double_click
            }
            MouseGesture::DoubleClick(MouseButton::Button2) => {
                self.config.mouse.button2_double_click
            }
            MouseGesture::DoubleClick(MouseButton::Button3) => {
                self.config.mouse.button3_double_click
            }
            MouseGesture::DoubleClick(MouseButton::Button4) => {
                self.config.mouse.button4_double_click
            }
            MouseGesture::DoubleClick(MouseButton::Button5) => {
                self.config.mouse.button5_double_click
            }
            MouseGesture::LongClick(MouseButton::Button1) => self.config.mouse.button1_long_click,
            MouseGesture::LongClick(MouseButton::Button2) => self.config.mouse.button2_long_click,
            MouseGesture::LongClick(MouseButton::Button3) => self.config.mouse.button3_long_click,
            MouseGesture::LongClick(MouseButton::Button4) => self.config.mouse.button4_long_click,
            MouseGesture::LongClick(MouseButton::Button5) => self.config.mouse.button5_long_click,
        }
    }

    fn map_gamepad_button(button: GilrsButton) -> Option<GamepadButton> {
        match button {
            GilrsButton::South => Some(GamepadButton::South),
            GilrsButton::East => Some(GamepadButton::East),
            GilrsButton::North => Some(GamepadButton::North),
            GilrsButton::West => Some(GamepadButton::West),
            GilrsButton::LeftTrigger => Some(GamepadButton::LeftTrigger),
            GilrsButton::LeftTrigger2 => Some(GamepadButton::LeftTrigger2),
            GilrsButton::RightTrigger => Some(GamepadButton::RightTrigger),
            GilrsButton::RightTrigger2 => Some(GamepadButton::RightTrigger2),
            GilrsButton::Select => Some(GamepadButton::Select),
            GilrsButton::Start => Some(GamepadButton::Start),
            GilrsButton::LeftThumb => Some(GamepadButton::LeftThumb),
            GilrsButton::RightThumb => Some(GamepadButton::RightThumb),
            GilrsButton::DPadUp => Some(GamepadButton::DPadUp),
            GilrsButton::DPadDown => Some(GamepadButton::DPadDown),
            GilrsButton::DPadLeft => Some(GamepadButton::DPadLeft),
            GilrsButton::DPadRight => Some(GamepadButton::DPadRight),
            _ => None,
        }
    }

    fn gamepad_button_index(button: GamepadButton) -> usize {
        match button {
            GamepadButton::South => 0,
            GamepadButton::East => 1,
            GamepadButton::North => 2,
            GamepadButton::West => 3,
            GamepadButton::LeftTrigger => 4,
            GamepadButton::LeftTrigger2 => 5,
            GamepadButton::RightTrigger => 6,
            GamepadButton::RightTrigger2 => 7,
            GamepadButton::Select => 8,
            GamepadButton::Start => 9,
            GamepadButton::LeftThumb => 10,
            GamepadButton::RightThumb => 11,
            GamepadButton::DPadUp => 12,
            GamepadButton::DPadDown => 13,
            GamepadButton::DPadLeft => 14,
            GamepadButton::DPadRight => 15,
        }
    }

    fn gamepad_action_for_button(config: GamepadConfig, button: GamepadButton) -> MangaAction {
        match button {
            GamepadButton::South => config.south,
            GamepadButton::East => config.east,
            GamepadButton::North => config.north,
            GamepadButton::West => config.west,
            GamepadButton::LeftTrigger => config.left_trigger,
            GamepadButton::LeftTrigger2 => config.left_trigger2,
            GamepadButton::RightTrigger => config.right_trigger,
            GamepadButton::RightTrigger2 => config.right_trigger2,
            GamepadButton::Select => config.select,
            GamepadButton::Start => config.start,
            GamepadButton::LeftThumb => config.left_thumb,
            GamepadButton::RightThumb => config.right_thumb,
            GamepadButton::DPadUp => config.dpad_up,
            GamepadButton::DPadDown => config.dpad_down,
            GamepadButton::DPadLeft => config.dpad_left,
            GamepadButton::DPadRight => config.dpad_right,
        }
    }

    fn is_repeatable_gamepad_action(action: MangaAction) -> bool {
        matches!(
            action,
            MangaAction::NextPage
                | MangaAction::PrevPage
                | MangaAction::OneNextPage
                | MangaAction::OnePrevPage
                | MangaAction::NextFile
                | MangaAction::PrevFile
                | MangaAction::NextFolder
                | MangaAction::PrevFolder
        )
    }

    fn gamepad_repeat_interval(&self) -> Duration {
        Duration::from_millis(self.config.image_delay.max(80))
    }

    fn collect_gamepad_action(&mut self, ctx: &egui::Context) -> Option<MangaAction> {
        let gamepad_config = self.config.gamepad;
        let repeat_interval = self.gamepad_repeat_interval();
        let gilrs = self.gamepad.as_mut()?;
        let mut action_to_run = None;
        let now = Instant::now();

        while let Some(event) = gilrs.next_event() {
            match event.event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(mapped_button) = Self::map_gamepad_button(button) {
                        let action = Self::gamepad_action_for_button(gamepad_config, mapped_button);
                        let index = Self::gamepad_button_index(mapped_button);
                        if Self::is_repeatable_gamepad_action(action) {
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
                    if let Some(mapped_button) = Self::map_gamepad_button(button) {
                        let index = Self::gamepad_button_index(mapped_button);
                        self.gamepad_repeat_deadlines[index] = None;
                    }
                }
                _ => {}
            }
        }

        let now = Instant::now();
        for button in GamepadButton::ALL {
            let index = Self::gamepad_button_index(button);
            if let Some(deadline) = self.gamepad_repeat_deadlines[index] {
                if now >= deadline {
                    let action = Self::gamepad_action_for_button(gamepad_config, button);
                    if Self::is_repeatable_gamepad_action(action) {
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
        Duration::from_millis(self.config.double_click_threshold_ms)
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
                let index = Self::mouse_button_index(button);
                if !ctx.input(|i| i.pointer.button_down(Self::mouse_button_to_pointer(button))) {
                    self.mouse_press_started[index] = None;
                    self.mouse_press_origin[index] = None;
                    self.mouse_long_triggered[index] = false;
                    self.mouse_drag_suppressed[index] = false;
                }
            }
            return None;
        }

        for button in MouseButton::ALL {
            let pointer_button = Self::mouse_button_to_pointer(button);
            let button_index = Self::mouse_button_index(button);
            let now = Instant::now();
            let is_down = ctx.input(|i| i.pointer.button_down(pointer_button));

            if is_down {
                if self.mouse_press_started[button_index].is_none() {
                    self.mouse_press_started[button_index] = Some(now);
                    self.mouse_press_origin[button_index] =
                        ctx.input(|i| i.pointer.interact_pos());
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

            // Render the image on top
            if let Some(tex) = &self.textures[tex_index] {
                let previous_clip_rect = ui.clip_rect();
                ui.set_clip_rect(previous_clip_rect.intersect(visible_rect));
                let layout = egui::Layout::top_down(align);
                ui.with_layout(layout, |ui| {
                    ui.add(
                        egui::Image::new(tex)
                            .fit_to_exact_size(image_size)
                            .maintain_aspect_ratio(true),
                    );
                });
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
        self.config.page_view_options == PageViewOptions::Single
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
                BindingTarget::Keyboard(action_name) => {
                    ctx.input(|i| {
                        for key in egui::Key::ALL {
                            if i.key_pressed(*key) {
                                let new_shortcut = Shortcut {
                                    key: *key,
                                    ctrl: i.modifiers.ctrl,
                                    alt: i.modifiers.alt,
                                    shift: i.modifiers.shift,
                                };

                                match action_name.as_str() {
                                    "Next Page" => self.config.keys.next_page = new_shortcut,
                                    "Previous Page" => self.config.keys.prev_page = new_shortcut,
                                    "1 Next Page" => self.config.keys.one_next_page = new_shortcut,
                                    "1 Prev Page" => self.config.keys.one_prev_page = new_shortcut,
                                    "First Page" => self.config.keys.first_page = new_shortcut,
                                    "Last Page" => self.config.keys.last_page = new_shortcut,
                                    "Next File" => self.config.keys.next_file = new_shortcut,
                                    "Previous File" => self.config.keys.prev_file = new_shortcut,
                                    "Next Folder" => self.config.keys.next_folder = new_shortcut,
                                    "Previous Folder" => {
                                        self.config.keys.prev_folder = new_shortcut
                                    }
                                    "Toggle Fullscreen" => {
                                        self.config.keys.fullscreen = new_shortcut
                                    }
                                    "View Mode" => self.config.keys.view_mode = new_shortcut,
                                    "Open File" => self.config.keys.open_file = new_shortcut,
                                    "Quit App" => self.config.keys.quit_app = new_shortcut,
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
                let keys = self.config.keys;

                // Helper to check if a shortcut is triggered
                let is_triggered = |s: &Shortcut| {
                    i.key_pressed(s.key)
                        && i.modifiers.ctrl == s.ctrl
                        && i.modifiers.alt == s.alt
                        && i.modifiers.shift == s.shift
                };

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
                        .auto_shrink([false; 2])       // don't shrink content; enable scrolling
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
                                ui.label(egui::RichText::new(tr("language.label")).size(20.0).strong());
                                separator_pct(ui);
                                let mut language_changed = false;
                                let english_label = self.language_label(UiLanguage::English).to_owned();
                                let japanese_label = self.language_label(UiLanguage::Japanese).to_owned();
                                egui::ComboBox::from_id_salt("language_select")
                                    .selected_text(self.language_label(self.config.language))
                                    .show_ui(ui, |ui| {
                                        language_changed |= ui.selectable_value(&mut self.config.language, UiLanguage::English, &english_label).changed();
                                        language_changed |= ui.selectable_value(&mut self.config.language, UiLanguage::Japanese, &japanese_label).changed();
                                    });
                                if language_changed {
                                    self.save_settings();
                                }

                                ui.add_space(20.0);
                                ui.label(egui::RichText::new(tr("settings.image_scaling")).size(20.0).strong());
                                separator_pct(ui);

                                let visuals = ui.visuals_mut();
                                //visuals.selection.bg_fill = egui::Color32::BLACK;
                                visuals.override_text_color = None;

                                {
                                    let mut changed = false;
                                    changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::None, egui::RichText::new(tr("settings.scaling.none"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::Nearest, egui::RichText::new(tr("settings.scaling.nearest"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::Triangle, egui::RichText::new(tr("settings.scaling.triangle"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::CatmullRom, egui::RichText::new(tr("settings.scaling.catmullrom"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::Lanczos3, egui::RichText::new(tr("settings.scaling.lanczos3"))).clicked();

                                    if changed {
                                        self.reset_buffer();
                                        self.texture_cache.clear();
                                        self.textures = self.load_pair(self.current_index, ctx);
                                        self.save_settings(); // Save when algorithm changes
                                    }
                                }

                                ui.add_space(20.0);
                                ui.label(egui::RichText::new(tr("settings.page_view")).size(20.0).strong());
                                separator_pct(ui);

                                {
                                    let mut changed = false;
                                    changed |= ui.radio_value(&mut self.config.page_view_options, PageViewOptions::Single, egui::RichText::new(tr("settings.page_view.single"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.page_view_options, PageViewOptions::DoubleRL, egui::RichText::new(tr("settings.page_view.double_rl"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.page_view_options, PageViewOptions::DoubleLR, egui::RichText::new(tr("settings.page_view.double_lr"))).clicked();
                                    separator_pct(ui);
                                    ui.label(egui::RichText::new(tr("settings.page_view.center_offset")).size(20.0).strong());
                                    let slider_width = ui.available_width() * 0.98;
                                    let previous_slider_width = ui.spacing().slider_width;
                                    ui.spacing_mut().slider_width = slider_width - 80.0;
                                    changed |= ui
                                        .add(egui::Slider::new(
                                            &mut self.config.spread_center_offset,
                                            -150.0..=150.0,
                                        ).step_by(1.0))
                                        .on_hover_text(tr("settings.page_view.center_offset.tooltip"))
                                        .changed();
                                    ui.spacing_mut().slider_width = previous_slider_width - 80.0;

                                    if changed {
                                        self.reset_buffer();
                                        self.reset_pan();
                                        self.textures = self.load_pair(self.current_index, ctx);
                                        self.save_settings();
                                    }
                                }

                                ui.add_space(20.0);
                                ui.label(egui::RichText::new(tr("settings.last_page")).size(20.0).strong());
                                separator_pct(ui);

                                {
                                    let mut changed = false;
                                    changed |= ui.radio_value(&mut self.config.last_page_action, LastPageAction::GotoNextFile, egui::RichText::new(tr("settings.last_page.next_file"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.last_page_action, LastPageAction::ToFirstPage, egui::RichText::new(tr("settings.last_page.first_page"))).clicked();
                                    changed |= ui.radio_value(&mut self.config.last_page_action, LastPageAction::Nothing, egui::RichText::new(tr("settings.last_page.nothing"))).clicked();

                                    if changed {
                                        self.reset_buffer();
                                        self.textures = self.load_pair(self.current_index, ctx);
                                        self.save_settings();
                                    }
                                }

                                ui.add_space(20.0);
                                ui.label(egui::RichText::new(tr("settings.zoom")).size(20.0).strong());
                                separator_pct(ui);

                                let previous_slider_width = ui.spacing().slider_width;
                                ui.spacing_mut().slider_width = self.config.settings_width * 0.9 - 120.0;
                                let zoom_slider = ui.add(egui::Slider::new(&mut self.zoom_factor, 0.5..=3.0).text(tr("settings.zoom.slider")));
                                ui.spacing_mut().slider_width = previous_slider_width;
                                let is_scrubbing_zoom = zoom_slider.dragged();
                                if zoom_slider.changed() && !is_scrubbing_zoom {
                                    if self.zoom_factor != 1.0 {
                                        self.reset_buffer();
                                        self.texture_cache.clear();
                                        self.reset_pan();
                                        self.textures = self.load_pair(self.current_index, ctx);
                                    }
                                }

                                if ui.button(egui::RichText::new(tr("settings.zoom.reset"))).clicked() {
                                    self.zoom_factor = 1.0;
                                    self.reset_pan();
                                }

                                ui.add_space(10.0);
                                ui.label(egui::RichText::new(tr("settings.image_sizing")).size(18.0).strong());
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
                                ui.label(egui::RichText::new(tr("settings.others")).size(20.0).strong());
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
                                ui.checkbox(&mut self.config.show_top_bar, tr("settings.show_toolbar"));
                                ui.checkbox(&mut self.config.transparency_support, tr("settings.transparency"))
                                    .on_hover_text(tr("settings.transparency.tooltip"));
                                ui.checkbox(&mut self.config.enable_auto_image_byte_fix, tr("settings.auto_image_fix"))
                                    .on_hover_text(tr("settings.auto_image_fix.tooltip"));
                                ui.checkbox(&mut self.config.enable_single_file_caching, tr("settings.single_file_cache"))
                                    .on_hover_text(tr("settings.single_file_cache.tooltip"));
                                ui.add(egui::Slider::new(&mut self.config.image_delay, 0..=1000)
                                    .text(tr("settings.image_delay"))).on_hover_text(tr("settings.image_delay.tooltip"));
                                ui.add_space(20.0);

                                egui::CollapsingHeader::new(egui::RichText::new(tr("settings.key_config")).size(20.0).strong())
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        separator_pct(ui);
                                        let listening_text = tr("common.listening").to_owned();
                                        egui::Grid::new("key_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                                            ui.label(tr("settings.key.next_page"));
                                            render_binding_button(ui, "Next Page", &mut self.config.keys.next_page, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.prev_page"));
                                            render_binding_button(ui, "Previous Page", &mut self.config.keys.prev_page, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.one_next_page"));
                                            render_binding_button(ui, "1 Next Page", &mut self.config.keys.one_next_page, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.one_prev_page"));
                                            render_binding_button(ui, "1 Prev Page", &mut self.config.keys.one_prev_page, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.first_page"));
                                            render_binding_button(ui, "First Page", &mut self.config.keys.first_page, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.last_page"));
                                            render_binding_button(ui, "Last Page", &mut self.config.keys.last_page, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.next_file"));
                                            render_binding_button(ui, "Next File", &mut self.config.keys.next_file, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.prev_file"));
                                            render_binding_button(ui, "Previous File", &mut self.config.keys.prev_file, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.next_folder"));
                                            render_binding_button(ui, "Next Folder", &mut self.config.keys.next_folder, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.prev_folder"));
                                            render_binding_button(ui, "Previous Folder", &mut self.config.keys.prev_folder, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.fullscreen"));
                                            render_binding_button(ui, "Toggle Fullscreen", &mut self.config.keys.fullscreen, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.view_mode"));
                                            render_binding_button(ui, "View Mode", &mut self.config.keys.view_mode, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.open_file"));
                                            render_binding_button(ui, "Open File", &mut self.config.keys.open_file, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                            ui.label(tr("settings.key.quit_app"));
                                            render_binding_button(ui, "Quit App", &mut self.config.keys.quit_app, &mut self.binding_action, &listening_text);
                                            ui.end_row();
                                        });
                                        separator_pct(ui);
                                    });

                                ui.add_space(20.0);
                                egui::CollapsingHeader::new(egui::RichText::new(tr("settings.mouse_mapping")).size(20.0).strong())
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        separator_pct(ui);
                                        ui.label(tr("settings.mouse_mapping.description"));
                                        ui.add_space(10.0);
                                        let mut mouse_changed = false;
                                        mouse_changed |= ui
                                            .add(
                                                egui::Slider::new(
                                                    &mut self.config.double_click_threshold_ms,
                                                    100..=1000,
                                                )
                                                .text(tr("settings.double_click_threshold")),
                                            )
                                            .changed();
                                        ui.add_space(10.0);
                                        let action_options: Vec<(MangaAction, String)> = MangaAction::ALL
                                            .into_iter()
                                            .map(|action| (action, self.action_label(action).to_owned()))
                                            .collect();
                                        let unassigned_label = tr("common.unassigned").to_owned();
                                        egui::Grid::new("mouse_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                                            ui.label(tr("settings.mouse.scroll_up"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_scroll_up", &mut self.config.mouse.scroll_up, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.scroll_down"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_scroll_down", &mut self.config.mouse.scroll_down, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button1_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button1_click", &mut self.config.mouse.button1_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button2_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button2_click", &mut self.config.mouse.button2_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button3_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button3_click", &mut self.config.mouse.button3_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button4_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button4_click", &mut self.config.mouse.button4_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button5_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button5_click", &mut self.config.mouse.button5_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button1_double_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button1_double_click", &mut self.config.mouse.button1_double_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button2_double_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button2_double_click", &mut self.config.mouse.button2_double_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button3_double_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button3_double_click", &mut self.config.mouse.button3_double_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button4_double_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button4_double_click", &mut self.config.mouse.button4_double_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button5_double_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button5_double_click", &mut self.config.mouse.button5_double_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button1_long_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button1_long_click", &mut self.config.mouse.button1_long_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button2_long_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button2_long_click", &mut self.config.mouse.button2_long_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button3_long_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button3_long_click", &mut self.config.mouse.button3_long_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button4_long_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button4_long_click", &mut self.config.mouse.button4_long_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                            ui.label(tr("settings.mouse.button5_long_click"));
                                            mouse_changed |= render_mouse_action_dropdown(ui, "mouse_button5_long_click", &mut self.config.mouse.button5_long_click, &action_options, &unassigned_label);
                                            ui.end_row();
                                        });
                                        if mouse_changed {
                                            self.save_settings();
                                        }
                                        separator_pct(ui);
                                    });

                                ui.add_space(20.0);
                                egui::CollapsingHeader::new(egui::RichText::new(tr("settings.gamepad_mapping")).size(20.0).strong())
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        separator_pct(ui);
                                        ui.label(tr("settings.gamepad_mapping.description"));
                                        ui.add_space(10.0);
                                        let mut gamepad_changed = false;
                                        let action_options: Vec<(MangaAction, String)> = MangaAction::ALL
                                            .into_iter()
                                            .map(|action| (action, self.action_label(action).to_owned()))
                                            .collect();
                                        let unassigned_label = tr("common.unassigned").to_owned();
                                        egui::Grid::new("gamepad_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                                            for button in GamepadButton::ALL {
                                                ui.label(self.gamepad_button_label(button));
                                                let action = match button {
                                                    GamepadButton::South => &mut self.config.gamepad.south,
                                                    GamepadButton::East => &mut self.config.gamepad.east,
                                                    GamepadButton::North => &mut self.config.gamepad.north,
                                                    GamepadButton::West => &mut self.config.gamepad.west,
                                                    GamepadButton::LeftTrigger => &mut self.config.gamepad.left_trigger,
                                                    GamepadButton::LeftTrigger2 => &mut self.config.gamepad.left_trigger2,
                                                    GamepadButton::RightTrigger => &mut self.config.gamepad.right_trigger,
                                                    GamepadButton::RightTrigger2 => &mut self.config.gamepad.right_trigger2,
                                                    GamepadButton::Select => &mut self.config.gamepad.select,
                                                    GamepadButton::Start => &mut self.config.gamepad.start,
                                                    GamepadButton::LeftThumb => &mut self.config.gamepad.left_thumb,
                                                    GamepadButton::RightThumb => &mut self.config.gamepad.right_thumb,
                                                    GamepadButton::DPadUp => &mut self.config.gamepad.dpad_up,
                                                    GamepadButton::DPadDown => &mut self.config.gamepad.dpad_down,
                                                    GamepadButton::DPadLeft => &mut self.config.gamepad.dpad_left,
                                                    GamepadButton::DPadRight => &mut self.config.gamepad.dpad_right,
                                                };
                                                gamepad_changed |= render_mouse_action_dropdown(
                                                    ui,
                                                    &format!("gamepad_{button:?}"),
                                                    action,
                                                    &action_options,
                                                    &unassigned_label,
                                                );
                                                ui.end_row();
                                            }
                                        });
                                        if gamepad_changed {
                                            self.save_settings();
                                        }
                                        separator_pct(ui);
                                    });

                                ui.add_space(50.0);
                        });


                    });
                    fn separator_pct(ui: &mut egui::Ui) {
                        let pct = 0.9;

                        let spacing = ui.spacing();
                        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;

                        // Allocate a thin horizontal area (like separator does)
                        let desired_h = spacing.scroll.bar_width; // this is usually ~1.0
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), desired_h), egui::Sense::hover());

                        // Compute a centered line that's pct of the available width
                        let full_w = rect.width();
                        let line_w = full_w * pct;

                        let indent = 0.0;
                        let x0 = rect.left() + indent;
                        let x1 = (x0 + line_w).min(rect.right());
                        let y  = rect.center().y;

                        ui.painter().line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], stroke);
                    }

                    // Helper function to keep the UI code clean
                    fn render_binding_button(ui: &mut egui::Ui, id: &str, shortcut: &mut Shortcut, binding: &mut Option<BindingTarget>, listening_text: &str) {
                        let is_binding = matches!(binding, Some(BindingTarget::Keyboard(action)) if action == id);
                        let text = if is_binding { listening_text.to_string() } else { shortcut.format() };

                        if ui.button(text).clicked() {
                            *binding = Some(BindingTarget::Keyboard(id.to_string()));
                        }
                    }

                    fn render_mouse_action_dropdown(ui: &mut egui::Ui, id: &str, action: &mut MangaAction, action_options: &[(MangaAction, String)], unassigned_label: &str) -> bool {
                        let previous = *action;
                        egui::ComboBox::from_id_salt(id)
                            .selected_text(action_options.iter().find(|(option, _)| option == action).map(|(_, label)| label.as_str()).unwrap_or(unassigned_label))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for (option, label) in action_options {
                                    ui.selectable_value(action, *option, label);
                                }
                            });

                        if *action != previous {
                            ui.ctx().request_repaint();
                            true
                        } else {
                            false
                        }
                    }
                });
        }

        // This allows opening/closing the settings
        let screen_rect = ctx.content_rect();
        let button_height = 160.0;

        // Calculate X position based on whether panel is open
        let x_pos = if self.config.show_settings {
            screen_rect.max.x - self.config.settings_width - 45.0
        } else {
            screen_rect.max.x - 25.0
        };

        // Calculate Y position to center the 200px button vertically
        let y_pos = screen_rect.center().y - (button_height / 2.0);

        egui::Area::new(egui::Id::new("settings_toggle"))
            .fixed_pos([x_pos, y_pos])
            .show(ctx, |ui| {
                let text = if self.config.show_settings {
                    "▶"
                } else {
                    "◀"
                };

                // We use add_sized to force the 200px height
                let toggle_btn = egui::Button::new(egui::RichText::new(text).size(20.0));
                if ui.add_sized([25.0, button_height], toggle_btn).clicked() {
                    self.config.show_settings = !self.config.show_settings;
                }
            });

        if self.config.show_top_bar && !self.is_fullscreen {
            egui::TopBottomPanel::top("top_toolbar")
                .frame(
                    egui::Frame::NONE
                        .fill(egui::Color32::from_gray(30))
                        .inner_margin(4.0),
                )
                .show(ctx, |ui| {
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
                            .button("📁⏭")
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
                            .button("📦⏭")
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
                            tr("state.odd_page")
                        } else {
                            tr("state.even_page")
                        };
                        if ui.button(shift_label).clicked() {
                            self.change_shifted_mode(ctx);
                        }

                        if ui
                            .button("📺")
                            .on_hover_text(tr("toolbar.fullscreen"))
                            .clicked()
                        {
                            self.is_fullscreen = !self.is_fullscreen;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                self.is_fullscreen,
                            ));
                        }
                        ui.separator();
                        if ui.button(tr("toolbar.open_file")).clicked() {
                            self.open_file_dialog();
                        }

                        ui.separator();
                        // --- THE SLIDER ---
                        // We use a 1-based slider for better user experience
                        let mut page_val = self.current_index + 1;
                        let max_pages = self.image_files.len().max(1);

                        // ui.available_width() ensures the slider stretches to fill the gap
                        let slider_width = ui.available_width() / 3.0; // Reserve space for right-side buttons

                        let style = ui.style_mut();
                        style.spacing.slider_width = slider_width;

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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("❌").on_hover_text(tr("toolbar.hide")).clicked() {
                                self.config.show_top_bar = false;
                            }
                        });
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.image_panel_background_color()))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                // Create a 'Response' for the entire background area first,
                // but we check it at the END of the code.
                let bg_response = ui.interact(rect, ui.id().with("bg"), egui::Sense::hover());

                if self.zip_path.is_some() {
                    // Show single image on center or if in shifted cover mode
                    let is_zoomed = (self.zoom_factor - 1.0).abs() > 0.01;
                    let double_needs_drag = self.double_page_needs_drag(rect);
                    let viewing_single = self.is_single_page()
                        || (self.is_shifted && self.current_index == 0);
                    let single_image_size = self.textures[0]
                        .as_ref()
                        .map(|tex| self.image_draw_size(tex, rect.size(), self.zoom_factor))
                        .unwrap_or(egui::Vec2::ZERO);
                    let single_needs_drag = single_image_size.x > rect.width()
                        || single_image_size.y > rect.height();

                    if viewing_single && !single_needs_drag {
                        if let Some(tex) = &self.textures[0] {
                            ui.painter().image(
                                tex.id(),
                                egui::Rect::from_center_size(rect.center(), single_image_size),
                                egui::Rect::from_min_max(
                                    egui::Pos2::ZERO,
                                    egui::pos2(1.0, 1.0),
                                ),
                                egui::Color32::WHITE,
                            );
                        }
                    } else if viewing_single {
                        let pan_response =
                            ui.interact(rect, ui.id().with("single_pan"), egui::Sense::click_and_drag());
                        if pan_response.dragged() {
                            self.pan_offset += ctx.input(|i| i.pointer.delta());
                            ctx.request_repaint();
                        }
                        if self.binding_action.is_none() && action_to_run == MangaAction::None {
                            if let Some(mouse_action) = self.collect_mouse_action(&pan_response, ctx)
                            {
                                action_to_run = mouse_action;
                            }
                        }
                        if let Some(tex) = &self.textures[0] {
                            ui.painter().image(
                                tex.id(),
                                egui::Rect::from_center_size(
                                    rect.center() + self.pan_offset,
                                    single_image_size,
                                ),
                                egui::Rect::from_min_max(
                                    egui::Pos2::ZERO,
                                    egui::pos2(1.0, 1.0),
                                ),
                                egui::Color32::WHITE,
                            );
                        }
                    } else if is_zoomed || double_needs_drag {
                        let pan_response =
                            ui.interact(rect, ui.id().with("spread_pan"), egui::Sense::click_and_drag());
                        if pan_response.dragged() {
                            self.pan_offset += ctx.input(|i| i.pointer.delta());
                            ctx.request_repaint();
                        }
                        if self.binding_action.is_none() && action_to_run == MangaAction::None {
                            if let Some(mouse_action) = self.collect_mouse_action(&pan_response, ctx)
                            {
                                action_to_run = mouse_action;
                            }
                        }
                        let page_container_size = egui::vec2(
                            (rect.width() * 0.5 - self.config.spread_center_offset.abs())
                                .max(1.0),
                            rect.height(),
                        );
                        let left_size = self.textures[1]
                            .as_ref()
                            .map(|tex| {
                                self.image_draw_size(tex, page_container_size, self.zoom_factor)
                            })
                            .unwrap_or(egui::Vec2::ZERO);
                        let right_size = self.textures[0]
                            .as_ref()
                            .map(|tex| {
                                self.image_draw_size(tex, page_container_size, self.zoom_factor)
                            })
                            .unwrap_or(egui::Vec2::ZERO);

                        let center_axis = rect.center().x + self.pan_offset.x;
                        let offset = self.config.spread_center_offset;
                        let top_y =
                            rect.center().y - left_size.y.max(right_size.y) * 0.5 + self.pan_offset.y;
                        let (visual_left_size, visual_right_size) =
                            if self.config.page_view_options == PageViewOptions::DoubleLR {
                                (right_size, left_size)
                            } else {
                                (left_size, right_size)
                            };
                        let visual_left_x = center_axis - offset - visual_left_size.x;
                        let visual_right_x = center_axis + offset;
                        let clip_axis = center_axis.clamp(rect.min.x, rect.max.x);
                        let left_visible_rect = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(clip_axis, rect.max.y),
                        );
                        let right_visible_rect = egui::Rect::from_min_max(
                            egui::pos2(clip_axis, rect.min.y),
                            rect.max,
                        );

                        if self.config.page_view_options == PageViewOptions::DoubleLR {
                            if let Some(tex) = &self.textures[0] {
                                self.paint_image_clipped(
                                    ui,
                                    tex,
                                    egui::Rect::from_min_size(
                                        egui::pos2(visual_left_x, top_y),
                                        right_size,
                                    ),
                                    left_visible_rect,
                                );
                            }
                            if let Some(tex) = &self.textures[1] {
                                self.paint_image_clipped(
                                    ui,
                                    tex,
                                    egui::Rect::from_min_size(
                                        egui::pos2(visual_right_x, top_y),
                                        left_size,
                                    ),
                                    right_visible_rect,
                                );
                            }
                        } else {
                            if let Some(tex) = &self.textures[1] {
                                self.paint_image_clipped(
                                    ui,
                                    tex,
                                    egui::Rect::from_min_size(
                                        egui::pos2(visual_left_x, top_y),
                                        left_size,
                                    ),
                                    left_visible_rect,
                                );
                            }
                            if let Some(tex) = &self.textures[0] {
                                self.paint_image_clipped(
                                    ui,
                                    tex,
                                    egui::Rect::from_min_size(
                                        egui::pos2(visual_right_x, top_y),
                                        right_size,
                                    ),
                                    right_visible_rect,
                                );
                            }
                        }

                    } else {
                        let center = rect.center().x;
                        let mut left_half = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(center - self.config.spread_center_offset, rect.max.y),
                        );
                        let mut right_half = egui::Rect::from_min_max(
                            egui::pos2(center + self.config.spread_center_offset, rect.min.y),
                            rect.max,
                        );
                        let mut align_for_left_side: Align = egui::Align::RIGHT;
                        let mut align_for_right_side: Align = egui::Align::LEFT;
                        if self.config.page_view_options == PageViewOptions::DoubleLR {
                            std::mem::swap(&mut left_half, &mut right_half);
                            align_for_left_side = egui::Align::LEFT;
                            align_for_right_side = egui::Align::RIGHT;
                        }

                        let left_visible_half =
                            egui::Rect::from_min_max(rect.min, egui::pos2(center, rect.max.y));
                        let right_visible_half =
                            egui::Rect::from_min_max(egui::pos2(center, rect.min.y), rect.max);
                        let left_visible_rect = if left_half.center().x <= center {
                            left_visible_half
                        } else {
                            right_visible_half
                        };
                        let right_visible_rect = if right_half.center().x <= center {
                            left_visible_half
                        } else {
                            right_visible_half
                        };
                        let left_image_size = self.textures[1]
                            .as_ref()
                            .map(|tex| self.image_draw_size(tex, left_half.size(), 1.0))
                            .unwrap_or(egui::Vec2::ZERO);
                        let right_image_size = self.textures[0]
                            .as_ref()
                            .map(|tex| self.image_draw_size(tex, right_half.size(), 1.0))
                            .unwrap_or(egui::Vec2::ZERO);

                        let left_response = self.create_image_rect(
                            ui,
                            left_half,
                            left_visible_rect,
                            "left_hit",
                            1,
                            align_for_left_side,
                            left_image_size,
                        );
                        let right_response = self.create_image_rect(
                            ui,
                            right_half,
                            right_visible_rect,
                            "right_hit",
                            0,
                            align_for_right_side,
                            right_image_size,
                        );
                        if self.binding_action.is_none() && action_to_run == MangaAction::None {
                            if let Some(mouse_action) =
                                self.collect_mouse_action(&left_response, ctx)
                            {
                                action_to_run = mouse_action;
                            } else if let Some(mouse_action) =
                                self.collect_mouse_action(&right_response, ctx)
                            {
                                action_to_run = mouse_action;
                            }
                        }

                        // ONLY TRIGGER IF BACKGROUND WAS CLICKED
                        // bg_response.clicked() is true if the background was clicked.
                        // However, we only want to trigger if a specific image wasn't the target.
                        if bg_response.clicked()
                            && !ctx.is_using_pointer()
                            && !ctx.input(|i| i.pointer.any_down())
                        {
                            // Extra safety: check if we are actually hovering an image
                            if !left_visible_rect
                                .contains(ctx.pointer_interact_pos().unwrap_or_default())
                                && !right_visible_rect
                                    .contains(ctx.pointer_interact_pos().unwrap_or_default())
                            {
                                self.open_file_dialog();
                            }
                        }
                    }
                } else {
                    // the start screen
                    ui.centered_and_justified(|ui| {
                        let start_btn = egui::Button::new(
                            egui::RichText::new(tr("start.open_zip"))
                                .size(20.0)
                                .color(egui::Color32::from_gray(200)),
                        )
                        .fill(egui::Color32::from_gray(40));
                        if ui.add_sized(ctx.content_rect().size(), start_btn).clicked() {
                            self.open_file_dialog();
                        }
                    });
                }

                // --- THE BACKGROUND CLICK CHECK (When Zip is Open) ---
                if self.zip_path.is_some() && bg_response.clicked() {
                    // Check if the click was actually handled by an image
                    if !ctx.is_using_pointer() {
                        // Check coordinates to ensure we aren't inside the "reading zones"
                        let left_half = egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.center().x, rect.max.y),
                        );
                        let right_half = egui::Rect::from_min_max(
                            egui::pos2(rect.center().x, rect.min.y),
                            rect.max,
                        );

                        let pointer_pos =
                            ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default();

                        if !left_half.contains(pointer_pos) && !right_half.contains(pointer_pos) {
                            self.open_file_dialog();
                        }
                    }
                }

                // Error Overlay Logic (Fading)
                if let Some((msg, start_time)) = &self.error_msg {
                    let elapsed = start_time.elapsed().as_secs_f32();
                    if elapsed < 2.0 {
                        let opacity = (1.0 - (elapsed / 2.0)).clamp(0.0, 1.0);
                        let padding = if self.config.show_settings {
                            -(self.config.settings_width / 2.0)
                        } else {
                            0.0
                        };
                        egui::Window::new("")
                            .anchor(egui::Align2::CENTER_TOP, [padding, 20.0]) // Positioned at top center
                            .frame(
                                egui::Frame::window(&ui.style())
                                    .fill(egui::Color32::from_black_alpha((180.0 * opacity) as u8))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_white_alpha((50.0 * opacity) as u8),
                                    )),
                            )
                            .title_bar(false)
                            .show(ctx, |ui| {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .color(egui::Color32::from_white_alpha(
                                            (255.0 * opacity) as u8,
                                        ))
                                        .size(24.0)
                                        .strong(),
                                );
                            });
                        ctx.request_repaint();
                    } else {
                        self.error_msg = None;
                    }
                }

                // --- ZIP FILENAME OVERLAY (Center-Top) ---
                if let Some((name, start_time)) = &self.zip_name_display {
                    let elapsed = start_time.elapsed().as_secs_f32();

                    if elapsed < 2.0 {
                        let opacity = (1.0 - (elapsed / 2.0)).clamp(0.0, 1.0);
                        let padding = if self.config.show_settings {
                            -(self.config.settings_width / 2.0)
                        } else {
                            0.0
                        };
                        egui::Window::new("zip_name_overlay")
                            .anchor(egui::Align2::CENTER_TOP, [padding, 80.0]) // Positioned at top center
                            .frame(
                                egui::Frame::window(&ui.style())
                                    .fill(egui::Color32::from_black_alpha((180.0 * opacity) as u8))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_white_alpha((50.0 * opacity) as u8),
                                    )),
                            )
                            .title_bar(false)
                            .resizable(false)
                            .show(ctx, |ui| {
                                ui.label(
                                    egui::RichText::new(name)
                                        .color(egui::Color32::from_white_alpha(
                                            (255.0 * opacity) as u8,
                                        ))
                                        .size(24.0)
                                        .strong(),
                                );
                            });
                        ctx.request_repaint(); // Keep the animation smooth
                    } else {
                        self.zip_name_display = None;
                    }
                }

                // --- THE PAGE INDICATOR OVERLAY (Large & Single Line) ---
                if let Some(start_time) = self.page_indicator_time {
                    let elapsed = start_time.elapsed().as_secs_f32();
                    if elapsed < 2.0 {
                        let opacity = (1.0 - (elapsed / 2.0)).clamp(0.0, 1.0);
                        let padding = if self.config.show_settings {
                            -30.0 - self.config.settings_width
                        } else {
                            -15.0
                        };
                        egui::Window::new("page_info")
                            .anchor(egui::Align2::RIGHT_TOP, [padding, 10.0])
                            .frame(
                                egui::Frame::NONE
                                    .fill(egui::Color32::from_rgba_unmultiplied(
                                        60,
                                        60,
                                        60,
                                        (opacity * 255.0) as u8,
                                    ))
                                    .inner_margin(5.0)
                                    .corner_radius(5.0),
                            ) // No background box
                            .title_bar(false)
                            .resizable(false)
                            .collapsible(false)
                            .fixed_size([300.0, 60.0]) // Force a wide area to prevent wrapping
                            .show(ctx, |ui| {
                                // Ensure text stays on one line
                                ui.horizontal(|ui| {
                                    let page_text = format!(
                                        "{} / {}",
                                        self.current_index + 1,
                                        self.image_files.len()
                                    );
                                    ui.label(
                                        egui::RichText::new(page_text)
                                            .color(egui::Color32::from_white_alpha(
                                                (200.0 * opacity) as u8,
                                            ))
                                            .size(22.0) // Much larger font
                                            .strong(),
                                    );
                                });
                            });
                        ctx.request_repaint();
                    } else {
                        self.page_indicator_time = None;
                    }
                }
            });

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

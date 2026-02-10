use std::env;
use eframe::egui;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use egui::{Align, Rect};
use image::{DynamicImage, ImageFormat};
use pdfium_render::prelude::Pixels;
use crate::config::{AppSettings, MangaAction, PageViewOptions, ResizeMethod, Shortcut, SourceMode};
use crate::font;
use crate::utils::{windows_natural_sort, windows_natural_sort_strings};

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
    binding_action: Option<String>,
    texture_cache: std::collections::HashMap<String, egui::TextureHandle>,
    initial_path: Option<PathBuf>,
    source_mode: SourceMode,
    last_image_switch_time: Instant,
}

impl MangaReader {
    pub fn new(_cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        font::setup_custom_fonts(&_cc.egui_ctx);
        let config: AppSettings = if let Ok(data) = std::fs::read_to_string("settings.json") {
            // add |_| here to accept the error argument but ignore it
            serde_json::from_str(&data).unwrap_or_else(|_| {
                eprintln!("Failed to parse settings.json, using defaults.");
                AppSettings::default()
            })
        } else {
            AppSettings::default()
        };

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

    fn open_file_dialog(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_dialog_time) > std::time::Duration::from_millis(500) {
            self.last_dialog_time = now;
            if !self.is_dialog_open {
                self.is_dialog_open = true;
                let sender = self.dialog_tx.clone();

                std::thread::spawn(move || {
                    let file = rfd::FileDialog::new()
                        .add_filter("Manga Files", &["zip", "cbz", "cbr", "rar", "png", "jpg", "jpeg", "bmp", "webp", "gif", "tiff", "tga", "avif", "pdf"])
                        .pick_file();

                    let _ = sender.send(file);
                });
            }
        }
    }

    fn scan_folder(&mut self, current_parent: &Path) -> Vec<PathBuf> {
        let mut items = Vec::new();
        if let Ok(entries) = fs::read_dir(current_parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_zip = path.extension().map_or(false, |ext| ext == "zip" || ext == "pdf" || ext == "cbz" || ext == "cbr" || ext == "rar");
                // Treat non-hidden directories as readable manga sources
                let is_dir = path.is_dir() && !path.file_name().unwrap_or_default().to_string_lossy().starts_with('.');

                if is_zip || is_dir {
                    items.push(path);
                }
            }
        }
        windows_natural_sort(&mut items);
        items
    }


    fn get_adjacent_directories(path_with_file_name: Option<PathBuf>) -> (Option<PathBuf>, Option<PathBuf>) {
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
                let prev = if pos > 0 { Some(dirs[pos - 1].clone()) } else { None };
                let next = if pos + 1 < dirs.len() { Some(dirs[pos + 1].clone()) } else { None };
                (prev, next)
            }
            None => (None, None),
        }
    }

    fn update_buffers(&mut self, ctx: &egui::Context) {
        let idx = self.current_index;

        if self.zip_path.is_none() {
            return
        }

        // Only update if we moved OR if the buffers were recently consumed
        if self.last_buffered_index == Some(idx) {
            return;
        }

        // Preload Next (2 pages ahead)
        if self.buffer_next[0].is_none() {
            let mut next_index_to_load = idx + 2;
            if idx == 0 && self.is_shifted {
                next_index_to_load = 1;
            }
            self.buffer_next = self.load_pair(next_index_to_load, ctx);
        }

        // Preload Prev (2 pages behind)
        if idx >= 2 && self.buffer_prev[0].is_none() {
            self.buffer_prev = self.load_pair(idx - 2, ctx);
        }

        self.last_buffered_index = Some(idx);
    }

    fn load_pair(&mut self, start_idx: usize, ctx: &egui::Context) -> [Option<egui::TextureHandle>; 2] {
        let mut pair: [Option<egui::TextureHandle>; 2] = [None, None];
        let source_path = self.zip_path.clone().unwrap();

        let mut archive = if self.source_mode == SourceMode::Zip {
            let path = source_path.clone();
            File::open(path).ok().and_then(|f| zip::ZipArchive::new(f).ok())
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
                    } else  if self.source_mode == SourceMode::Rar {
                        unrar::Archive::new(&source_path).open_for_processing().ok().and_then(|rar_achive| {
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
                                                cursor = e.skip().ok().and_then(|arc| arc.read_header().ok().flatten());
                                            }
                                        } else {
                                            // Filename wasn't valid UTF-8, skip it
                                            cursor = e.skip().ok().and_then(|arc| arc.read_header().ok().flatten());
                                        }
                                    }
                                    None => break None,
                                }
                            }
                        })
                    } else {
                        None
                    };

                    if let Some(buffer) = bytes {
                        match image::guess_format(&buffer) {
                            Ok(format) => {
                                if let Ok(img) = image::load_from_memory_with_format(&buffer, format) {
                                    pair[i] = self.load_texture(img, filename.clone(), ctx);
                                }
                            }
                            Err(_) => {
                                // Fallback: If guessing fails, try loading as TGA
                                // since TGA is often the one that fails detection.
                                if let Ok(img) = image::load_from_memory_with_format(&buffer, ImageFormat::Tga) {
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

    /// Helper to render a specific page
    fn render_pdf_page(&self, index: usize, ctx: &egui::Context) -> Option<DynamicImage> {
        // You would typically store a reference to the loaded document
        // and render the page here.
        let pdfium = pdfium_render::prelude::Pdfium::default();
        let doc = pdfium.load_pdf_from_file(self.zip_path.as_ref()?, None).ok()?;
        let page = doc.pages().get(index as u16).ok()?;
        let width_inch = page.width().value ;
        let height_inch = page.height().value;

        let screen_size = ctx.content_rect();
        let target_h = screen_size.height();
        let h_ratio = target_h / height_inch;
        let target_w = width_inch * h_ratio;

        // Render at 300 DPI or based on screen height for clarity
        let bitmap = page.render(target_w as Pixels, target_h as Pixels, None).ok()?;
        Some(bitmap.as_image()) // pdfium-render integrates with the 'image' crate
    }

    fn load_texture(&mut self, img: DynamicImage, cache_name:String, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let resize_start = Instant::now();
        let filter = self.config.resize_method.to_filter();
        let processed_img = if let Some(filter_type) = filter {
            let screen_size = ctx.content_rect();
            let target_h = screen_size.height() as u32;
            let aspect_ratio = img.width() as f32 / img.height() as f32;
            let target_w = (target_h as f32 * aspect_ratio) as u32;
            img.resize(target_w, target_h, filter_type)
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
            egui::ColorImage::from_rgb(
                size,
                processed_img.to_rgb8().as_raw()
            )
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
            egui::TextureOptions::LINEAR // Smooth scaling
        );
        if self.config.enable_single_file_caching {
            self.texture_cache.insert(cache_name.clone(), handle.clone());
        }
        Some(handle)
    }

    fn load_source(&mut self, path: PathBuf, ctx: &egui::Context) {
        let mut target_path = path.clone();
        let mut start_at_filename: Option<String> = None;

        let mut images = Vec::new();
        let exts = ["png", "jpg", "jpeg", "bmp", "webp", "gif", "tiff", "tga", "avif"];
        let extension = path.extension().map_or("".to_string(), |ext| ext.to_string_lossy().to_lowercase());
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
                                if exts.iter().any(|&e_ext| name.to_lowercase().ends_with(e_ext)) {
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
                            if p.is_file() && exts.iter().any(|&e| p.extension().map_or(false, |ext| ext.to_string_lossy().to_lowercase() == e)) {
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
            self.show_fading_error("No images found in selection.");
        } else {
            self.reset_buffer();
            self.texture_cache.clear();

            // If we opened a specific image, find its index in the sorted list
            self.current_index = if let Some(target_name) = start_at_filename {
                images.iter().position(|r| r == &target_name).unwrap_or(0)
            } else {
                0
            };

            self.zip_path = Some(target_path.clone());
            self.image_files = images;

            // Scan parent for Next/Prev file navigation
            self.all_zips_in_folder = self.scan_folder(&target_path.parent().unwrap_or(Path::new("")));

            if let Some(file_name) = target_path.file_name() {
                self.zip_name_display = Some((file_name.to_string_lossy().to_string(), Instant::now()));
            }

            self.textures = self.load_pair(self.current_index, ctx);
        }
    }

    fn show_fading_error(&mut self, msg: &str) {
        self.error_msg = Some((msg.to_string(), Instant::now()));
    }

    fn next_page(&mut self, ctx: &egui::Context) {
        if self.last_image_switch_time + Duration::from_millis(self.config.image_delay) > Instant::now() {
            return;
        } else {
            self.last_image_switch_time = Instant::now();
        }
        let step = if self.is_single_page() || (self.is_shifted && self.current_index == 0) { 1 } else { 2 };

        if self.current_index + step < self.image_files.len() {
            self.current_index += step;
            // If the next pages are already in the buffer, swap them in
            if self.buffer_next[0].is_some() {
                // Take the textures from the buffer and put them in the active slot
                // also slide the current texture to prev buffer
                self.buffer_prev = std::mem::take(&mut self.textures);
                self.textures = std::mem::take(&mut self.buffer_next);
                self.buffer_next = [None, None];
            } else {
                // Fallback if buffer wasn't ready (e.g., very fast clicking)
                self.textures = self.load_pair(self.current_index, ctx);
            }
        } else {
            // End of Zip list reached, try to find next zip ---
            self.next_zip(ctx);
        }
        self.page_indicator_time = Some(Instant::now());
    }

    fn prev_page(&mut self, ctx: &egui::Context) {
        if self.last_image_switch_time + Duration::from_millis(self.config.image_delay) > Instant::now() {
            return;
        } else {
            self.last_image_switch_time = Instant::now();
        }
        let step = if self.is_single_page() || (self.is_shifted && self.current_index == 1) { 1 } else { 2 };

        if self.current_index >= step {
            self.current_index -= step;
            // Use the previous buffer textures
            if self.buffer_prev[0].is_some() {
                self.buffer_next = std::mem::take(&mut self.textures);
                self.textures = std::mem::take(&mut self.buffer_prev);
                self.buffer_prev = [None, None];
            } else {
                self.textures = self.load_pair(self.current_index, ctx);
            }
        } else {
            //e are at the start of the Zip, move to PREVIOUS ZIP ---
            self.prev_zip(ctx);
        }
        self.page_indicator_time = Some(Instant::now());
    }

    fn next_zip(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self.all_zips_in_folder.iter().position(|p| Some(p) == self.zip_path.as_ref()) {
            if pos + 1 < self.all_zips_in_folder.len() {
                // There is a next file
                let next_path = self.all_zips_in_folder[pos + 1].clone();
                self.load_source(next_path, ctx);
            } else {
                // NO MORE FILES - This is the fix
                self.show_fading_error("No more zip files in folder.");
            }
        }
    }

    fn prev_zip(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self.all_zips_in_folder.iter().position(|p| Some(p) == self.zip_path.as_ref()) {
            if pos > 0 {
                let prev_path = self.all_zips_in_folder[pos - 1].clone();
                // We pass 'true' to load_zip so it knows to start at the end of the new file
                self.load_source(prev_path, ctx);
            } else {
                self.show_fading_error("No previous zip files in folder.");
            }
        }
    }

    fn next_folder(&mut self, ctx: &egui::Context) {
        let (_, next_dir) = Self::get_adjacent_directories(self.zip_path.clone());

        // Check if next_dir actually exists
        if let Some(dir) = next_dir {
            let zips = self.scan_folder(&*dir);
            if zips.is_empty() {
                self.show_fading_error("No Archive found in next folder.");
            } else {
                self.load_source(zips[0].clone(), ctx);
            }
        } else {
            self.show_fading_error("No next directory found.");
        }
    }

    fn prev_folder(&mut self, ctx: &egui::Context) {
        let (prev_dir, _) = Self::get_adjacent_directories(self.zip_path.clone());

        // Check if next_dir actually exists
        if let Some(dir) = prev_dir {
            let zips = self.scan_folder(&*dir);
            if zips.is_empty() {
                self.show_fading_error("No Archive found in previous folder.");
            } else {
                self.load_source(zips[0].clone(), ctx);
            }
        } else {
            self.show_fading_error("No previous directory found.");
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

    fn create_image_rect(&mut self, ui: &mut egui::Ui, rect: Rect, hit_id: &str, is_next: bool, tex_index: usize, ctx: &egui::Context, align: egui::Align) {
        ui.allocate_ui_at_rect(rect, |ui| {
            // 1. Create an invisible interaction area for the whole half
            let resp = ui.interact(rect, ui.id().with(hit_id), egui::Sense::click());
            if resp.clicked() {
                if is_next {
                    self.next_page(ctx);
                } else {
                    self.prev_page(ctx);
                }
            }

            // 2. Render the image on top
            if let Some(tex) = &self.textures[tex_index] {
                let layout = egui::Layout::top_down(align);
                ui.with_layout(layout, |ui| {
                    ui.add(egui::Image::new(tex)
                        .fit_to_exact_size(rect.size())
                        .maintain_aspect_ratio(true));
                });
            }
        });
    }


    fn is_single_page(&self) -> bool {
        self.config.page_view_options == PageViewOptions::Single
    }
}

impl eframe::App for MangaReader {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut action_to_run = MangaAction::None;

        // REBINDING LOGIC
        if let Some(action_name) = self.binding_action.clone() {
            ctx.input(|i| {
                for key in egui::Key::ALL {
                    // Check if a key is pressed and it's NOT just a modifier key alone
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
                            "First Page" => self.config.keys.first_page = new_shortcut,
                            "Last Page" => self.config.keys.last_page = new_shortcut,
                            "Next File" => self.config.keys.next_file = new_shortcut,
                            "Previous File" => self.config.keys.prev_file = new_shortcut,
                            "Next Folder" => self.config.keys.next_folder = new_shortcut,
                            "Previous Folder" => self.config.keys.prev_folder = new_shortcut,
                            "Fullscreen" => self.config.keys.fullscreen = new_shortcut,
                            "View Mode" => self.config.keys.view_mode = new_shortcut,
                            "Open File" => self.config.keys.open_file = new_shortcut,
                            _ => {}
                        }
                        self.binding_action = None;
                        self.save_settings(); // Save to JSON immediately
                    }
                }
            });
        }
        // PART B: EXECUTION LOGIC
        else {
            ctx.input(|i| {
                let keys = self.config.keys;

                // Helper to check if a shortcut is triggered
                let is_triggered = |s: &Shortcut| {
                    i.key_pressed(s.key) && i.modifiers.ctrl == s.ctrl &&
                        i.modifiers.alt == s.alt && i.modifiers.shift == s.shift
                };

                if is_triggered(&keys.next_page) { action_to_run = MangaAction::NextPage; }
                if is_triggered(&keys.prev_page) { action_to_run = MangaAction::PrevPage; }
                if is_triggered(&keys.first_page) { action_to_run = MangaAction::FirstPage; }
                if is_triggered(&keys.last_page) { action_to_run = MangaAction::LastPage; }
                if is_triggered(&keys.next_file) { action_to_run = MangaAction::NextFile; }
                if is_triggered(&keys.prev_file) { action_to_run = MangaAction::PrevFile;}
                if is_triggered(&keys.next_folder) { action_to_run = MangaAction::NextFolder; }
                if is_triggered(&keys.prev_folder) { action_to_run = MangaAction::PrevFolder;}
                if is_triggered(&keys.fullscreen) { action_to_run = MangaAction::FullScreen; }
                if is_triggered(&keys.view_mode) { action_to_run = MangaAction::ViewMode; }
                if is_triggered(&keys.open_file) { action_to_run = MangaAction::OpenFile; }
            });
        }

        match action_to_run {
            MangaAction::NextPage => self.next_page(ctx),
            MangaAction::PrevPage => self.prev_page(ctx),
            MangaAction::FirstPage => self.go_to_first_page(ctx),
            MangaAction::LastPage => self.go_to_last_page(ctx),
            MangaAction::NextFile => self.next_zip(ctx),
            MangaAction::PrevFile => self.prev_zip(ctx),
            MangaAction::NextFolder => self.next_folder(ctx),
            MangaAction::PrevFolder => self.prev_folder(ctx),
            MangaAction::FullScreen => {
                self.is_fullscreen = !self.is_fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.is_fullscreen));
            },
            MangaAction::ViewMode => {
                self.is_shifted = !self.is_shifted;

                // Adjust current_index to keep the view consistent
                if self.is_shifted {
                    if self.current_index == 0 { self.current_index = 0; }
                    else if self.current_index % 2 == 0 { self.current_index += 1; }
                } else {
                    // Return to even index alignment
                    self.current_index = self.current_index.saturating_sub(1);
                    if self.current_index % 2 != 0 { self.current_index = self.current_index.saturating_sub(1); }
                }

                self.reset_buffer();
                self.texture_cache.clear();
                self.textures = self.load_pair(self.current_index, ctx);
                let msg = if self.is_shifted { "Mode: Cover + Spreads" } else { "Mode: Standard Pairs" };
                self.show_fading_error(msg); // Reusing your error logic to show the mode change
            },
            MangaAction::OpenFile => self.open_file_dialog(),
            MangaAction::None => {},
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

        // INSTANT STATE-BASED SCROLLING
        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta);
        let scroll_threshold = 2.0;

        // If the wheel is moving significantly
        if scroll_delta.y.abs() > scroll_threshold || scroll_delta.x.abs() > scroll_threshold {
            if self.can_scroll {
                if scroll_delta.y < -scroll_threshold || scroll_delta.x < -scroll_threshold {
                    self.next_page(ctx);
                } else if scroll_delta.y > scroll_threshold || scroll_delta.x > scroll_threshold {
                    self.prev_page(ctx);
                }
                // Lock the scrolling until it stops
                self.can_scroll = false;
            }
        } else {
            // The wheel has stopped or slowed down significantly
            self.can_scroll = true;
        }

        if self.config.show_settings {
            egui::SidePanel::right("settings_panel")
                .resizable(true) // Enable mouse dragging
                .default_width(self.config.settings_width)
                .width_range(150.0..=500.0) // Constraints
                .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(60)).inner_margin(10.0))
                .show(ctx, |ui| {
                    // Update the stored width based on user dragging
                    self.config.settings_width = ui.available_width();

                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.heading(
                            egui::RichText::new("Settings")
                                .color(egui::Color32::from_gray(200)) // Example: Orange
                                .strong()
                        );
                    });
                    ui.add_space(10.0);
                    ui.separator();

                    ui.vertical(|ui| {
                        ui.add_space(10.0);

                        // [Open File] Button
                        if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("📂 Open File")).clicked() {
                            self.open_file_dialog();
                        }

                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("Image Scaling Algorithm:").color(egui::Color32::from_gray(200)).size(20.0).strong());
                        ui.separator();

                        let visuals = ui.visuals_mut();
                        visuals.selection.bg_fill = egui::Color32::BLACK;
                        visuals.override_text_color = Some(egui::Color32::from_gray(200));

                        {
                            let mut changed = false;
                            changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::None, egui::RichText::new("None (Good for small image)")).clicked();
                            changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::Nearest, egui::RichText::new("Nearest (Fastest)")).clicked();
                            changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::Triangle, egui::RichText::new("Bilinear (Balance)")).clicked();
                            changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::CatmullRom, egui::RichText::new("Bicubic")).clicked();
                            changed |= ui.radio_value(&mut self.config.resize_method, ResizeMethod::Lanczos3, egui::RichText::new("Lanczos3 (High Quality, Slow)")).clicked();

                            if changed {
                                self.reset_buffer();
                                self.textures = self.load_pair(self.current_index, ctx);
                                self.save_settings(); // Save when algorithm changes
                            }
                        }

                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("Page Viewing Options:").color(egui::Color32::from_gray(200)).size(20.0).strong());
                        ui.separator();

                        {
                            let mut changed = false;
                            changed |= ui.radio_value(&mut self.config.page_view_options, PageViewOptions::Single, egui::RichText::new("Single Page")).clicked();
                            changed |= ui.radio_value(&mut self.config.page_view_options, PageViewOptions::DoubleRL, egui::RichText::new("Double Page(Right to Left")).clicked();
                            changed |= ui.radio_value(&mut self.config.page_view_options, PageViewOptions::DoubleLR, egui::RichText::new("Double Page(Left to Right)")).clicked();

                            if changed {
                                self.reset_buffer();
                                self.textures = self.load_pair(self.current_index, ctx);
                                self.save_settings();
                            }
                        }

                        ui.add_space(20.0);
                        ui.label(egui::RichText::new("Others:").color(egui::Color32::from_gray(200)).size(20.0).strong());
                        ui.separator();
                        ui.checkbox(&mut self.config.transparency_support, "Support Transparent Image")
                            .on_hover_text("Manga normally does not have transparent image, enable this will sacrifice load image speed by about 35%.");
                        ui.checkbox(&mut self.config.enable_single_file_caching, "Enable caching on single file")
                            .on_hover_text("Cached the image files already load on a single zip file. Cached will be cleared after loading next zip.");
                        ui.add(egui::Slider::new(&mut self.config.image_delay, 0..=1000)
                            .text("Image Delay (ms)")).on_hover_text("Delay time in between before the next image shown. Useful when holding next/prev image button.");
                        ui.add_space(20.0);

                        egui::CollapsingHeader::new(egui::RichText::new("Key Config").color(egui::Color32::from_gray(200)).size(20.0).strong())
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.separator();
                                egui::Grid::new("key_grid").num_columns(2).spacing([20.0, 10.0]).show(ui, |ui| {
                                    ui.label("Next Page:");
                                    render_binding_button(ui, "Next Page", &mut self.config.keys.next_page, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Previous Page:");
                                    render_binding_button(ui, "Previous Page", &mut self.config.keys.prev_page, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Go to First Page:");
                                    render_binding_button(ui, "First Page", &mut self.config.keys.first_page, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Go to Last Page:");
                                    render_binding_button(ui, "Last Page", &mut self.config.keys.last_page, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Next File:");
                                    render_binding_button(ui, "Next File", &mut self.config.keys.next_file, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Previous File:");
                                    render_binding_button(ui, "Previous File", &mut self.config.keys.prev_file, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Next Folder:");
                                    render_binding_button(ui, "Next Folder", &mut self.config.keys.next_folder, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Previous Folder:");
                                    render_binding_button(ui, "Previous Folder", &mut self.config.keys.prev_folder, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Toggle Fullscreen:");
                                    render_binding_button(ui, "Fullscreen", &mut self.config.keys.fullscreen, &mut self.binding_action);
                                    ui.end_row();
                                    ui.label("Odd/Even Page Start:");
                                    render_binding_button(ui, "View Mode", &mut self.config.keys.view_mode, &mut self.binding_action);
                                    ui.end_row();
                                });
                            });

                        // Helper function to keep the UI code clean
                        fn render_binding_button(ui: &mut egui::Ui, id: &str, shortcut: &mut Shortcut, binding: &mut Option<String>) {
                            let is_binding = binding.as_deref() == Some(id);
                            let text = if is_binding { "Listening...".to_string() } else { shortcut.format() };

                            if ui.button(egui::RichText::new(text).color(egui::Color32::from_gray(60))).clicked() {
                                *binding = Some(id.to_string());
                            }
                        }
                    });
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
                let text = if self.config.show_settings { "▶" } else { "◀" };

                // We use add_sized to force the 200px height
                let toggle_btn = egui::Button::new(egui::RichText::new(text).size(20.0));
                if ui.add_sized([25.0, button_height], toggle_btn).clicked() {
                    self.config.show_settings = !self.config.show_settings;
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(40)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                // Create a 'Response' for the entire background area first,
                // but we check it at the END of the code.
                let bg_response = ui.interact(rect, ui.id().with("bg"), egui::Sense::click());

                if self.zip_path.is_some() {
                    // Show single image on center if in shifted mode
                    if self.is_single_page() || (self.is_shifted && self.current_index == 0) {
                        // --- STANDALONE COVER VIEW ---
                        self.create_image_rect(ui, rect, "cover_hit", true, 0, ctx, egui::Align::Center);
                    } else {
                        let center = rect.center().x;
                        let mut left_half = egui::Rect::from_min_max(rect.min, egui::pos2(center, rect.max.y));
                        let mut right_half = egui::Rect::from_min_max(egui::pos2(center, rect.min.y), rect.max);
                        let mut align_for_left_side: Align = egui::Align::RIGHT;
                        let mut align_for_right_side: Align = egui::Align::LEFT;
                        if self.config.page_view_options == PageViewOptions::DoubleLR {
                            std::mem::swap(&mut left_half, &mut right_half);
                            align_for_left_side = egui::Align::LEFT;
                            align_for_right_side = egui::Align::RIGHT;
                        }

                        self.create_image_rect(ui, left_half, "left_hit", true, 1, ctx, align_for_left_side);
                        self.create_image_rect(ui, right_half, "right_hit", false, 0, ctx, align_for_right_side);

                        // ONLY TRIGGER IF BACKGROUND WAS CLICKED
                        // bg_response.clicked() is true if the background was clicked.
                        // However, we only want to trigger if a specific image wasn't the target.
                        if bg_response.clicked() && !ctx.is_using_pointer() && !ctx.input(|i| i.pointer.any_down()) {
                            // Extra safety: check if we are actually hovering an image
                            if !left_half.contains(ctx.pointer_interact_pos().unwrap_or_default()) &&
                                !right_half.contains(ctx.pointer_interact_pos().unwrap_or_default()) {
                                self.open_file_dialog();
                            }
                        }
                    }
                } else {
                    // the start screen
                    ui.centered_and_justified(|ui| {
                        let start_btn = egui::Button::new(
                            egui::RichText::new("Click anywhere to open a Zip file")
                                .size(20.0)
                                .color(egui::Color32::from_gray(200))
                        ).fill(egui::Color32::from_gray(40));
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
                        let left_half = egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y));
                        let right_half = egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max);

                        let pointer_pos = ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default();

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
                        let padding = if self.config.show_settings { -(self.config.settings_width/2.0)  } else { 0.0 };
                        egui::Window::new("")
                            .anchor(egui::Align2::CENTER_TOP, [padding, 20.0]) // Positioned at top center
                            .frame(egui::Frame::window(&ui.style())
                                .fill(egui::Color32::from_black_alpha((180.0 * opacity) as u8))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha((50.0 * opacity) as u8))))
                            .title_bar(false)
                            .show(ctx, |ui| {
                                ui.label(egui::RichText::new(msg)
                                    .color(egui::Color32::from_white_alpha((255.0 * opacity) as u8))
                                    .size(24.0)
                                    .strong());
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
                        let padding = if self.config.show_settings { -(self.config.settings_width/2.0) } else { 0.0 };
                        egui::Window::new("zip_name_overlay")
                            .anchor(egui::Align2::CENTER_TOP, [padding, 80.0]) // Positioned at top center
                            .frame(egui::Frame::window(&ui.style())
                                .fill(egui::Color32::from_black_alpha((180.0 * opacity) as u8))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha((50.0 * opacity) as u8))))
                            .title_bar(false)
                            .resizable(false)
                            .show(ctx, |ui| {
                                ui.label(egui::RichText::new(name)
                                    .color(egui::Color32::from_white_alpha((255.0 * opacity) as u8))
                                    .size(24.0)
                                    .strong());
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
                        let padding = if self.config.show_settings { -30.0 - self.config.settings_width } else { -15.0 };
                        egui::Window::new("page_info")
                            .anchor(egui::Align2::RIGHT_TOP, [padding, 10.0])
                            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgba_unmultiplied(60,60,60,(opacity*255.0) as u8)).inner_margin(5.0).corner_radius(5.0)) // No background box
                            .title_bar(false)
                            .resizable(false)
                            .collapsible(false)
                            .fixed_size([300.0, 60.0]) // Force a wide area to prevent wrapping
                            .show(ctx, |ui| {
                                // Ensure text stays on one line
                                ui.horizontal(|ui| {
                                    let page_text = format!("{} / {}", self.current_index + 1, self.image_files.len());
                                    ui.label(egui::RichText::new(page_text)
                                        .color(egui::Color32::from_white_alpha((200.0 * opacity) as u8))
                                        .size(22.0) // Much larger font
                                        .strong());
                                });
                            });
                        ctx.request_repaint();
                    } else {
                        self.page_indicator_time = None;
                    }
                }
            });

        // Keep preloading buffers
        self.update_buffers(ctx);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Instant};
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::StrCmpLogicalW;
use std::os::windows::ffi::OsStrExt;
use image::DynamicImage;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(true)
            .with_decorations(true),
        ..Default::default()
    };

    eframe::run_native(
        "Manga Reader",
        options,
        Box::new(|cc| Ok(Box::new(MangaReader::new(cc)))),
    )
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum ResizeMethod {
    Nearest,
    Triangle,   // Bilinear
    CatmullRom, // Bicubic
    Lanczos3,   // High Quality
}

impl ResizeMethod {
    fn to_filter(self) -> image::imageops::FilterType {
        match self {
            ResizeMethod::Nearest => image::imageops::FilterType::Nearest,
            ResizeMethod::Triangle => image::imageops::FilterType::Triangle,
            ResizeMethod::CatmullRom => image::imageops::FilterType::CatmullRom,
            ResizeMethod::Lanczos3 => image::imageops::FilterType::Lanczos3,
        }
    }
}

struct MangaReader {
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
    can_scroll: bool, // New flag
    dialog_rx: Receiver<Option<PathBuf>>,
    dialog_tx: Sender<Option<PathBuf>>,
    page_indicator_time: Option<Instant>,
    was_image_clicked: bool,
    last_dialog_time: std::time::Instant,
    is_dialog_open: bool,
    zip_name_display: Option<(String, Instant)>,
    is_shifted: bool,
    resize_method: ResizeMethod,
}

impl MangaReader {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        Self {
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
            was_image_clicked: false,
            last_dialog_time: Instant::now(),
            is_dialog_open: false,
            zip_name_display: None,
            is_shifted: false,
            resize_method: ResizeMethod::Triangle,
        }
    }

    fn open_file_dialog(&mut self) {
        let now = std::time::Instant::now();
        // Only allow opening if at least 500ms has passed since the last one
        if now.duration_since(self.last_dialog_time) > std::time::Duration::from_millis(500) {
            self.last_dialog_time = now;
            if !self.is_dialog_open {
                self.is_dialog_open = true;

                let sender = self.dialog_tx.clone();
                std::thread::spawn(move || {
                    let file = rfd::FileDialog::new()
                        .add_filter("Zip files", &["zip"])
                        .pick_file();
                    let _ = sender.send(file);
                });
            }
        }
    }

    fn scan_folder(&mut self, current_zip: &Path) {
        if let Some(parent) = current_zip.parent() {
            let mut zips = Vec::new();
            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "zip") {
                        zips.push(path);
                    }
                }
            }
            zips.sort_by(|a, b| {
                let a_name: Vec<u16> = a.file_name().unwrap_or_default().encode_wide().chain(Some(0)).collect();
                let b_name: Vec<u16> = b.file_name().unwrap_or_default().encode_wide().chain(Some(0)).collect();

                let result = unsafe {
                    StrCmpLogicalW(PCWSTR(a_name.as_ptr()), PCWSTR(b_name.as_ptr()))
                };

                match result {
                    r if r < 0 => std::cmp::Ordering::Less,
                    r if r > 0 => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                }
            });
            self.all_zips_in_folder = zips;
        }
    }

    fn update_buffers(&mut self, ctx: &egui::Context) {
        let idx = self.current_index;

        // Only update if we moved OR if the buffers were recently consumed
        if self.last_buffered_index == Some(idx) {
            return;
        }

        // Preload Next (2 pages ahead)
        if self.buffer_next[0].is_none() {
            self.buffer_next = self.load_pair(idx + 2, ctx);
        }

        // Preload Prev (2 pages behind)
        if idx >= 2 && self.buffer_prev[0].is_none() {
            self.buffer_prev = self.load_pair(idx - 2, ctx);
        }

        self.last_buffered_index = Some(idx);
    }

    fn load_pair(&self, start_idx: usize, ctx: &egui::Context) -> [Option<egui::TextureHandle>; 2] {
        let mut pair: [Option<egui::TextureHandle>; 2] = [None, None];

        // Check if we even have a zip file loaded
        let zip_path = match &self.zip_path {
            Some(p) => p,
            None => return pair,
        };

        // Open the archive once for both images to save overhead
        let file = match File::open(zip_path) {
            Ok(f) => f,
            Err(_) => return pair,
        };

        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(_) => return pair,
        };

        for i in 0..2 {
            let current_target = start_idx + i;

            // Safety check for index bounds
            if let Some(filename) = self.image_files.get(current_target) {
                if let Ok(mut zip_file) = archive.by_name(filename) {
                    let mut buffer = Vec::new();
                    if zip_file.read_to_end(&mut buffer).is_ok() {
                        // Decode image from memory
                        if let Ok(img) = image::load_from_memory(&buffer) {
                            pair[i] = self.load_texture(img, i, ctx);
                        }
                    }
                }
            }
        }
        pair
    }

    fn load_texture(&self, img: DynamicImage, i:usize, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let resize_start = Instant::now();
        let filter = self.resize_method.to_filter();
        let screen_size = ctx.content_rect().size();
        let h = screen_size.y as u32;
        let w = img.width() as f32 * (screen_size.y / img.height() as f32);
        let processed_img = img.resize(w as u32, h, filter);
        let _resize_time = resize_start.elapsed();

        let process_start = Instant::now();
        let size = [processed_img.width() as _, processed_img.height() as _];
        let color_img = egui::ColorImage::from_rgba_unmultiplied(
            size,
            processed_img.to_rgba8().as_flat_samples().as_slice(),
        );
        let _process_time = process_start.elapsed();

        #[cfg(debug_assertions)]
        {
            println!("----------------------------------");
            println!("resize_time: {:?} ({:?}, {:?})", _resize_time, w, h);
            println!("process_time: {:?}", _process_time);
            println!("total: {:?}", _process_time + _resize_time);
            println!("filter: {:?}", filter);
            println!("img_{:?}", i);
            println!("----------------------------------");
        }
        Some(ctx.load_texture(
            format!("img_{}", i),
            color_img,
            egui::TextureOptions::LINEAR // GPU still uses Linear for the final sub-pixel pass
        ))
    }

    fn load_zip(&mut self, path: PathBuf, ctx: &egui::Context, start_at_end: bool) {
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => return,
        };

        if let Ok(mut archive) = zip::ZipArchive::new(file) {
            let mut images = Vec::new();
            let exts = ["png", "jpg", "jpeg", "bmp", "webp"];
            for i in 0..archive.len() {
                if let Ok(f) = archive.by_index(i) {
                    let name = f.name().to_lowercase();
                    if exts.iter().any(|&e| name.ends_with(&format!(".{}", e))) {
                        images.push(f.name().to_string());
                    }
                }
            }
            images.sort();

            if images.is_empty() {
                self.show_fading_error("Zip contains no images.");
            } else {
                // Reset buffer when loading another zip
                self.buffer_next = [None, None];
                self.buffer_prev = [None, None];

                self.scan_folder(&path);
                self.zip_path = Some(path.clone());

                // Trigger the Zip Name Overlay
                if let Some(file_name) = path.file_name() {
                    let name_str = file_name.to_string_lossy().to_string();
                    self.zip_name_display = Some((name_str, Instant::now()));
                }

                self.image_files = images;
                // If moving backward, start at the last possible pair
                self.current_index = if start_at_end {
                    let last = self.image_files.len().saturating_sub(1);
                    if self.is_shifted {
                        // Logic to find the last odd-starting pair
                        ((last.saturating_sub(1) / 2) * 2) + 1
                    } else {
                        (last / 2) * 2
                    }
                } else {
                    0
                };
                self.load_images(ctx);
            }
        }
    }

    fn load_images(&mut self, ctx: &egui::Context) {
        self.textures = [None, None];
        for i in 0..2 {
            if let Some(filename) = self.image_files.get(self.current_index + i) {
                if let Some(path) = &self.zip_path {
                    if let Ok(f) = File::open(path) {
                        if let Ok(mut arc) = zip::ZipArchive::new(f) {
                            if let Ok(mut zf) = arc.by_name(filename) {
                                let mut buf = Vec::new();
                                if zf.read_to_end(&mut buf).is_ok() {
                                    if let Ok(img) = image::load_from_memory(&buf) {
                                        self.textures[i] = self.load_texture(img, i, ctx);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn show_fading_error(&mut self, msg: &str) {
        self.error_msg = Some((msg.to_string(), Instant::now()));
    }

    fn next_page(&mut self, ctx: &egui::Context) {
        let step = if self.is_shifted && self.current_index == 0 { 1 } else { 2 };

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
        let step = if self.is_shifted && self.current_index == 1 { 1 } else { 2 };

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
                self.load_zip(next_path, ctx, false);
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
                self.load_zip(prev_path, ctx, true);
            } else {
                self.show_fading_error("No previous zip files in folder.");
            }
        }
    }

    fn go_to_first_page(&mut self, ctx: &egui::Context) {
        if !self.image_files.is_empty() && self.current_index != 0 {
            self.reset_buffer();
            self.current_index = 0;
            self.load_images(ctx);
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
                self.load_images(ctx);
                self.page_indicator_time = Some(Instant::now());
            }
        }
    }

    fn reset_buffer(&mut self) {
        self.buffer_prev = [None, None];
        self.buffer_next = [None, None];
    }
}

impl eframe::App for MangaReader {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // Fullscreen Toggle (Enter Key)
        if ctx.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl) {
            self.is_fullscreen = !self.is_fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.is_fullscreen));
        }

        // Toggle Page Shift (Enter Key)
        if ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.ctrl) {
            self.is_shifted = !self.is_shifted;

            // Adjust current_index to keep the view consistent
            if self.is_shifted {
                // If we were at 0, move to 1. Otherwise, ensure we land on an odd index.
                if self.current_index == 0 { self.current_index = 0; }
                else if self.current_index % 2 == 0 { self.current_index += 1; }
            } else {
                // Return to even index alignment
                self.current_index = self.current_index.saturating_sub(1);
                if self.current_index % 2 != 0 { self.current_index = self.current_index.saturating_sub(1); }
            }

            self.load_images(ctx);
            let msg = if self.is_shifted { "Mode: Cover + Spreads" } else { "Mode: Standard Pairs" };
            self.show_fading_error(msg); // Reusing your error logic to show the mode change
        }

        // File Dialog Result
        if let Ok(result) = self.dialog_rx.try_recv() {
            if let Some(path) = result {
                self.load_zip(path, ctx, false);
            }
        }

        // KEYBOARD NAVIGATION (Right-to-Left Flow)
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::A)) {
            self.next_page(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::D)) {
            self.prev_page(ctx);
        }

        // New: Home and End key support
        if ctx.input(|i| i.key_pressed(egui::Key::Home)) {
            self.go_to_first_page(ctx);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::End)) {
            self.go_to_last_page(ctx);
        }

        // Next/Prev file loading
        if ctx.input(|i| (i.key_pressed(egui::Key::ArrowLeft) && i.modifiers.ctrl) || (i.key_pressed(egui::Key::A) && i.modifiers.ctrl) ) {
            self.next_zip(ctx);
        }
        if ctx.input(|i| (i.key_pressed(egui::Key::ArrowRight) && i.modifiers.ctrl) || (i.key_pressed(egui::Key::D) && i.modifiers.ctrl) ) {
            self.prev_zip(ctx);
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

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();

                // Create a 'Response' for the entire background area first
                // but we check it at the END of the code.
                let bg_response = ui.interact(rect, ui.id().with("bg"), egui::Sense::click());

                if self.zip_path.is_some() {
                    // Show single image on center if in shifted mode
                    if self.is_shifted && self.current_index == 0 {
                        // --- STANDALONE COVER VIEW ---
                        ui.allocate_ui_at_rect(rect, |ui| {
                            let resp = ui.interact(rect, ui.id().with("cover_hit"), egui::Sense::click());
                            if resp.clicked() { self.next_page(ctx); }

                            ui.centered_and_justified(|ui| {
                                if let Some(tex) = &self.textures[0] {
                                    ui.add(egui::Image::new(tex)
                                        .fit_to_exact_size(rect.size())
                                        .maintain_aspect_ratio(true));
                                }
                            });
                        });
                    } else {
                        let left_half = egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y));
                        let right_half = egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max);

                        // --- LEFT PANE (Next) ---
                        ui.allocate_ui_at_rect(left_half, |ui| {
                            // 1. Create an invisible interaction area for the whole half
                            let resp = ui.interact(left_half, ui.id().with("left_hit"), egui::Sense::click());
                            if resp.clicked() {
                                self.next_page(ctx);
                                self.was_image_clicked = true;
                            }

                            // 2. Render the image on top (non-interactive so it doesn't block the area)
                            ui.centered_and_justified(|ui| {
                                if let Some(tex) = &self.textures[1] {
                                    ui.add(egui::Image::new(tex)
                                        .fit_to_exact_size(left_half.size())
                                        .maintain_aspect_ratio(true));
                                }
                            });
                        });

                        // --- RIGHT PANE (Prev) ---
                        ui.allocate_ui_at_rect(right_half, |ui| {
                            // 1. Create an invisible interaction area for the whole half
                            let resp = ui.interact(right_half, ui.id().with("right_hit"), egui::Sense::click());
                            if resp.clicked() {
                                self.prev_page(ctx);
                                self.was_image_clicked = true;
                            }

                            // 2. Render the image on top
                            ui.centered_and_justified(|ui| {
                                if let Some(tex) = &self.textures[0] {
                                    ui.add(egui::Image::new(tex)
                                        .fit_to_exact_size(right_half.size())
                                        .maintain_aspect_ratio(true));
                                }
                            });
                        });

                        // --- THE FIX: ONLY TRIGGER IF BACKGROUND WAS CLICKED ---
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
                    // --- THE START SCREEN ---
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new("Click anywhere to open a Zip file\n(Press Enter to toggle Fullscreen Mode)")
                            .heading()
                            .color(egui::Color32::GRAY));
                    });

                    // If we are in the start screen, any click on the panel opens the dialog
                    if ui.input(|i| i.pointer.any_click()) {
                        self.open_file_dialog();
                    }
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
                if elapsed < 3.0 {
                    let opacity = (1.0 - (elapsed / 3.0)).clamp(0.0, 1.0);
                    egui::Window::new("")
                        .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0]) // Positioned at top center
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
                let duration = 3.0; // Show for 3 seconds total

                if elapsed < duration {
                    let opacity = (1.0 - (elapsed / duration)).clamp(0.0, 1.0);

                    egui::Window::new("zip_name_overlay")
                        .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0]) // Positioned at top center
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

                    egui::Window::new("page_info")
                        .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
                        .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT)) // No background box
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
                                    .size(24.0) // Much larger font
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

        egui::Window::new("Settings")
            .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
            .collapsible(true)
            .show(ctx, |ui| {
                ui.label("Resizing Method:");
                let mut changed = false;

                changed |= ui.selectable_value(&mut self.resize_method, ResizeMethod::Nearest, "Nearest (Fastest)").clicked();
                changed |= ui.selectable_value(&mut self.resize_method, ResizeMethod::Triangle, "Bilinear").clicked();
                changed |= ui.selectable_value(&mut self.resize_method, ResizeMethod::CatmullRom, "Bicubic").clicked();
                changed |= ui.selectable_value(&mut self.resize_method, ResizeMethod::Lanczos3, "Lanczos3 (Best)").clicked();

                if changed {
                    // Reload images with the new filter
                    self.load_images(ctx);
                }
            });
    }
}
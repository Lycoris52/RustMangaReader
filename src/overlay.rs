use super::MangaReader;
use eframe::egui;

impl MangaReader {
    pub(super) fn render_error_overlay(&mut self, ctx: &egui::Context, ui: &egui::Ui) {
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
                                .color(egui::Color32::from_white_alpha((255.0 * opacity) as u8))
                                .size(24.0)
                                .strong(),
                        );
                    });
                ctx.request_repaint();
            } else {
                self.error_msg = None;
            }
        }
    }

    pub(super) fn render_zip_name_overlay(&mut self, ctx: &egui::Context, ui: &egui::Ui) {
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
                                .color(egui::Color32::from_white_alpha((255.0 * opacity) as u8))
                                .size(24.0)
                                .strong(),
                        );
                    });
                ctx.request_repaint(); // Keep the animation smooth
            } else {
                self.zip_name_display = None;
            }
        }
    }

    pub(super) fn render_page_indicator_overlay(&mut self, ctx: &egui::Context) {
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
                            let page_text =
                                format!("{} / {}", self.current_index + 1, self.image_files.len());
                            ui.label(
                                egui::RichText::new(page_text)
                                    .color(egui::Color32::from_white_alpha((200.0 * opacity) as u8))
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
    }
}

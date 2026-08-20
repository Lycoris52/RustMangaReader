use super::MangaReader;
use crate::config::{ImageSizingMode, MangaAction, PageViewOptions};
use crate::localize::tr;
use eframe::egui;
use egui::Align;

impl MangaReader {
    fn top_down_anchor_bias(&self, index: usize, size: egui::Vec2, rect: egui::Rect) -> f32 {
        if size == egui::Vec2::ZERO {
            return 0.0;
        }

        if index == 0 {
            (size.y - rect.height()) * 0.5
        } else if index + 1 >= self.image_files.len() {
            ((rect.height() - size.y) * 0.5).min(0.0)
        } else {
            0.0
        }
    }

    pub(super) fn render_page_view(
        &mut self,
        ctx: &egui::Context,
        action_to_run: &mut MangaAction,
    ) {
        if self.config.page_view_options == PageViewOptions::TopDown {
            self.render_top_down_page_view(ctx, action_to_run);
        } else {
            self.render_default_page_view(ctx, action_to_run);
        }
    }

    pub(super) fn render_default_page_view(
        &mut self,
        ctx: &egui::Context,
        action_to_run: &mut MangaAction,
    ) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.image_panel_background_color()))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                self.last_page_view_rect = Some(rect);

                // Create a 'Response' for the entire background area first,
                // but we check it at the END of the code.
                let bg_response = ui.interact(rect, ui.id().with("bg"), egui::Sense::hover());

                if self.zip_path.is_some() {
                    // Show single image on center or if in shifted cover mode
                    let is_zoomed = (self.zoom_factor - 1.0).abs() > 0.01;
                    let double_needs_drag = self.double_page_needs_drag(rect);
                    let viewing_single =
                        self.is_single_page() || (self.is_shifted && self.current_index == 0);
                    let single_image_size = self.textures[0]
                        .as_ref()
                        .map(|tex| self.image_draw_size(tex, rect.size(), self.zoom_factor))
                        .unwrap_or(egui::Vec2::ZERO);
                    let single_needs_drag =
                        single_image_size.x > rect.width() || single_image_size.y > rect.height();

                    if viewing_single && !single_needs_drag {
                        let single_response =
                            ui.interact(rect, ui.id().with("single_click"), egui::Sense::click());
                        if self.binding_action.is_none() && *action_to_run == MangaAction::None {
                            if let Some(mouse_action) =
                                self.collect_mouse_action(&single_response, ctx)
                            {
                                *action_to_run = mouse_action;
                            }
                        }
                        if let Some(tex) = &self.textures[0] {
                            ui.painter().image(
                                tex.id(),
                                egui::Rect::from_center_size(rect.center(), single_image_size),
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                    } else if viewing_single {
                        let pan_response = ui.interact(
                            rect,
                            ui.id().with("single_pan"),
                            egui::Sense::click_and_drag(),
                        );
                        if pan_response.dragged() {
                            self.pan_offset += ctx.input(|i| i.pointer.delta());
                            ctx.request_repaint();
                        }
                        if self.binding_action.is_none() && *action_to_run == MangaAction::None {
                            if let Some(mouse_action) =
                                self.collect_mouse_action(&pan_response, ctx)
                            {
                                *action_to_run = mouse_action;
                            }
                        }
                        if let Some(tex) = &self.textures[0] {
                            ui.painter().image(
                                tex.id(),
                                egui::Rect::from_center_size(
                                    rect.center() + self.pan_offset,
                                    single_image_size,
                                ),
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                    } else if is_zoomed || double_needs_drag {
                        let pan_response = ui.interact(
                            rect,
                            ui.id().with("spread_pan"),
                            egui::Sense::click_and_drag(),
                        );
                        if pan_response.dragged() {
                            self.pan_offset += ctx.input(|i| i.pointer.delta());
                            ctx.request_repaint();
                        }
                        if self.binding_action.is_none() && *action_to_run == MangaAction::None {
                            if let Some(mouse_action) =
                                self.collect_mouse_action(&pan_response, ctx)
                            {
                                *action_to_run = mouse_action;
                            }
                        }
                        let page_container_size = egui::vec2(
                            (rect.width() * 0.5 - self.config.spread_center_offset.abs()).max(1.0),
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
                        let top_y = rect.center().y - left_size.y.max(right_size.y) * 0.5
                            + self.pan_offset.y;
                        let image_top_y = |size: egui::Vec2| {
                            if self.config.image_sizing_mode == ImageSizingMode::FitBoth {
                                rect.center().y - size.y * 0.5 + self.pan_offset.y
                            } else {
                                top_y
                            }
                        };
                        let (visual_left_size, _) =
                            if self.config.page_view_options == PageViewOptions::DoubleLR {
                                (right_size, left_size)
                            } else {
                                (left_size, right_size)
                            };
                        let visual_left_x = center_axis - offset - visual_left_size.x;
                        let visual_right_x = center_axis + offset;
                        let clip_axis = center_axis.clamp(rect.min.x, rect.max.x);
                        let left_visible_rect =
                            egui::Rect::from_min_max(rect.min, egui::pos2(clip_axis, rect.max.y));
                        let right_visible_rect =
                            egui::Rect::from_min_max(egui::pos2(clip_axis, rect.min.y), rect.max);

                        if self.config.page_view_options == PageViewOptions::DoubleLR {
                            if let Some(tex) = &self.textures[0] {
                                self.paint_image_clipped(
                                    ui,
                                    tex,
                                    egui::Rect::from_min_size(
                                        egui::pos2(visual_left_x, image_top_y(right_size)),
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
                                        egui::pos2(visual_right_x, image_top_y(left_size)),
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
                                        egui::pos2(visual_left_x, image_top_y(left_size)),
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
                                        egui::pos2(visual_right_x, image_top_y(right_size)),
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
                        if self.binding_action.is_none() && *action_to_run == MangaAction::None {
                            if let Some(mouse_action) =
                                self.collect_mouse_action(&left_response, ctx)
                            {
                                *action_to_run = mouse_action;
                            } else if let Some(mouse_action) =
                                self.collect_mouse_action(&right_response, ctx)
                            {
                                *action_to_run = mouse_action;
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

                self.render_error_overlay(ctx, ui);
                self.render_zip_name_overlay(ctx, ui);
                self.render_page_indicator_overlay(ctx);
            });
    }

    pub(super) fn render_top_down_page_view(
        &mut self,
        ctx: &egui::Context,
        action_to_run: &mut MangaAction,
    ) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.image_panel_background_color()))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                self.last_page_view_rect = Some(rect);
                let response = ui.interact(
                    rect,
                    ui.id().with("top_down_bg"),
                    egui::Sense::click_and_drag(),
                );
                if self.binding_action.is_none() && *action_to_run == MangaAction::None {
                    if let Some(mouse_action) = self.collect_mouse_action(&response, ctx) {
                        *action_to_run = mouse_action;
                    }
                }
                if response.dragged() {
                    self.top_down_scroll_offset +=
                        ctx.input(|i| i.pointer.delta()).y * self.config.top_down_image_drag_speed;
                    self.clamp_top_down_scroll_offset();
                    ctx.request_repaint();
                }

                if self.zip_path.is_some() {
                    self.ensure_top_down_loaded_around(self.current_index, ctx);

                    let page_gap = 0.0;
                    let container_size = rect.size();
                    let mut current_size = self
                        .top_down_texture(self.current_index)
                        .map(|tex| self.top_down_image_draw_size(tex, container_size))
                        .unwrap_or(egui::Vec2::ZERO);
                    let mut prev_size = self
                        .current_index
                        .checked_sub(1)
                        .and_then(|index| self.top_down_texture(index))
                        .map(|tex| self.top_down_image_draw_size(tex, container_size))
                        .unwrap_or(egui::Vec2::ZERO);
                    let mut next_size = self
                        .top_down_texture(self.current_index + 1)
                        .map(|tex| self.top_down_image_draw_size(tex, container_size))
                        .unwrap_or(egui::Vec2::ZERO);

                    let mut current_bias =
                        self.top_down_anchor_bias(self.current_index, current_size, rect);
                    let mut current_position_offset = self.top_down_scroll_offset + current_bias;
                    let mut current_center_y = rect.center().y + current_position_offset;
                    let mut prev_distance = if prev_size != egui::Vec2::ZERO {
                        current_size.y * 0.5 + page_gap + prev_size.y * 0.5
                    } else {
                        0.0
                    };
                    let mut next_distance = if next_size != egui::Vec2::ZERO {
                        current_size.y * 0.5 + page_gap + next_size.y * 0.5
                    } else {
                        0.0
                    };

                    let mut changed_page = false;
                    if prev_distance > 0.0 && current_position_offset > prev_distance * 0.5 {
                        self.current_index -= 1;
                        current_position_offset -= prev_distance;
                        self.ensure_top_down_loaded_around(self.current_index, ctx);
                        self.page_indicator_time = Some(std::time::Instant::now());
                        changed_page = true;
                        ctx.request_repaint();
                    } else if next_distance > 0.0
                        && current_position_offset < -(next_distance * 0.5)
                    {
                        self.current_index += 1;
                        current_position_offset += next_distance;
                        self.ensure_top_down_loaded_around(self.current_index, ctx);
                        self.page_indicator_time = Some(std::time::Instant::now());
                        changed_page = true;
                        ctx.request_repaint();
                    }

                    if changed_page {
                        current_size = self
                            .top_down_texture(self.current_index)
                            .map(|tex| self.top_down_image_draw_size(tex, container_size))
                            .unwrap_or(egui::Vec2::ZERO);
                        prev_size = self
                            .current_index
                            .checked_sub(1)
                            .and_then(|index| self.top_down_texture(index))
                            .map(|tex| self.top_down_image_draw_size(tex, container_size))
                            .unwrap_or(egui::Vec2::ZERO);
                        next_size = self
                            .top_down_texture(self.current_index + 1)
                            .map(|tex| self.top_down_image_draw_size(tex, container_size))
                            .unwrap_or(egui::Vec2::ZERO);
                        current_bias =
                            self.top_down_anchor_bias(self.current_index, current_size, rect);
                        prev_distance = if prev_size != egui::Vec2::ZERO {
                            current_size.y * 0.5 + page_gap + prev_size.y * 0.5
                        } else {
                            0.0
                        };
                        next_distance = if next_size != egui::Vec2::ZERO {
                            current_size.y * 0.5 + page_gap + next_size.y * 0.5
                        } else {
                            0.0
                        };
                    }

                    self.top_down_scroll_offset = current_position_offset - current_bias;
                    current_center_y = rect.center().y + current_position_offset;

                    if let Some(prev_index) = self.current_index.checked_sub(1) {
                        if let Some(tex) = self.top_down_texture(prev_index) {
                            let size = self.top_down_image_draw_size(tex, container_size);
                            let center =
                                egui::pos2(rect.center().x, current_center_y - prev_distance);
                            ui.painter().image(
                                tex.id(),
                                egui::Rect::from_center_size(center, size),
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );

                            if let Some(prev_prev_index) = self.current_index.checked_sub(2) {
                                if let Some(prev_prev_tex) = self.top_down_texture(prev_prev_index)
                                {
                                    let prev_prev_size = self
                                        .top_down_image_draw_size(prev_prev_tex, container_size);
                                    let prev_prev_distance = prev_distance
                                        + size.y * 0.5
                                        + page_gap
                                        + prev_prev_size.y * 0.5;
                                    let prev_prev_center = egui::pos2(
                                        rect.center().x,
                                        current_center_y - prev_prev_distance,
                                    );
                                    ui.painter().image(
                                        prev_prev_tex.id(),
                                        egui::Rect::from_center_size(
                                            prev_prev_center,
                                            prev_prev_size,
                                        ),
                                        egui::Rect::from_min_max(
                                            egui::Pos2::ZERO,
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                }
                            }
                        }
                    }
                    if let Some(tex) = self.top_down_texture(self.current_index) {
                        let size = self.top_down_image_draw_size(tex, container_size);
                        let center = egui::pos2(rect.center().x, current_center_y);
                        ui.painter().image(
                            tex.id(),
                            egui::Rect::from_center_size(center, size),
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                    if let Some(tex) = self.top_down_texture(self.current_index + 1) {
                        let size = self.top_down_image_draw_size(tex, container_size);
                        let center = egui::pos2(rect.center().x, current_center_y + next_distance);
                        ui.painter().image(
                            tex.id(),
                            egui::Rect::from_center_size(center, size),
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );

                        if let Some(next_next_tex) = self.top_down_texture(self.current_index + 2) {
                            let next_next_size =
                                self.top_down_image_draw_size(next_next_tex, container_size);
                            let next_next_distance =
                                next_distance + size.y * 0.5 + page_gap + next_next_size.y * 0.5;
                            let next_next_center =
                                egui::pos2(rect.center().x, current_center_y + next_next_distance);
                            ui.painter().image(
                                next_next_tex.id(),
                                egui::Rect::from_center_size(next_next_center, next_next_size),
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                } else {
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

                self.render_error_overlay(ctx, ui);
                self.render_zip_name_overlay(ctx, ui);
                self.render_page_indicator_overlay(ctx);
            });
    }
}

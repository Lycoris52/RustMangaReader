use std::sync::Arc;

pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // include the bytes of a .ttf file in your binary:
    fonts.font_data.insert(
        "notosan".to_owned(),
        Arc::from(egui::FontData::from_static(include_bytes!("assets/NotoSansJP-Regular.ttf"))),
    );

    // Put notosan top priority for both Proportional and Monospace
    fonts.families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "notosan".to_owned());

    fonts.families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .push("notosan".to_owned());

    // 3. Tell egui to use these fonts
    ctx.set_fonts(fonts);
}
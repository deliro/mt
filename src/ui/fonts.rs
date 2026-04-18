use eframe::egui;

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/Inter-Regular.ttf");

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("inter".to_owned(), egui::FontData::from_static(INTER_REGULAR));
    prepend(&mut fonts, egui::FontFamily::Proportional, "inter");
    prepend(&mut fonts, egui::FontFamily::Monospace, "inter");
    ctx.set_fonts(fonts);
}

fn prepend(fonts: &mut egui::FontDefinitions, family: egui::FontFamily, name: &str) {
    fonts.families.entry(family).or_default().insert(0, name.to_owned());
}

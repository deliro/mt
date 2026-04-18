use eframe::egui;

const DEJAVU_SANS: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("dejavu".to_owned(), egui::FontData::from_static(DEJAVU_SANS));
    prepend(&mut fonts, egui::FontFamily::Proportional, "dejavu");
    prepend(&mut fonts, egui::FontFamily::Monospace, "dejavu");
    ctx.set_fonts(fonts);
}

fn prepend(fonts: &mut egui::FontDefinitions, family: egui::FontFamily, name: &str) {
    fonts.families.entry(family).or_default().insert(0, name.to_owned());
}

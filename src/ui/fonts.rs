use eframe::egui;

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/Inter-Regular.ttf");
const DEJAVU_SANS: &[u8] = include_bytes!("../../assets/DejaVuSans.ttf");

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("inter".to_owned(), egui::FontData::from_static(INTER_REGULAR));
    fonts
        .font_data
        .insert("dejavu".to_owned(), egui::FontData::from_static(DEJAVU_SANS));
    prepend(&mut fonts, egui::FontFamily::Proportional, "inter");
    prepend(&mut fonts, egui::FontFamily::Monospace, "inter");
    append(&mut fonts, egui::FontFamily::Proportional, "dejavu");
    append(&mut fonts, egui::FontFamily::Monospace, "dejavu");
    ctx.set_fonts(fonts);
}

fn prepend(fonts: &mut egui::FontDefinitions, family: egui::FontFamily, name: &str) {
    fonts.families.entry(family).or_default().insert(0, name.to_owned());
}

fn append(fonts: &mut egui::FontDefinitions, family: egui::FontFamily, name: &str) {
    fonts.families.entry(family).or_default().push(name.to_owned());
}

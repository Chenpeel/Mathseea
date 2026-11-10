use eframe::egui::{self, Color32, FontId, Response, RichText, Stroke, TextStyle, Ui, Vec2};

pub const CANVAS: Color32 = Color32::from_rgb(8, 12, 21);
pub const PANEL: Color32 = Color32::from_rgb(15, 21, 33);
pub const SURFACE: Color32 = Color32::from_rgb(24, 33, 49);
pub const BORDER: Color32 = Color32::from_rgb(43, 56, 76);
pub const TEXT: Color32 = Color32::from_rgb(224, 233, 247);
pub const MUTED: Color32 = Color32::from_rgb(142, 160, 186);
pub const ACCENT: Color32 = Color32::from_rgb(105, 153, 255);
pub const SUCCESS: Color32 = Color32::from_rgb(104, 211, 169);

pub fn apply(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);
    let mut style = (*context.global_style()).clone();
    style.spacing.item_spacing = Vec2::splat(8.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style.spacing.window_margin = egui::Margin::same(16);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(20.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = CANVAS;
    visuals.faint_bg_color = SURFACE;
    visuals.selection.bg_fill = Color32::from_rgb(40, 70, 118);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.window_corner_radius = 8.into();
    visuals.menu_corner_radius = 8.into();
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = 6.into();
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
    }
    visuals.widgets.noninteractive.fg_stroke.color = MUTED;
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 49, 71);
    visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
    visuals.widgets.hovered.bg_stroke.color = ACCENT;
    visuals.widgets.active.bg_fill = Color32::from_rgb(42, 66, 108);
    visuals.widgets.active.weak_bg_fill = visuals.widgets.active.bg_fill;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT);
    visuals.widgets.open = visuals.widgets.active;
    style.visuals = visuals;
    context.set_global_style(style);
}

pub fn button(ui: &mut Ui, label: &str, primary: bool, enabled: bool) -> Response {
    ui.scope(|ui| {
        if primary {
            let widgets = &mut ui.style_mut().visuals.widgets;
            widgets.inactive.weak_bg_fill = Color32::from_rgb(57, 102, 191);
            widgets.hovered.weak_bg_fill = Color32::from_rgb(72, 122, 219);
            widgets.active.weak_bg_fill = Color32::from_rgb(43, 82, 160);
        }
        ui.add_enabled(
            enabled,
            egui::Button::new(label).min_size(Vec2::new(0.0, 34.0)),
        )
    })
    .inner
}

pub fn section(ui: &mut Ui, title: &str, detail: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).strong().color(TEXT));
    if !detail.is_empty() {
        ui.label(RichText::new(detail).small().color(MUTED));
    }
    ui.add_space(4.0);
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .inner_margin(16)
        .stroke(Stroke::new(1.0, BORDER))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkbenchSize {
    Compact,
    Wide,
}
pub fn workbench_size(width: f32) -> WorkbenchSize {
    if width >= 1180.0 {
        WorkbenchSize::Wide
    } else {
        WorkbenchSize::Compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_has_consistent_controls_and_responsive_breakpoint() {
        let context = egui::Context::default();
        apply(&context);
        let style = context.global_style();
        assert_eq!(style.spacing.item_spacing, Vec2::splat(8.0));
        assert!((32.0..=36.0).contains(&style.spacing.interact_size.y));
        assert_ne!(
            style.visuals.widgets.inactive.weak_bg_fill,
            style.visuals.widgets.hovered.weak_bg_fill
        );
        assert_ne!(
            style.visuals.widgets.inactive.bg_stroke,
            style.visuals.widgets.active.bg_stroke
        );
        assert_eq!(workbench_size(640.0), WorkbenchSize::Compact);
        assert_eq!(workbench_size(1440.0), WorkbenchSize::Wide);
    }
}

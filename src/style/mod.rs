use egui::style::{Selection, WidgetVisuals, Widgets, default_text_styles};
use egui::{Context, FontId, Spacing, Stroke, Style, TextStyle, Theme, Vec2, Visuals};

mod colors;
pub use colors::Palette;

pub fn install_style(ctx: &Context) {
    ctx.options_mut(|opt| {
        opt.light_style = style_for_theme(Theme::Light).into();
        opt.dark_style = style_for_theme(Theme::Dark).into();
    });
}

fn style_for_theme(theme: Theme) -> Style {
    let palette = Palette::for_theme(theme);
    let base = theme.default_style();

    let widgets = Widgets {
        noninteractive: WidgetVisuals {
            bg_fill: palette.bg_2,
            bg_stroke: Stroke {
                color: palette.separator_1,
                ..base.visuals.widgets.noninteractive.bg_stroke
            },
            fg_stroke: Stroke {
                color: palette.fg_1,
                ..base.visuals.widgets.noninteractive.fg_stroke
            },
            ..base.visuals.widgets.noninteractive
        },
        inactive: WidgetVisuals {
            weak_bg_fill: palette.bg_2,
            bg_fill: palette.bg_3,
            bg_stroke: Stroke {
                color: palette.separator_1,
                ..base.visuals.widgets.inactive.bg_stroke
            },
            fg_stroke: Stroke {
                color: palette.fg_1,
                ..base.visuals.widgets.inactive.fg_stroke
            },
            ..base.visuals.widgets.inactive
        },
        hovered: WidgetVisuals {
            weak_bg_fill: palette.bg_3,
            bg_fill: palette.bg_3,
            bg_stroke: Stroke {
                color: palette.separator_2,
                ..base.visuals.widgets.hovered.bg_stroke
            },
            fg_stroke: Stroke {
                color: palette.fg_0,
                ..base.visuals.widgets.hovered.fg_stroke
            },
            ..base.visuals.widgets.hovered
        },
        active: WidgetVisuals {
            weak_bg_fill: palette.bg_3,
            bg_fill: palette.bg_3,
            bg_stroke: Stroke {
                color: palette.separator_2,
                ..base.visuals.widgets.active.bg_stroke
            },
            fg_stroke: Stroke {
                color: palette.fg_0,
                ..base.visuals.widgets.active.fg_stroke
            },
            ..base.visuals.widgets.active
        },
        open: WidgetVisuals {
            weak_bg_fill: palette.bg_3,
            bg_fill: palette.bg_3,
            bg_stroke: Stroke {
                color: palette.separator_1,
                ..base.visuals.widgets.open.bg_stroke
            },
            fg_stroke: Stroke {
                color: palette.fg_0,
                ..base.visuals.widgets.open.fg_stroke
            },
            ..base.visuals.widgets.open
        },
    };

    let selection = Selection {
        bg_fill: palette.bg_accent,
        ..base.visuals.selection
    };

    let visuals = Visuals {
        widgets,
        selection,
        hyperlink_color: palette.fg_accent,
        error_fg_color: palette.fg_serious,
        warn_fg_color: palette.fg_moderate,
        weak_text_color: Some(palette.fg_3),
        panel_fill: palette.bg_2,
        window_fill: palette.bg_2,
        window_stroke: Stroke {
            color: palette.separator_1,
            ..base.visuals.window_stroke
        },
        extreme_bg_color: palette.bg_1,
        faint_bg_color: palette.bg_3,
        code_bg_color: palette.bg_3,
        ..base.visuals
    };

    let spacing = Spacing {
        menu_margin: egui::Margin::symmetric(10, 6),
        item_spacing: Vec2::new(8., 4.),
        ..Default::default()
    };

    let mut text_styles = default_text_styles();
    text_styles.insert(TextStyle::Small, FontId::proportional(10.));

    Style {
        visuals,
        spacing,
        text_styles,
        ..base
    }
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "publicsans".to_owned(),
        egui::FontData::from_static(include_bytes!("../../resources/publicsans-regular.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "firacode".to_owned(),
        egui::FontData::from_static(include_bytes!("../../resources/firacode.ttf")).into(),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "publicsans".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "firacode".to_owned());

    ctx.set_fonts(fonts);
}

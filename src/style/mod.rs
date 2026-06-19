use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{Context, Stroke, Style, Theme, Visuals};

mod colors;
pub use colors::Palette;

pub fn install_custom_style(ctx: &Context) {
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

    Style { visuals, ..base }
}

use egui::{Color32, Theme};

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    pub fg_0: Color32,
    pub fg_1: Color32,
    pub fg_2: Color32,
    pub fg_3: Color32,
    pub fg_4: Color32,
    pub fg_serious: Color32,
    pub fg_success: Color32,
    pub fg_moderate: Color32,
    pub fg_accent: Color32,
    pub bg_1: Color32,
    pub bg_2: Color32,
    pub bg_3: Color32,
    pub bg_serious: Color32,
    pub bg_success: Color32,
    pub bg_moderate: Color32,
    pub bg_accent: Color32,
    pub separator_1: Color32,
    pub separator_2: Color32,
    pub separator_serious: Color32,
    pub separator_success: Color32,
    pub separator_moderate: Color32,
    pub separator_accent: Color32,
}

impl Palette {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Self::DARK,
            Theme::Light => Self::LIGHT,
        }
    }

    pub(super) const LIGHT: Palette = Palette {
        fg_0: Color32::BLACK,
        fg_1: Color32::from_rgb(59, 59, 59),
        fg_2: Color32::from_rgb(77, 77, 77),
        fg_3: Color32::from_rgb(99, 99, 99),
        fg_4: Color32::from_rgb(112, 112, 112),
        fg_serious: Color32::from_rgb(219, 27, 6),
        fg_success: Color32::from_rgb(0, 100, 48),
        fg_moderate: Color32::from_rgb(166, 80, 6),
        fg_accent: Color32::from_rgb(0, 44, 242),
        bg_1: Color32::from_rgb(249, 249, 246),
        bg_2: Color32::from_rgb(243, 243, 241),
        bg_3: Color32::from_rgb(226, 226, 223),
        bg_serious: Color32::from_rgb(250, 230, 230),
        bg_success: Color32::from_rgb(209, 250, 223),
        bg_moderate: Color32::from_rgb(250, 237, 209),
        bg_accent: Color32::from_rgb(226, 235, 253),
        separator_1: Color32::from_rgb(219, 219, 216),
        separator_2: Color32::from_rgb(189, 189, 187),
        separator_serious: Color32::from_rgb(255, 205, 199),
        separator_success: Color32::from_rgb(148, 220, 181),
        separator_moderate: Color32::from_rgb(251, 215, 160),
        separator_accent: Color32::from_rgb(179, 204, 255),
    };

    pub(super) const DARK: Palette = Palette {
        fg_0: Color32::WHITE,
        fg_1: Color32::from_rgb(204, 204, 204),
        fg_2: Color32::from_rgb(178, 178, 178),
        fg_3: Color32::from_rgb(153, 153, 153),
        fg_4: Color32::from_rgb(102, 102, 102),
        fg_serious: Color32::from_rgb(255, 117, 102),
        fg_success: Color32::from_rgb(59, 201, 129),
        fg_moderate: Color32::from_rgb(255, 183, 82),
        fg_accent: Color32::from_rgb(31, 213, 249),
        bg_1: Color32::from_rgb(7, 7, 7),
        bg_2: Color32::from_rgb(19, 19, 19),
        bg_3: Color32::from_rgb(32, 32, 32),
        bg_serious: Color32::from_rgb(31, 14, 11),
        bg_success: Color32::from_rgb(0, 25, 5),
        bg_moderate: Color32::from_rgb(26, 14, 4),
        bg_accent: Color32::from_rgb(5, 21, 24),
        separator_1: Color32::from_rgb(32, 32, 32),
        separator_2: Color32::from_rgb(48, 48, 47),
        separator_serious: Color32::from_rgb(90, 28, 22),
        separator_success: Color32::from_rgb(0, 50, 19),
        separator_moderate: Color32::from_rgb(63, 34, 8),
        separator_accent: Color32::from_rgb(1, 42, 50),
    };
}

//! BODKA presentation settings. Kept separate from upstream gameplay and authentication.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    RoyalBrown,
    WhiteMinimal,
    FrostedGlass,
    UltraDark,
    #[default]
    #[serde(other)]
    ClassicBlack,
}

impl Theme {
    pub const ALL: [Self; 5] = [
        Self::ClassicBlack,
        Self::RoyalBrown,
        Self::WhiteMinimal,
        Self::FrostedGlass,
        Self::UltraDark,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::ClassicBlack => "Classic black",
            Self::RoyalBrown => "Royal brown",
            Self::WhiteMinimal => "White minimal",
            Self::FrostedGlass => "Frosted glass",
            Self::UltraDark => "Ultra dark",
        }
    }
    pub fn palette(self) -> Palette {
        match self {
            Self::ClassicBlack => Palette {
                background: 0x090909,
                panel: 0x171514,
                accent: 0xa77b52,
                ink: 0xf5f5f5,
                opacity: 0.97,
                radius: 8.,
                shadow: 0.22,
            },
            Self::RoyalBrown => Palette {
                background: 0x160f0c,
                panel: 0x38281f,
                accent: 0xc59668,
                ink: 0xf5eee7,
                opacity: 0.95,
                radius: 6.,
                shadow: 0.32,
            },
            Self::WhiteMinimal => Palette {
                background: 0xf5f5f5,
                panel: 0xffffff,
                accent: 0x805430,
                ink: 0x242424,
                opacity: 0.98,
                radius: 4.,
                shadow: 0.08,
            },
            Self::FrostedGlass => Palette {
                background: 0x161616,
                panel: 0x38322d,
                accent: 0xc29c78,
                ink: 0xf5f5f5,
                opacity: 0.76,
                radius: 12.,
                shadow: 0.18,
            },
            Self::UltraDark => Palette {
                background: 0x000000,
                panel: 0x090909,
                accent: 0xa77b52,
                ink: 0xf5f5f5,
                opacity: 1.,
                radius: 2.,
                shadow: 0.,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: u32,
    pub panel: u32,
    pub accent: u32,
    pub ink: u32,
    pub opacity: f32,
    pub radius: f32,
    pub shadow: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Full,
    Reduced,
    Off,
    #[default]
    #[serde(other)]
    Balanced,
}

impl Motion {
    pub const ALL: [Self; 4] = [Self::Full, Self::Balanced, Self::Reduced, Self::Off];
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full motion",
            Self::Balanced => "Balanced motion",
            Self::Reduced => "Reduced motion",
            Self::Off => "Motion off",
        }
    }
    pub fn duration_ms(self, requested: u64, system_reduced: bool) -> u64 {
        if self == Self::Off {
            0
        } else if system_reduced || self == Self::Reduced {
            requested.min(90)
        } else if self == Self::Balanced {
            requested.min(200)
        } else {
            requested
        }
    }
    pub fn moves(self, system_reduced: bool, active: bool) -> bool {
        active && !system_reduced && matches!(self, Self::Full | Self::Balanced)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub motion: Motion,
    pub animated_background: bool,
    pub background_intensity: u8,
    pub dim: u8,
    pub brightness: u8,
    pub background_blur: u8,
    /// Only public, signature-verified Horizon builds; never an entitlement bypass.
    pub download_on_select: bool,
    pub dual_source: crate::dual::Source,
    pub dual_folder: Option<std::path::PathBuf>,
    pub selected_candidate: Option<String>,
    pub sound: bool,
    pub volume: u8,
    pub soft_sound: bool,
    pub opening_seen: bool,
    pub sunset_background: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            motion: Motion::default(),
            animated_background: true,
            background_intensity: 30,
            dim: 35,
            brightness: 100,
            background_blur: 0,
            download_on_select: false,
            dual_source: Default::default(),
            dual_folder: None,
            selected_candidate: None,
            sound: false,
            volume: 25,
            soft_sound: true,
            opening_seen: false,
            sunset_background: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_and_future_settings_do_not_erase_preferences() {
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert!(!old.sound && !old.download_on_select);
        let future: Settings =
            serde_json::from_str(r#"{"theme":"future","motion":"future","dim":62}"#).unwrap();
        assert_eq!(future.dim, 62);
        let saved = serde_json::to_string(&future).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&saved).unwrap().dim, 62);
    }
    #[test]
    fn motion_policy_respects_accessibility_and_inactive_windows() {
        for mode in Motion::ALL {
            assert!(!mode.moves(false, false));
            assert!(!mode.moves(true, true));
        }
        assert_eq!(Motion::Off.duration_ms(400, false), 0);
        assert_eq!(Motion::Full.duration_ms(400, true), 90);
        assert_eq!(Motion::Balanced.duration_ms(400, false), 200);
        assert_eq!(Motion::Full.duration_ms(400, false), 400);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchMode {
    Unused,
    Mru,
    AllLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookKeyCombo {
    CapsLock,
    WinSpace,
    AltShift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleSize {
    ExtraSmall,
    Small,
    Medium,
    Large,
    ExtraLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Carousel,
    Simple,
    Expanded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
    Custom,
}

pub const OPACITY_VALUES: [u8; 7] = [64, 128, 191, 217, 230, 242, 255];

#[derive(Debug, Clone, Copy)]
pub struct CustomThemeColors {
    pub bg_color: u32,
    pub fg_color: u32,
    pub opacity: u8,
}

impl Default for CustomThemeColors {
    fn default() -> Self {
        Self {
            bg_color: 0x002D2D2D,
            fg_color: 0x00FFFFFF,
            opacity: 0xDD,
        }
    }
}

impl BubbleSize {
    pub fn metrics(self) -> SizeMetrics {
        match self {
            BubbleSize::ExtraSmall => SizeMetrics {
                item_width: 18.0,
                item_height: 16.0,
                font_size: 11.0,
                padding: 3.0,
                corner_radius: 5.0,
            },
            BubbleSize::Small => SizeMetrics {
                item_width: 24.0,
                item_height: 20.0,
                font_size: 14.0,
                padding: 4.0,
                corner_radius: 6.0,
            },
            BubbleSize::Medium => SizeMetrics {
                item_width: 30.0,
                item_height: 24.0,
                font_size: 18.0,
                padding: 6.0,
                corner_radius: 8.0,
            },
            BubbleSize::Large => SizeMetrics {
                item_width: 40.0,
                item_height: 32.0,
                font_size: 22.0,
                padding: 8.0,
                corner_radius: 10.0,
            },
            BubbleSize::ExtraLarge => SizeMetrics {
                item_width: 50.0,
                item_height: 40.0,
                font_size: 28.0,
                padding: 10.0,
                corner_radius: 12.0,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SizeMetrics {
    pub item_width: f32,
    pub item_height: f32,
    pub font_size: f32,
    pub padding: f32,
    pub corner_radius: f32,
}

impl SwitchMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "MRU" => SwitchMode::Mru,
            "AllLanguage" => SwitchMode::AllLanguage,
            _ => SwitchMode::Unused,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SwitchMode::Unused => "Unused",
            SwitchMode::Mru => "MRU",
            SwitchMode::AllLanguage => "AllLanguage",
        }
    }
}

impl BubbleSize {
    pub fn from_str(s: &str) -> Self {
        match s {
            "ExtraSmall" => BubbleSize::ExtraSmall,
            "Small" => BubbleSize::Small,
            "Large" => BubbleSize::Large,
            "ExtraLarge" => BubbleSize::ExtraLarge,
            _ => BubbleSize::Medium,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            BubbleSize::ExtraSmall => "ExtraSmall",
            BubbleSize::Small => "Small",
            BubbleSize::Medium => "Medium",
            BubbleSize::Large => "Large",
            BubbleSize::ExtraLarge => "ExtraLarge",
        }
    }
}

impl DisplayMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Simple" => DisplayMode::Simple,
            "Expanded" => DisplayMode::Expanded,
            _ => DisplayMode::Carousel,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DisplayMode::Carousel => "Carousel",
            DisplayMode::Simple => "Simple",
            DisplayMode::Expanded => "Expanded",
        }
    }
}

impl ThemeMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Light" => ThemeMode::Light,
            "Dark" => ThemeMode::Dark,
            "Custom" => ThemeMode::Custom,
            _ => ThemeMode::System,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "System",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::Custom => "Custom",
        }
    }
}

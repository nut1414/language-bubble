#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchMode {
    Unused,
    Mru,
    AllLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HookKeyCombo {
    CapsLock = 0,
    WinSpace = 1,
    AltShift = 2,
}

impl HookKeyCombo {
    pub const ALL: [Self; 3] = [Self::CapsLock, Self::WinSpace, Self::AltShift];

    const fn index(self) -> usize {
        self as usize
    }
}

impl TryFrom<usize> for HookKeyCombo {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::CapsLock),
            1 => Ok(Self::WinSpace),
            2 => Ok(Self::AltShift),
            _ => Err(()),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindingConfig {
    pub switch_mode: SwitchMode,
    pub display_mode: DisplayMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindings {
    entries: [KeyBindingConfig; 3],
}

impl KeyBindings {
    pub const fn new(
        caps_lock: KeyBindingConfig,
        win_space: KeyBindingConfig,
        alt_shift: KeyBindingConfig,
    ) -> Self {
        Self {
            entries: [caps_lock, win_space, alt_shift],
        }
    }

    pub fn get(&self, combo: HookKeyCombo) -> KeyBindingConfig {
        self.entries[combo.index()]
    }

    pub fn set_switch_mode(&mut self, combo: HookKeyCombo, mode: SwitchMode) {
        self.entries[combo.index()].switch_mode = mode;
    }

    pub fn set_display_mode(&mut self, combo: HookKeyCombo, mode: DisplayMode) {
        self.entries[combo.index()].display_mode = mode;
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::new(
            KeyBindingConfig {
                switch_mode: SwitchMode::AllLanguage,
                display_mode: DisplayMode::Carousel,
            },
            KeyBindingConfig {
                switch_mode: SwitchMode::Unused,
                display_mode: DisplayMode::Carousel,
            },
            KeyBindingConfig {
                switch_mode: SwitchMode::Unused,
                display_mode: DisplayMode::Carousel,
            },
        )
    }
}

impl Default for CustomThemeColors {
    fn default() -> Self {
        Self {
            bg_color: 0x002D2D2D,
            fg_color: 0x00FFFFFF,
            opacity: 217,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_key_combo_message_values_are_stable_and_checked() {
        for (value, combo) in HookKeyCombo::ALL.into_iter().enumerate() {
            assert_eq!(combo as usize, value);
            assert_eq!(HookKeyCombo::try_from(value), Ok(combo));
        }
        assert_eq!(HookKeyCombo::try_from(3), Err(()));
        assert_eq!(HookKeyCombo::try_from(usize::MAX), Err(()));
    }

    #[test]
    fn key_bindings_preserve_existing_defaults() {
        let bindings = KeyBindings::default();
        assert_eq!(
            bindings.get(HookKeyCombo::CapsLock),
            KeyBindingConfig {
                switch_mode: SwitchMode::AllLanguage,
                display_mode: DisplayMode::Carousel,
            }
        );
        for combo in [HookKeyCombo::WinSpace, HookKeyCombo::AltShift] {
            assert_eq!(
                bindings.get(combo),
                KeyBindingConfig {
                    switch_mode: SwitchMode::Unused,
                    display_mode: DisplayMode::Carousel,
                }
            );
        }
    }

    #[test]
    fn key_bindings_update_only_the_selected_combo() {
        let mut bindings = KeyBindings::default();
        bindings.set_switch_mode(HookKeyCombo::WinSpace, SwitchMode::Mru);
        bindings.set_display_mode(HookKeyCombo::WinSpace, DisplayMode::Expanded);

        assert_eq!(
            bindings.get(HookKeyCombo::WinSpace),
            KeyBindingConfig {
                switch_mode: SwitchMode::Mru,
                display_mode: DisplayMode::Expanded,
            }
        );
        assert_eq!(
            bindings.get(HookKeyCombo::CapsLock),
            KeyBindings::default().get(HookKeyCombo::CapsLock)
        );
        assert_eq!(
            bindings.get(HookKeyCombo::AltShift),
            KeyBindings::default().get(HookKeyCombo::AltShift)
        );
    }

    #[test]
    fn persisted_enum_values_round_trip() {
        for mode in [SwitchMode::Unused, SwitchMode::Mru, SwitchMode::AllLanguage] {
            assert_eq!(SwitchMode::from_str(mode.as_str()), mode);
        }
        for size in [
            BubbleSize::ExtraSmall,
            BubbleSize::Small,
            BubbleSize::Medium,
            BubbleSize::Large,
            BubbleSize::ExtraLarge,
        ] {
            assert_eq!(BubbleSize::from_str(size.as_str()), size);
        }
        for mode in [
            DisplayMode::Carousel,
            DisplayMode::Simple,
            DisplayMode::Expanded,
        ] {
            assert_eq!(DisplayMode::from_str(mode.as_str()), mode);
        }
        for mode in [
            ThemeMode::System,
            ThemeMode::Light,
            ThemeMode::Dark,
            ThemeMode::Custom,
        ] {
            assert_eq!(ThemeMode::from_str(mode.as_str()), mode);
        }
    }

    #[test]
    fn unknown_persisted_values_keep_existing_fallbacks() {
        assert_eq!(SwitchMode::from_str("unknown"), SwitchMode::Unused);
        assert_eq!(BubbleSize::from_str("unknown"), BubbleSize::Medium);
        assert_eq!(DisplayMode::from_str("unknown"), DisplayMode::Carousel);
        assert_eq!(ThemeMode::from_str("unknown"), ThemeMode::System);
    }
}

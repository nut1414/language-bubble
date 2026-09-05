use crate::types::*;

// Context menu command IDs
const CMD_EXIT: u16 = 1000;
const CMD_START_WITH_WINDOWS: u16 = 1001;
const CMD_HIDE_ON_TYPING: u16 = 1002;
const CMD_EXPANDED_MRU_ONLY: u16 = 1003;
const CMD_FEEDBACK: u16 = 1004;

// Size: 1100-1104
const CMD_SIZE_BASE: u16 = 1100;
// Key bindings switch mode: CapsLock 1300-1302, WinSpace 1310-1312, AltShift 1320-1322
const CMD_KEY_CAPSLOCK_BASE: u16 = 1300;
const CMD_KEY_WINSPACE_BASE: u16 = 1310;
const CMD_KEY_ALTSHIFT_BASE: u16 = 1320;
// Key bindings display mode: CapsLock 1330-1332, WinSpace 1340-1342, AltShift 1350-1352
const CMD_KEY_CAPSLOCK_DISPLAY_BASE: u16 = 1330;
const CMD_KEY_WINSPACE_DISPLAY_BASE: u16 = 1340;
const CMD_KEY_ALTSHIFT_DISPLAY_BASE: u16 = 1350;
// Theme: 1400=System, 1401=Light, 1402=Dark, 1403=Custom
const CMD_THEME_BASE: u16 = 1400;
const CMD_CUSTOM_BG_COLOR: u16 = 1410;
const CMD_CUSTOM_FG_COLOR: u16 = 1411;
const CMD_OPACITY_BASE: u16 = 1420;
const CMD_CHECK_UPDATES_TOGGLE: u16 = 1503;
const CMD_DOWNLOAD_UPDATE: u16 = 1504;

const SIZE_VALUES: [BubbleSize; 5] = [
    BubbleSize::ExtraSmall,
    BubbleSize::Small,
    BubbleSize::Medium,
    BubbleSize::Large,
    BubbleSize::ExtraLarge,
];
const SWITCH_MODE_VALUES: [SwitchMode; 3] =
    [SwitchMode::AllLanguage, SwitchMode::Mru, SwitchMode::Unused];
const DISPLAY_MODE_VALUES: [DisplayMode; 3] = [
    DisplayMode::Carousel,
    DisplayMode::Simple,
    DisplayMode::Expanded,
];
const THEME_VALUES: [ThemeMode; 4] = [
    ThemeMode::System,
    ThemeMode::Light,
    ThemeMode::Dark,
    ThemeMode::Custom,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Exit,
    ToggleStartWithWindows,
    ToggleHideOnTyping,
    ToggleExpandedMruOnly,
    Feedback,
    SetSize(BubbleSize),
    SetSwitchMode {
        combo: HookKeyCombo,
        mode: SwitchMode,
    },
    SetDisplayMode {
        combo: HookKeyCombo,
        mode: DisplayMode,
    },
    SetTheme(ThemeMode),
    PickCustomBackground,
    PickCustomForeground,
    SetOpacity(u8),
    ToggleUpdateChecks,
    DownloadUpdate,
}

impl TrayCommand {
    fn id(self) -> Option<u16> {
        let id = match self {
            Self::Exit => CMD_EXIT,
            Self::ToggleStartWithWindows => CMD_START_WITH_WINDOWS,
            Self::ToggleHideOnTyping => CMD_HIDE_ON_TYPING,
            Self::ToggleExpandedMruOnly => CMD_EXPANDED_MRU_ONLY,
            Self::Feedback => CMD_FEEDBACK,
            Self::SetSize(size) => CMD_SIZE_BASE + value_index(&SIZE_VALUES, size)? as u16,
            Self::SetSwitchMode { combo, mode } => {
                switch_mode_base(combo) + value_index(&SWITCH_MODE_VALUES, mode)? as u16
            }
            Self::SetDisplayMode { combo, mode } => {
                display_mode_base(combo) + value_index(&DISPLAY_MODE_VALUES, mode)? as u16
            }
            Self::SetTheme(theme) => CMD_THEME_BASE + value_index(&THEME_VALUES, theme)? as u16,
            Self::PickCustomBackground => CMD_CUSTOM_BG_COLOR,
            Self::PickCustomForeground => CMD_CUSTOM_FG_COLOR,
            Self::SetOpacity(value) => {
                CMD_OPACITY_BASE + value_index(&OPACITY_VALUES, value)? as u16
            }
            Self::ToggleUpdateChecks => CMD_CHECK_UPDATES_TOGGLE,
            Self::DownloadUpdate => CMD_DOWNLOAD_UPDATE,
        };
        Some(id)
    }

    pub(super) fn from_id(id: u16) -> Option<Self> {
        match id {
            CMD_EXIT => return Some(Self::Exit),
            CMD_START_WITH_WINDOWS => return Some(Self::ToggleStartWithWindows),
            CMD_HIDE_ON_TYPING => return Some(Self::ToggleHideOnTyping),
            CMD_EXPANDED_MRU_ONLY => return Some(Self::ToggleExpandedMruOnly),
            CMD_FEEDBACK => return Some(Self::Feedback),
            CMD_CUSTOM_BG_COLOR => return Some(Self::PickCustomBackground),
            CMD_CUSTOM_FG_COLOR => return Some(Self::PickCustomForeground),
            CMD_CHECK_UPDATES_TOGGLE => return Some(Self::ToggleUpdateChecks),
            CMD_DOWNLOAD_UPDATE => return Some(Self::DownloadUpdate),
            _ => {}
        }

        if let Some(size) = value_from_id(id, CMD_SIZE_BASE, &SIZE_VALUES) {
            return Some(Self::SetSize(size));
        }
        if let Some(theme) = value_from_id(id, CMD_THEME_BASE, &THEME_VALUES) {
            return Some(Self::SetTheme(theme));
        }
        if let Some(value) = value_from_id(id, CMD_OPACITY_BASE, &OPACITY_VALUES) {
            return Some(Self::SetOpacity(value));
        }

        for combo in HookKeyCombo::ALL {
            if let Some(mode) = value_from_id(id, switch_mode_base(combo), &SWITCH_MODE_VALUES) {
                return Some(Self::SetSwitchMode { combo, mode });
            }
            if let Some(mode) = value_from_id(id, display_mode_base(combo), &DISPLAY_MODE_VALUES) {
                return Some(Self::SetDisplayMode { combo, mode });
            }
        }

        None
    }
}

fn value_index<T: Copy + PartialEq>(values: &[T], value: T) -> Option<usize> {
    values.iter().position(|candidate| *candidate == value)
}

fn value_from_id<T: Copy>(id: u16, base: u16, values: &[T]) -> Option<T> {
    let index = id.checked_sub(base)? as usize;
    values.get(index).copied()
}

const fn switch_mode_base(combo: HookKeyCombo) -> u16 {
    match combo {
        HookKeyCombo::CapsLock => CMD_KEY_CAPSLOCK_BASE,
        HookKeyCombo::WinSpace => CMD_KEY_WINSPACE_BASE,
        HookKeyCombo::AltShift => CMD_KEY_ALTSHIFT_BASE,
    }
}

const fn display_mode_base(combo: HookKeyCombo) -> u16 {
    match combo {
        HookKeyCombo::CapsLock => CMD_KEY_CAPSLOCK_DISPLAY_BASE,
        HookKeyCombo::WinSpace => CMD_KEY_WINSPACE_DISPLAY_BASE,
        HookKeyCombo::AltShift => CMD_KEY_ALTSHIFT_DISPLAY_BASE,
    }
}

pub(super) fn command_id(command: TrayCommand) -> usize {
    command.id().expect("menu command must have a stable ID") as usize
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn all_commands() -> Vec<TrayCommand> {
        let mut commands = vec![
            TrayCommand::Exit,
            TrayCommand::ToggleStartWithWindows,
            TrayCommand::ToggleHideOnTyping,
            TrayCommand::ToggleExpandedMruOnly,
            TrayCommand::Feedback,
            TrayCommand::PickCustomBackground,
            TrayCommand::PickCustomForeground,
            TrayCommand::ToggleUpdateChecks,
            TrayCommand::DownloadUpdate,
        ];
        commands.extend(SIZE_VALUES.into_iter().map(TrayCommand::SetSize));
        commands.extend(THEME_VALUES.into_iter().map(TrayCommand::SetTheme));
        commands.extend(OPACITY_VALUES.into_iter().map(TrayCommand::SetOpacity));
        for combo in HookKeyCombo::ALL {
            commands.extend(
                SWITCH_MODE_VALUES
                    .into_iter()
                    .map(|mode| TrayCommand::SetSwitchMode { combo, mode }),
            );
            commands.extend(
                DISPLAY_MODE_VALUES
                    .into_iter()
                    .map(|mode| TrayCommand::SetDisplayMode { combo, mode }),
            );
        }
        commands
    }

    #[test]
    fn every_menu_command_id_round_trips_and_is_unique() {
        let commands = all_commands();
        let mut ids = HashSet::new();
        for command in commands {
            let id = command.id().expect("supported command should have an ID");
            assert!(ids.insert(id), "duplicate command ID: {id}");
            assert_eq!(TrayCommand::from_id(id), Some(command));
        }
    }

    #[test]
    fn command_ids_preserve_win32_compatibility() {
        let fixed = [
            (TrayCommand::Exit, 1000),
            (TrayCommand::ToggleStartWithWindows, 1001),
            (TrayCommand::ToggleHideOnTyping, 1002),
            (TrayCommand::ToggleExpandedMruOnly, 1003),
            (TrayCommand::Feedback, 1004),
            (TrayCommand::PickCustomBackground, 1410),
            (TrayCommand::PickCustomForeground, 1411),
            (TrayCommand::ToggleUpdateChecks, 1503),
            (TrayCommand::DownloadUpdate, 1504),
        ];
        for (command, expected) in fixed {
            assert_eq!(command.id(), Some(expected));
        }

        for (index, size) in SIZE_VALUES.into_iter().enumerate() {
            assert_eq!(TrayCommand::SetSize(size).id(), Some(1100 + index as u16));
        }
        for (index, theme) in THEME_VALUES.into_iter().enumerate() {
            assert_eq!(TrayCommand::SetTheme(theme).id(), Some(1400 + index as u16));
        }
        for (index, opacity) in OPACITY_VALUES.into_iter().enumerate() {
            assert_eq!(
                TrayCommand::SetOpacity(opacity).id(),
                Some(1420 + index as u16)
            );
        }

        let switch_bases = [1300, 1310, 1320];
        let display_bases = [1330, 1340, 1350];
        for (combo_index, combo) in HookKeyCombo::ALL.into_iter().enumerate() {
            for (mode_index, mode) in SWITCH_MODE_VALUES.into_iter().enumerate() {
                assert_eq!(
                    TrayCommand::SetSwitchMode { combo, mode }.id(),
                    Some(switch_bases[combo_index] + mode_index as u16)
                );
            }
            for (mode_index, mode) in DISPLAY_MODE_VALUES.into_iter().enumerate() {
                assert_eq!(
                    TrayCommand::SetDisplayMode { combo, mode }.id(),
                    Some(display_bases[combo_index] + mode_index as u16)
                );
            }
        }
    }

    #[test]
    fn invalid_and_gap_ids_are_rejected() {
        for id in [
            0,
            999,
            1005,
            1099,
            1105,
            1299,
            1303,
            1313,
            1323,
            1333,
            1343,
            1353,
            1404,
            1412,
            1419,
            1427,
            1502,
            1505,
            u16::MAX,
        ] {
            assert_eq!(TrayCommand::from_id(id), None, "ID {id} should be invalid");
        }
        assert_eq!(TrayCommand::SetOpacity(0).id(), None);
    }
}

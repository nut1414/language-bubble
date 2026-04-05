using System.Windows;
using System.Windows.Controls;
using LanguageBubble.Native;
using LanguageBubble.Services;
using Application = System.Windows.Application;

namespace LanguageBubble;

public partial class App : Application
{
    private Mutex? _singleInstanceMutex;
    private KeyboardHook? _keyboardHook;
    private LanguageService? _languageService;
    private BubbleWindow? _bubbleWindow;
    private NativeTrayIcon? _trayIcon;
    private ContextMenu? _contextMenu;
    private bool _isSwitching;
    private bool _useMruSwitching;
    private bool _hideOnTyping;
    private bool _expandedMruOnly;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        // Single-instance check
        _singleInstanceMutex = new Mutex(true, "Global\\LanguageBubble_SingleInstance", out bool createdNew);
        if (!createdNew)
        {
            MessageBox.Show("Language Bubble is already running.", "Language Bubble",
                MessageBoxButton.OK, MessageBoxImage.Information);
            Shutdown();
            return;
        }

        // Initialize services
        _languageService = new LanguageService();
        _languageService.RefreshLayouts();

        _useMruSwitching = GetSavedMruSwitching();
        _hideOnTyping = GetSavedHideOnTyping();
        _expandedMruOnly = GetSavedExpandedMruOnly();

        // Seed MRU with the currently active language
        var initialLayout = _languageService.GetCurrentLayout();
        if (initialLayout != null)
            _languageService.RecordLayoutUsage(initialLayout.Hkl);

        // Create bubble window
        _bubbleWindow = new BubbleWindow();
        _bubbleWindow.SetSize(GetSavedBubbleSize());
        _bubbleWindow.CurrentDisplayMode = GetSavedDisplayMode();

        // Install keyboard hook
        _keyboardHook = new KeyboardHook();
        _keyboardHook.CapsLockPressed += OnCapsLockPressed;
        _keyboardHook.KeyPressed += OnKeyPressed;
        _keyboardHook.Install();

        // Force Caps Lock off on startup
        CapsLockService.EnsureCapsLockOff(_keyboardHook);

        // Setup system tray
        SetupTrayIcon();
    }

    private void SetupTrayIcon()
    {
        var iconPath = System.IO.Path.Combine(
            AppDomain.CurrentDomain.BaseDirectory, "Resources", "app.ico");

        _trayIcon = new NativeTrayIcon();
        _trayIcon.Create("Language Bubble", iconPath);
        _trayIcon.RightClick += OnTrayRightClick;
    }

    private void OnTrayRightClick()
    {
        BuildContextMenu();
        _contextMenu!.IsOpen = true;
    }

    private void BuildContextMenu()
    {
        if (_languageService == null) return;

        _contextMenu = new ContextMenu();

        // Header
        _contextMenu.Items.Add(new MenuItem
        {
            Header = "Languages",
            IsEnabled = false,
            FontWeight = FontWeights.Bold
        });
        _contextMenu.Items.Add(new Separator());

        // Language items
        var current = _languageService.GetCurrentLayout();
        foreach (var layout in _languageService.Layouts)
        {
            var item = new MenuItem
            {
                Header = $"{layout.DisplayName} - {layout.Culture.EnglishName}",
                IsCheckable = true,
                IsChecked = current != null && layout.Hkl == current.Hkl
            };
            _contextMenu.Items.Add(item);
        }

        _contextMenu.Items.Add(new Separator());

        // Start with Windows toggle
        var startWithWindows = new MenuItem
        {
            Header = "Start with Windows",
            IsCheckable = true,
            IsChecked = IsStartWithWindowsEnabled()
        };
        startWithWindows.Click += (_, _) =>
        {
            ToggleStartWithWindows(startWithWindows.IsChecked);
        };
        _contextMenu.Items.Add(startWithWindows);

        // Size submenu
        var sizeMenu = new MenuItem { Header = "Size" };
        var currentSize = _bubbleWindow?.CurrentSize ?? BubbleSize.Medium;
        var sizeOptions = new (string Label, BubbleSize Value)[]
        {
            ("Extra Small", BubbleSize.ExtraSmall),
            ("Small", BubbleSize.Small),
            ("Medium", BubbleSize.Medium),
            ("Large", BubbleSize.Large),
            ("Extra Large", BubbleSize.ExtraLarge),
        };
        foreach (var (label, value) in sizeOptions)
        {
            var sizeItem = new MenuItem
            {
                Header = label,
                IsCheckable = true,
                IsChecked = value == currentSize
            };
            var captured = value;
            sizeItem.Click += (_, _) =>
            {
                _bubbleWindow?.SetSize(captured);
                SaveBubbleSize(captured);
            };
            sizeMenu.Items.Add(sizeItem);
        }
        _contextMenu.Items.Add(sizeMenu);

        // Display mode submenu
        var modeMenu = new MenuItem { Header = "Display Mode" };
        var currentMode = _bubbleWindow?.CurrentDisplayMode ?? DisplayMode.Carousel;
        var modeOptions = new (string Label, DisplayMode Value)[]
        {
            ("Carousel", DisplayMode.Carousel),
            ("Simple", DisplayMode.Simple),
            ("Show All Languages", DisplayMode.Expanded),
        };
        foreach (var (label, value) in modeOptions)
        {
            var modeItem = new MenuItem
            {
                Header = label,
                IsCheckable = true,
                IsChecked = value == currentMode
            };
            var captured = value;
            modeItem.Click += (_, _) =>
            {
                if (_bubbleWindow != null)
                    _bubbleWindow.CurrentDisplayMode = captured;
                SaveDisplayMode(captured);
            };
            modeMenu.Items.Add(modeItem);
        }
        modeMenu.Items.Add(new Separator());
        var mruOnlyItem = new MenuItem
        {
            Header = "Show Only Recent Languages",
            IsCheckable = true,
            IsChecked = _expandedMruOnly
        };
        mruOnlyItem.Click += (_, _) =>
        {
            _expandedMruOnly = mruOnlyItem.IsChecked;
            SaveExpandedMruOnly(_expandedMruOnly);
        };
        modeMenu.Items.Add(mruOnlyItem);
        _contextMenu.Items.Add(modeMenu);

        // MRU switching toggle
        var mruSwitching = new MenuItem
        {
            Header = "Switch Between Recent and English",
            IsCheckable = true,
            IsChecked = _useMruSwitching
        };
        mruSwitching.Click += (_, _) =>
        {
            _useMruSwitching = mruSwitching.IsChecked;
            SaveMruSwitching(_useMruSwitching);
        };
        _contextMenu.Items.Add(mruSwitching);

        // Hide on typing toggle
        var hideOnTyping = new MenuItem
        {
            Header = "Hide on Typing",
            IsCheckable = true,
            IsChecked = _hideOnTyping
        };
        hideOnTyping.Click += (_, _) =>
        {
            _hideOnTyping = hideOnTyping.IsChecked;
            SaveHideOnTyping(_hideOnTyping);
        };
        _contextMenu.Items.Add(hideOnTyping);

        _contextMenu.Items.Add(new Separator());

        // Exit
        var exitItem = new MenuItem { Header = "Exit" };
        exitItem.Click += (_, _) => ExitApplication();
        _contextMenu.Items.Add(exitItem);
    }

    private void OnKeyPressed()
    {
        if (_hideOnTyping)
            Dispatcher.InvokeAsync(() => _bubbleWindow?.InstantHide());
    }

    private void OnCapsLockPressed()
    {
        // Debounce: ignore if already switching
        if (_isSwitching)
            return;

        _isSwitching = true;

        // Dispatch async to avoid blocking the hook callback
        Dispatcher.InvokeAsync(() =>
        {
            try
            {
                // Ensure Caps Lock stays off
                CapsLockService.EnsureCapsLockOff(_keyboardHook!);

                // Record the current layout before switching so external
                // changes (Win+Space, taskbar) are captured in MRU history
                var beforeSwitch = _languageService!.GetCurrentLayout();
                if (beforeSwitch != null)
                    _languageService.RecordLayoutUsage(beforeSwitch.Hkl);

                // Switch language
                var newLayout = _useMruSwitching
                    ? _languageService!.SwitchToMruLayout()
                    : _languageService!.SwitchToNextLayout();
                if (newLayout == null)
                    return;

                _languageService.RecordLayoutUsage(newLayout.Hkl);

                // Get caret position (physical pixels)
                var caretPos = CaretPositionService.GetCaretScreenPosition();

                // Pick which layouts to show in the bubble
                var displayLayouts = (_expandedMruOnly
                    && _bubbleWindow!.CurrentDisplayMode == DisplayMode.Expanded)
                    ? _languageService.GetMruLayouts()
                    : _languageService.Layouts;

                int selectedIndex = -1;
                for (int i = 0; i < displayLayouts.Count; i++)
                {
                    if (displayLayouts[i].Hkl == newLayout.Hkl)
                    {
                        selectedIndex = i;
                        break;
                    }
                }

                _bubbleWindow!.ShowBubble(displayLayouts, selectedIndex, caretPos);
            }
            finally
            {
                _isSwitching = false;
            }
        });
    }

    private void ExitApplication()
    {
        _keyboardHook?.Dispose();
        _trayIcon?.Dispose();

        _singleInstanceMutex?.ReleaseMutex();
        _singleInstanceMutex?.Dispose();

        Shutdown();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _keyboardHook?.Dispose();
        _trayIcon?.Dispose();

        _singleInstanceMutex?.Dispose();
        base.OnExit(e);
    }

    private static bool IsStartWithWindowsEnabled()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Run", false);
            return key?.GetValue("LanguageBubble") != null;
        }
        catch
        {
            return false;
        }
    }

    private static BubbleSize GetSavedBubbleSize()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\LanguageBubble", false);
            var value = key?.GetValue("Size") as string;
            if (value != null && Enum.TryParse<BubbleSize>(value, out var size))
                return size;
        }
        catch { }
        return BubbleSize.Medium;
    }

    private static void SaveBubbleSize(BubbleSize size)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(
                @"Software\LanguageBubble");
            key.SetValue("Size", size.ToString());
        }
        catch { }
    }

    private static bool GetSavedMruSwitching()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\LanguageBubble", false);
            var value = key?.GetValue("MruSwitching") as string;
            return value == "True";
        }
        catch { }
        return false;
    }

    private static void SaveMruSwitching(bool enabled)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(
                @"Software\LanguageBubble");
            key.SetValue("MruSwitching", enabled.ToString());
        }
        catch { }
    }

    private static bool GetSavedHideOnTyping()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\LanguageBubble", false);
            var value = key?.GetValue("HideOnTyping") as string;
            return value == "True";
        }
        catch { }
        return false;
    }

    private static void SaveHideOnTyping(bool enabled)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(
                @"Software\LanguageBubble");
            key.SetValue("HideOnTyping", enabled.ToString());
        }
        catch { }
    }

    private static DisplayMode GetSavedDisplayMode()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\LanguageBubble", false);
            var value = key?.GetValue("DisplayMode") as string;
            if (value != null && Enum.TryParse<DisplayMode>(value, out var mode))
                return mode;
        }
        catch { }
        return DisplayMode.Carousel;
    }

    private static void SaveDisplayMode(DisplayMode mode)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(
                @"Software\LanguageBubble");
            key.SetValue("DisplayMode", mode.ToString());
        }
        catch { }
    }

    private static bool GetSavedExpandedMruOnly()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\LanguageBubble", false);
            var value = key?.GetValue("ExpandedMruOnly") as string;
            return value == "True";
        }
        catch { }
        return false;
    }

    private static void SaveExpandedMruOnly(bool enabled)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(
                @"Software\LanguageBubble");
            key.SetValue("ExpandedMruOnly", enabled.ToString());
        }
        catch { }
    }

    private static void ToggleStartWithWindows(bool enable)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Run", true);

            if (key == null) return;

            if (enable)
            {
                string exePath = Environment.ProcessPath ?? "";
                if (!string.IsNullOrEmpty(exePath))
                {
                    key.SetValue("LanguageBubble", $"\"{exePath}\"");
                }
            }
            else
            {
                key.DeleteValue("LanguageBubble", false);
            }
        }
        catch
        {
            // Silently fail if registry access is denied
        }
    }
}

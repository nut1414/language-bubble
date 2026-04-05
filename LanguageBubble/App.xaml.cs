using System.Windows;
using System.Windows.Threading;
using LanguageBubble.Native;
using LanguageBubble.Services;
using WinForms = System.Windows.Forms;
using DrawingFont = System.Drawing.Font;
using DrawingFontStyle = System.Drawing.FontStyle;
using DrawingIcon = System.Drawing.Icon;
using DrawingSystemIcons = System.Drawing.SystemIcons;
using Application = System.Windows.Application;

namespace LanguageBubble;

public partial class App : Application
{
    private Mutex? _singleInstanceMutex;
    private KeyboardHook? _keyboardHook;
    private LanguageService? _languageService;
    private BubbleWindow? _bubbleWindow;
    private WinForms.NotifyIcon? _notifyIcon;
    private bool _isSwitching;

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        // Single-instance check
        _singleInstanceMutex = new Mutex(true, "Global\\LanguageBubble_SingleInstance", out bool createdNew);
        if (!createdNew)
        {
            WinForms.MessageBox.Show("Language Bubble is already running.", "Language Bubble",
                WinForms.MessageBoxButtons.OK, WinForms.MessageBoxIcon.Information);
            Shutdown();
            return;
        }

        // Create a hidden main window (needed for DPI-aware PresentationSource)
        MainWindow = new Window
        {
            Width = 0,
            Height = 0,
            WindowStyle = WindowStyle.None,
            ShowInTaskbar = false,
            ShowActivated = false,
            Visibility = Visibility.Hidden
        };

        // Initialize services
        _languageService = new LanguageService();
        _languageService.RefreshLayouts();

        // Create bubble window
        _bubbleWindow = new BubbleWindow();
        _bubbleWindow.SetSize(GetSavedBubbleSize());

        // Install keyboard hook
        _keyboardHook = new KeyboardHook();
        _keyboardHook.CapsLockPressed += OnCapsLockPressed;
        _keyboardHook.Install();

        // Force Caps Lock off on startup
        CapsLockService.EnsureCapsLockOff(_keyboardHook);

        // Setup system tray
        SetupTrayIcon();
    }

    private void SetupTrayIcon()
    {
        _notifyIcon = new WinForms.NotifyIcon
        {
            Text = "Language Bubble",
            Visible = true
        };

        // Load icon from embedded resource
        try
        {
            var iconPath = System.IO.Path.Combine(
                AppDomain.CurrentDomain.BaseDirectory, "Resources", "app.ico");
            if (System.IO.File.Exists(iconPath))
            {
                _notifyIcon.Icon = new DrawingIcon(iconPath);
            }
            else
            {
                _notifyIcon.Icon = DrawingSystemIcons.Application;
            }
        }
        catch
        {
            _notifyIcon.Icon = DrawingSystemIcons.Application;
        }

        RebuildContextMenu();
    }

    private void RebuildContextMenu()
    {
        if (_notifyIcon == null || _languageService == null)
            return;

        var menu = new WinForms.ContextMenuStrip();

        // Header
        var header = new WinForms.ToolStripLabel("Languages");
        header.Font = new DrawingFont(header.Font, DrawingFontStyle.Bold);
        menu.Items.Add(header);
        menu.Items.Add(new WinForms.ToolStripSeparator());

        // Language items
        var current = _languageService.GetCurrentLayout();
        foreach (var layout in _languageService.Layouts)
        {
            var item = new WinForms.ToolStripMenuItem(
                $"{layout.DisplayName} - {layout.Culture.EnglishName}");

            if (current != null && layout.Hkl == current.Hkl)
            {
                item.Checked = true;
            }

            menu.Items.Add(item);
        }

        menu.Items.Add(new WinForms.ToolStripSeparator());

        // Start with Windows toggle
        var startWithWindows = new WinForms.ToolStripMenuItem("Start with Windows");
        startWithWindows.Checked = IsStartWithWindowsEnabled();
        startWithWindows.Click += (_, _) =>
        {
            ToggleStartWithWindows(!startWithWindows.Checked);
            startWithWindows.Checked = IsStartWithWindowsEnabled();
        };
        menu.Items.Add(startWithWindows);

        // Size submenu
        var sizeMenu = new WinForms.ToolStripMenuItem("Size");
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
            var sizeItem = new WinForms.ToolStripMenuItem(label);
            sizeItem.Checked = value == currentSize;
            var captured = value;
            sizeItem.Click += (_, _) =>
            {
                _bubbleWindow?.SetSize(captured);
                SaveBubbleSize(captured);
                RebuildContextMenu();
            };
            sizeMenu.DropDownItems.Add(sizeItem);
        }
        menu.Items.Add(sizeMenu);

        // Slide animation toggle
        var slideAnimation = new WinForms.ToolStripMenuItem("Slide Animation");
        slideAnimation.Checked = _bubbleWindow?.UseSlideAnimation ?? true;
        slideAnimation.Click += (_, _) =>
        {
            if (_bubbleWindow != null)
            {
                _bubbleWindow.UseSlideAnimation = !_bubbleWindow.UseSlideAnimation;
                slideAnimation.Checked = _bubbleWindow.UseSlideAnimation;
            }
        };
        menu.Items.Add(slideAnimation);

        menu.Items.Add(new WinForms.ToolStripSeparator());

        // Exit
        var exitItem = new WinForms.ToolStripMenuItem("Exit");
        exitItem.Click += (_, _) => ExitApplication();
        menu.Items.Add(exitItem);

        _notifyIcon.ContextMenuStrip = menu;
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

                // Switch to the next language
                var newLayout = _languageService!.SwitchToNextLayout();
                if (newLayout == null)
                    return;

                // Get caret position (physical pixels)
                var caretPos = CaretPositionService.GetCaretScreenPosition();

                // Find selected index in layouts list
                var allLayouts = _languageService.Layouts;
                int selectedIndex = -1;
                for (int i = 0; i < allLayouts.Count; i++)
                {
                    if (allLayouts[i].Hkl == newLayout.Hkl)
                    {
                        selectedIndex = i;
                        break;
                    }
                }

                // Show the bubble with all languages
                _bubbleWindow!.ShowBubble(allLayouts, selectedIndex, caretPos);

                // Update tray menu
                RebuildContextMenu();
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

        if (_notifyIcon != null)
        {
            _notifyIcon.Visible = false;
            _notifyIcon.Dispose();
        }

        _singleInstanceMutex?.ReleaseMutex();
        _singleInstanceMutex?.Dispose();

        Shutdown();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _keyboardHook?.Dispose();

        if (_notifyIcon != null)
        {
            _notifyIcon.Visible = false;
            _notifyIcon.Dispose();
        }

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

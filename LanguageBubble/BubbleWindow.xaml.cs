using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using LanguageBubble.Native;
using LanguageBubble.Services;
using Microsoft.Win32;

namespace LanguageBubble;

public enum BubbleSize { ExtraSmall, Small, Medium, Large, ExtraLarge }
public enum DisplayMode { Carousel, Simple, Expanded }

public partial class BubbleWindow : Window
{
    private readonly DispatcherTimer _hideTimer;
    private readonly DispatcherTimer _topmostTimer;
    private Storyboard? _fadeOutStoryboard;

    private double _itemWidth = 32;
    private double _itemHeight = 24;
    private double _fontSize = 18;
    private int _previousSelectedIndex = -1;
    private int _layoutCount;
    private readonly List<TextBlock> _labels = new();
    private static readonly System.Windows.Media.FontFamily s_font = new("Segoe UI Semibold");
    private int _desiredPhysX, _desiredPhysY;
    private bool _hasPendingPosition;
    private Brush _foreground = Brushes.White;

    // Window slide animation state (for expanded mode)
    private DispatcherTimer? _slideTimer;
    private int _slideStartX, _slideTargetX;
    private DateTime _slideStartTime;
    private int _slideCurrentX;
    private bool _slideInProgress;
    private const double SlideDurationMs = 200;

    // Dark mode colors
    private static readonly Brush s_darkBackground = new SolidColorBrush(Color.FromArgb(0xDD, 0x2D, 0x2D, 0x2D));
    private static readonly Brush s_darkBorder = new SolidColorBrush(Color.FromArgb(0x44, 0xFF, 0xFF, 0xFF));

    // Light mode colors
    private static readonly Brush s_lightBackground = new SolidColorBrush(Color.FromArgb(0xDD, 0xF3, 0xF3, 0xF3));
    private static readonly Brush s_lightBorder = new SolidColorBrush(Color.FromArgb(0x44, 0x00, 0x00, 0x00));

    static BubbleWindow()
    {
        s_darkBackground.Freeze();
        s_darkBorder.Freeze();
        s_lightBackground.Freeze();
        s_lightBorder.Freeze();
    }

    public DisplayMode CurrentDisplayMode { get; set; } = DisplayMode.Carousel;
    public BubbleSize CurrentSize { get; private set; } = BubbleSize.Medium;

    public void SetSize(BubbleSize size)
    {
        CurrentSize = size;
        switch (size)
        {
            case BubbleSize.ExtraSmall:
                _itemWidth = 18; _itemHeight = 16; _fontSize = 11;
                OuterBorder.Padding = new Thickness(3, 3, 3, 3);
                OuterBorder.CornerRadius = new CornerRadius(5);
                break;
            case BubbleSize.Small:
                _itemWidth = 24; _itemHeight = 20; _fontSize = 14;
                OuterBorder.Padding = new Thickness(4, 4, 4, 4);
                OuterBorder.CornerRadius = new CornerRadius(6);
                break;
            case BubbleSize.Medium:
                _itemWidth = 30; _itemHeight = 24; _fontSize = 18;
                OuterBorder.Padding = new Thickness(6, 6, 6, 6);
                OuterBorder.CornerRadius = new CornerRadius(8);
                break;
            case BubbleSize.Large:
                _itemWidth = 40; _itemHeight = 32; _fontSize = 22;
                OuterBorder.Padding = new Thickness(8, 8, 8, 8);
                OuterBorder.CornerRadius = new CornerRadius(10);
                break;
            case BubbleSize.ExtraLarge:
                _itemWidth = 50; _itemHeight = 40; _fontSize = 28;
                OuterBorder.Padding = new Thickness(10, 10, 10, 10);
                OuterBorder.CornerRadius = new CornerRadius(12);
                break;
        }
        // Force label rebuild on next show
        _layoutCount = 0;
    }

    public BubbleWindow()
    {
        InitializeComponent();

        _hideTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(1.5)
        };
        _hideTimer.Tick += OnHideTimerTick;

        _topmostTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(100)
        };
        _topmostTimer.Tick += OnTopmostTimerTick;
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);

        var hwnd = new WindowInteropHelper(this).Handle;
        int exStyle = NativeMethods.GetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE);

        NativeMethods.SetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE,
            exStyle | NativeMethods.WS_EX_TRANSPARENT | NativeMethods.WS_EX_TOOLWINDOW);

        // Use DWM composited transparency instead of AllowsTransparency.
        // AllowsTransparency forces WPF software rendering (large RAM bitmaps);
        // DWM composition uses hardware acceleration with minimal memory.
        var source = HwndSource.FromHwnd(hwnd);
        if (source?.CompositionTarget != null)
            source.CompositionTarget.BackgroundColor = Colors.Transparent;

        var margins = new NativeMethods.MARGINS { Left = -1, Right = -1, Top = -1, Bottom = -1 };
        NativeMethods.DwmExtendFrameIntoClientArea(hwnd, ref margins);

        // Hook WndProc to intercept WM_DPICHANGED — WPF's default handler
        // repositions the window using a "suggested rect" that overrides our
        // SetWindowPos placement. We let WPF update its DPI state, then
        // immediately re-apply our desired physical position.
        source?.AddHook(WndProc);

        _fadeOutStoryboard = (Storyboard)FindResource("FadeOut");
        _fadeOutStoryboard.Completed += OnFadeOutCompleted;

        ApplyTheme();
    }

    private void ApplyTheme()
    {
        bool dark = IsWindowsDarkMode();
        OuterBorder.Background = dark ? s_darkBackground : s_lightBackground;
        OuterBorder.BorderBrush = dark ? s_darkBorder : s_lightBorder;
        _foreground = dark ? Brushes.White : Brushes.Black;

        foreach (var label in _labels)
            label.Foreground = _foreground;
    }

    private static bool IsWindowsDarkMode()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", false);
            var value = key?.GetValue("AppsUseLightTheme");
            return value is int i && i == 0;
        }
        catch { return true; }
    }

    internal void ShowBubble(IReadOnlyList<LayoutInfo> layouts, int selectedIndex,
        CaretPositionService.ScreenPoint? caretPhysical)
    {
        _hideTimer.Stop();
        _fadeOutStoryboard?.Stop(this);
        _slideTimer?.Stop();

        // Rebuild labels if layout count changed
        if (layouts.Count != _layoutCount)
        {
            BuildLabels(layouts);
        }

        // Clamp index
        if (selectedIndex < 0 || selectedIndex >= _labels.Count)
            selectedIndex = 0;

        bool canSlide = _previousSelectedIndex >= 0
            && _previousSelectedIndex != selectedIndex && _labels.Count > 1
            && caretPhysical.HasValue;

        if (CurrentDisplayMode == DisplayMode.Expanded && _labels.Count > 1)
        {
            // --- Expanded mode: all languages visible, window slides ---
            CarouselCanvas.Visibility = Visibility.Visible;

            double totalWidth = _labels.Count * _itemWidth;
            CarouselCanvas.Width = totalWidth;
            CarouselCanvas.Height = _itemHeight;
            Width = totalWidth + 16 + 1;

            // Row stays at origin — all items visible
            RowTranslate.BeginAnimation(TranslateTransform.XProperty, null);
            RowTranslate.X = 0;

            // Update label opacities
            for (int i = 0; i < _labels.Count; i++)
            {
                _labels[i].BeginAnimation(OpacityProperty, null);
                _labels[i].Opacity = (i == selectedIndex) ? 1.0 : 0.3;
            }

            Opacity = 0;
            Show();
            UpdateLayout();

            if (caretPhysical.HasValue)
                PositionAtCaretExpanded(caretPhysical.Value, selectedIndex);
            else
                CenterOnScreen();

            if (canSlide)
            {
                Opacity = 1;

                int targetX = _desiredPhysX;
                int startX;

                if (_slideInProgress)
                {
                    // Slide was interrupted — start from actual current position
                    startX = _slideCurrentX;
                }
                else
                {
                    // No slide in progress — compute from index delta
                    int indexDelta = selectedIndex - _previousSelectedIndex;
                    var hwnd2 = new WindowInteropHelper(this).Handle;
                    NativeMethods.GetWindowRect(hwnd2, out var rect);
                    int physW = rect.Right - rect.Left;
                    double dpiScale = physW / Width;
                    int offsetPhys = (int)(indexDelta * _itemWidth * dpiScale);
                    startX = targetX + offsetPhys;
                }

                // Move window to start position, then animate to target
                var hwnd = new WindowInteropHelper(this).Handle;
                SetPhysicalPosition(hwnd, startX, _desiredPhysY);
                AnimateWindowSlide(startX, targetX);

                // Animate label opacities
                if (_previousSelectedIndex >= 0 && _previousSelectedIndex < _labels.Count)
                {
                    var prevLabel = _labels[_previousSelectedIndex];
                    double curOp = prevLabel.Opacity;
                    prevLabel.BeginAnimation(OpacityProperty, null);
                    prevLabel.Opacity = curOp;
                    prevLabel.BeginAnimation(OpacityProperty,
                        new DoubleAnimation(curOp, 0.3, TimeSpan.FromMilliseconds(200)));
                }
                {
                    var newLabel = _labels[selectedIndex];
                    double curOp = newLabel.Opacity;
                    newLabel.BeginAnimation(OpacityProperty, null);
                    newLabel.Opacity = curOp;
                    newLabel.BeginAnimation(OpacityProperty,
                        new DoubleAnimation(curOp, 1.0, TimeSpan.FromMilliseconds(200)));
                }
            }
            else
            {
                var fadeIn = new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(150))
                {
                    EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                };
                BeginAnimation(OpacityProperty, fadeIn);
            }
        }
        else if (CurrentDisplayMode == DisplayMode.Carousel && _labels.Count > 1)
        {
            // --- Carousel mode ---
            CarouselCanvas.Visibility = Visibility.Visible;

            // Size the canvas to show one item width
            CarouselCanvas.Width = _itemWidth;
            CarouselCanvas.Height = _itemHeight;
            Width = _itemWidth + 16 + 1;

            // Show window first so we can animate
            Opacity = 0;
            Show();
            UpdateLayout();

            if (caretPhysical.HasValue)
                PositionAtCaret(caretPhysical.Value);
            else
                CenterOnScreen();

            // Update label opacities — clear leftover animations first
            for (int i = 0; i < _labels.Count; i++)
            {
                _labels[i].BeginAnimation(OpacityProperty, null);
                _labels[i].Opacity = (i == selectedIndex) ? 1.0 : 0.3;
            }

            double targetX = -selectedIndex * _itemWidth;

            if (canSlide)
            {
                // Already visible — just slide
                Opacity = 1;

                // Animate the row position from current to target
                double currentX = RowTranslate.X;
                RowTranslate.BeginAnimation(TranslateTransform.XProperty, null);
                RowTranslate.X = currentX;
                var slideAnim = new DoubleAnimation
                {
                    From = currentX,
                    To = targetX,
                    Duration = TimeSpan.FromMilliseconds(200),
                    EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                };
                RowTranslate.BeginAnimation(TranslateTransform.XProperty, slideAnim);

                // Animate old selected label dimming
                if (_previousSelectedIndex >= 0 && _previousSelectedIndex < _labels.Count)
                {
                    var prevLabel = _labels[_previousSelectedIndex];
                    double curOp = prevLabel.Opacity;
                    prevLabel.BeginAnimation(OpacityProperty, null);
                    prevLabel.Opacity = curOp;
                    prevLabel.BeginAnimation(OpacityProperty,
                        new DoubleAnimation(curOp, 0.3, TimeSpan.FromMilliseconds(200)));
                }

                // Animate new selected label brightening
                {
                    var newLabel = _labels[selectedIndex];
                    double curOp = newLabel.Opacity;
                    newLabel.BeginAnimation(OpacityProperty, null);
                    newLabel.Opacity = curOp;
                    newLabel.BeginAnimation(OpacityProperty,
                        new DoubleAnimation(curOp, 1.0, TimeSpan.FromMilliseconds(200)));
                }
            }
            else
            {
                // First show or no slide — snap to position and fade in
                RowTranslate.BeginAnimation(TranslateTransform.XProperty, null);
                RowTranslate.X = targetX;

                var fadeIn = new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(150))
                {
                    EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                };
                BeginAnimation(OpacityProperty, fadeIn);
            }
        }
        else
        {
            // --- Simple single-label mode ---
            CarouselCanvas.Visibility = Visibility.Visible;
            CarouselCanvas.Width = _itemWidth;
            CarouselCanvas.Height = _itemHeight;
            Width = _itemWidth + 28 + 1;

            // Show only selected label — clear any leftover animations first
            for (int i = 0; i < _labels.Count; i++)
            {
                _labels[i].BeginAnimation(OpacityProperty, null);
                _labels[i].Opacity = (i == selectedIndex) ? 1.0 : 0.0;
            }

            RowTranslate.BeginAnimation(TranslateTransform.XProperty, null);
            RowTranslate.X = -selectedIndex * _itemWidth;

            Opacity = 0;
            Show();
            UpdateLayout();

            if (caretPhysical.HasValue)
                PositionAtCaret(caretPhysical.Value);
            else
                CenterOnScreen();

            var fadeIn = new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(150))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            BeginAnimation(OpacityProperty, fadeIn);
        }

        _previousSelectedIndex = selectedIndex;
        _topmostTimer.Start();
        _hideTimer.Start();
    }

    private void BuildLabels(IReadOnlyList<LayoutInfo> layouts)
    {
        _labels.Clear();
        LanguageRow.Children.Clear();

        foreach (var layout in layouts)
        {
            var label = new TextBlock
            {
                Text = layout.BubbleText,
                Foreground = _foreground,
                FontSize = _fontSize,
                FontFamily = s_font,
                TextAlignment = TextAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center,
                Width = _itemWidth,
                Height = _itemHeight,
                LineHeight = _itemHeight,
                Opacity = 0.3
            };
            _labels.Add(label);
            LanguageRow.Children.Add(label);
        }

        _layoutCount = layouts.Count;
        _previousSelectedIndex = -1;
    }

    private void CenterOnScreen()
    {
        // Ensure PerMonitorV2 — COM calls in MSAA caret detection can corrupt
        // the thread's DPI awareness, causing wrong coordinate spaces.
        NativeMethods.SetThreadDpiAwarenessContext(
            NativeMethods.DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        var bubbleHwnd = new WindowInteropHelper(this).Handle;

        // Get bubble size in physical pixels
        NativeMethods.GetWindowRect(bubbleHwnd, out NativeMethods.RECT bubbleRect);
        int bubbleW = bubbleRect.Right - bubbleRect.Left;
        int bubbleH = bubbleRect.Bottom - bubbleRect.Top;

        // Find the monitor the user is working on
        IntPtr fgHwnd = NativeMethods.GetForegroundWindow();
        IntPtr hMonitor;

        if (fgHwnd != IntPtr.Zero)
        {
            hMonitor = NativeMethods.MonitorFromWindow(
                fgHwnd, NativeMethods.MONITOR_DEFAULTTONEAREST);
        }
        else
        {
            NativeMethods.GetCursorPos(out NativeMethods.POINT cursorPt);
            hMonitor = NativeMethods.MonitorFromPoint(
                cursorPt, NativeMethods.MONITOR_DEFAULTTONEAREST);
        }

        var mi = new NativeMethods.MONITORINFO();
        mi.cbSize = Marshal.SizeOf<NativeMethods.MONITORINFO>();
        NativeMethods.GetMonitorInfo(hMonitor, ref mi);

        // Center within work area — all in physical pixels
        int workW = mi.rcWork.Right - mi.rcWork.Left;
        int workH = mi.rcWork.Bottom - mi.rcWork.Top;
        int x = mi.rcWork.Left + (workW - bubbleW) / 2;
        int y = mi.rcWork.Top + (workH - bubbleH) / 2;

        SetPhysicalPosition(bubbleHwnd, x, y);
    }

    private void PositionAtCaret(CaretPositionService.ScreenPoint physicalPt)
    {
        // Ensure PerMonitorV2 — COM calls in MSAA caret detection can corrupt
        // the thread's DPI awareness, causing wrong coordinate spaces.
        NativeMethods.SetThreadDpiAwarenessContext(
            NativeMethods.DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        var bubbleHwnd = new WindowInteropHelper(this).Handle;

        // Get bubble size in physical pixels
        NativeMethods.GetWindowRect(bubbleHwnd, out NativeMethods.RECT bubbleRect);
        int bubbleW = bubbleRect.Right - bubbleRect.Left;
        int bubbleH = bubbleRect.Bottom - bubbleRect.Top;

        System.Diagnostics.Debug.WriteLine(
            $"[Bubble] Caret=({physicalPt.X},{physicalPt.Y}) BubbleSize=({bubbleW}x{bubbleH})");

        // Position in physical pixels: centered on caret, just below
        int x = physicalPt.X - bubbleW / 2;
        int y = physicalPt.Y + 4;

        // Get work area of the monitor containing the caret
        var caretPt = new NativeMethods.POINT { X = physicalPt.X, Y = physicalPt.Y };
        IntPtr hMonitor = NativeMethods.MonitorFromPoint(
            caretPt, NativeMethods.MONITOR_DEFAULTTONEAREST);
        var mi = new NativeMethods.MONITORINFO();
        mi.cbSize = Marshal.SizeOf<NativeMethods.MONITORINFO>();
        NativeMethods.GetMonitorInfo(hMonitor, ref mi);

        // Clamp within the correct monitor's work area (all physical pixels)
        const int margin = 10;
        if (x + bubbleW > mi.rcWork.Right - margin)
            x = mi.rcWork.Right - bubbleW - margin;
        if (x < mi.rcWork.Left + margin)
            x = mi.rcWork.Left + margin;
        if (y + bubbleH > mi.rcWork.Bottom - margin)
            y = physicalPt.Y - bubbleH - 4; // flip above caret
        if (y < mi.rcWork.Top + margin)
            y = mi.rcWork.Top + margin;

        System.Diagnostics.Debug.WriteLine(
            $"[Bubble] Final position=({x},{y}) Monitor=({mi.rcWork.Left},{mi.rcWork.Top})-({mi.rcWork.Right},{mi.rcWork.Bottom})");

        SetPhysicalPosition(bubbleHwnd, x, y);
    }

    private void PositionAtCaretExpanded(CaretPositionService.ScreenPoint physicalPt, int selectedIndex)
    {
        NativeMethods.SetThreadDpiAwarenessContext(
            NativeMethods.DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        var bubbleHwnd = new WindowInteropHelper(this).Handle;

        NativeMethods.GetWindowRect(bubbleHwnd, out NativeMethods.RECT bubbleRect);
        int bubbleW = bubbleRect.Right - bubbleRect.Left;
        int bubbleH = bubbleRect.Bottom - bubbleRect.Top;

        // DPI scale: physical pixels per WPF DIP
        double dpiScale = bubbleW / Width;

        // Compute physical offset from window left edge to selected item center
        double paddingLeft = OuterBorder.Padding.Left;
        double selectedCenterDip = paddingLeft + selectedIndex * _itemWidth + _itemWidth / 2.0;
        int selectedCenterPhys = (int)(selectedCenterDip * dpiScale);

        // Position so selected item center is under the caret
        int x = physicalPt.X - selectedCenterPhys;
        int y = physicalPt.Y + 4;

        // Monitor clamping
        var caretPt = new NativeMethods.POINT { X = physicalPt.X, Y = physicalPt.Y };
        IntPtr hMonitor = NativeMethods.MonitorFromPoint(
            caretPt, NativeMethods.MONITOR_DEFAULTTONEAREST);
        var mi = new NativeMethods.MONITORINFO();
        mi.cbSize = Marshal.SizeOf<NativeMethods.MONITORINFO>();
        NativeMethods.GetMonitorInfo(hMonitor, ref mi);

        const int margin = 10;
        if (x + bubbleW > mi.rcWork.Right - margin)
            x = mi.rcWork.Right - bubbleW - margin;
        if (x < mi.rcWork.Left + margin)
            x = mi.rcWork.Left + margin;
        if (y + bubbleH > mi.rcWork.Bottom - margin)
            y = physicalPt.Y - bubbleH - 4;
        if (y < mi.rcWork.Top + margin)
            y = mi.rcWork.Top + margin;

        SetPhysicalPosition(bubbleHwnd, x, y);
    }

    private void AnimateWindowSlide(int fromX, int toX)
    {
        _slideInProgress = true;
        _slideCurrentX = fromX;
        _slideStartX = fromX;
        _slideTargetX = toX;
        _slideStartTime = DateTime.UtcNow;

        if (_slideTimer == null)
        {
            _slideTimer = new DispatcherTimer(DispatcherPriority.Send) { Interval = TimeSpan.FromMilliseconds(8) };
            _slideTimer.Tick += OnSlideTimerTick;
        }
        _slideTimer.Start();
    }

    private void OnSlideTimerTick(object? sender, EventArgs e)
    {
        double elapsed = (DateTime.UtcNow - _slideStartTime).TotalMilliseconds;
        double t = Math.Min(elapsed / SlideDurationMs, 1.0);

        // Cubic ease-out
        double eased = 1 - Math.Pow(1 - t, 3);

        int x = _slideStartX + (int)((_slideTargetX - _slideStartX) * eased);
        _slideCurrentX = x;

        var hwnd = new WindowInteropHelper(this).Handle;
        SetPhysicalPosition(hwnd, x, _desiredPhysY);

        if (t >= 1.0)
        {
            _slideInProgress = false;
            _slideTimer!.Stop();
        }
    }

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        const int WM_SETTINGCHANGE = 0x001A;
        if (msg == WM_SETTINGCHANGE)
        {
            var param = lParam != IntPtr.Zero ? Marshal.PtrToStringUni(lParam) : null;
            if (param == "ImmersiveColorSet")
                ApplyTheme();
        }

        const int WM_DPICHANGED = 0x02E0;
        if (msg == WM_DPICHANGED && _hasPendingPosition)
        {
            // WPF's default WM_DPICHANGED handler calls SetWindowPos with the
            // OS-suggested rect (lParam), overriding our placement. This hook runs
            // BEFORE WPF's handler, so we modify the suggested rect in-place to
            // keep our desired position while accepting the new DPI-scaled size.
            var suggested = Marshal.PtrToStructure<NativeMethods.RECT>(lParam);
            int newW = suggested.Right - suggested.Left;
            int newH = suggested.Bottom - suggested.Top;

            var corrected = new NativeMethods.RECT
            {
                Left = _desiredPhysX,
                Top = _desiredPhysY,
                Right = _desiredPhysX + newW,
                Bottom = _desiredPhysY + newH
            };
            Marshal.StructureToPtr(corrected, lParam, false);
            _hasPendingPosition = false;
        }
        return IntPtr.Zero;
    }

    private void SetPhysicalPosition(IntPtr hwnd, int x, int y)
    {
        _desiredPhysX = x;
        _desiredPhysY = y;
        _hasPendingPosition = true;
        NativeMethods.SetWindowPos(hwnd, NativeMethods.HWND_TOPMOST, x, y, 0, 0,
            NativeMethods.SWP_NOSIZE | NativeMethods.SWP_NOACTIVATE);
    }

    internal void InstantHide()
    {
        if (!IsVisible)
            return;

        _hideTimer.Stop();
        _topmostTimer.Stop();
        _fadeOutStoryboard?.Stop(this);
        BeginAnimation(OpacityProperty, null);
        Opacity = 0;
        Hide();
    }

    private void OnHideTimerTick(object? sender, EventArgs e)
    {
        _hideTimer.Stop();
        _fadeOutStoryboard?.Begin(this, true);
    }

    private void OnTopmostTimerTick(object? sender, EventArgs e)
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        if (hwnd != IntPtr.Zero)
        {
            NativeMethods.SetWindowPos(hwnd, NativeMethods.HWND_TOPMOST, 0, 0, 0, 0,
                NativeMethods.SWP_NOMOVE | NativeMethods.SWP_NOSIZE | NativeMethods.SWP_NOACTIVATE);
        }
    }

    private void OnFadeOutCompleted(object? sender, EventArgs e)
    {
        _topmostTimer.Stop();
        Hide();

        // Return memory to the OS — .NET GC holds committed pages by default.
        // A gen-2 collect after the bubble hides reclaims animation objects,
        // COM wrappers from caret detection, and other transient allocations.
        GC.Collect(2, GCCollectionMode.Forced, false, true);
    }
}

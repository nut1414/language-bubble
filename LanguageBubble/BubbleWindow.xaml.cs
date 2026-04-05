using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using LanguageBubble.Native;
using LanguageBubble.Services;

namespace LanguageBubble;

public enum BubbleSize { ExtraSmall, Small, Medium, Large, ExtraLarge }

public partial class BubbleWindow : Window
{
    private readonly DispatcherTimer _hideTimer;
    private Storyboard? _fadeOutStoryboard;

    private double _itemWidth = 32;
    private double _itemHeight = 24;
    private double _fontSize = 18;
    private int _previousSelectedIndex = -1;
    private int _layoutCount;
    private readonly List<TextBlock> _labels = new();
    private int _desiredPhysX, _desiredPhysY;
    private bool _hasPendingPosition;

    public bool UseSlideAnimation { get; set; } = true;
    public BubbleSize CurrentSize { get; private set; } = BubbleSize.Medium;

    public void SetSize(BubbleSize size)
    {
        CurrentSize = size;
        switch (size)
        {
            case BubbleSize.ExtraSmall:
                _itemWidth = 20; _itemHeight = 16; _fontSize = 11;
                OuterBorder.Padding = new Thickness(3, 3, 3, 3);
                OuterBorder.CornerRadius = new CornerRadius(5);
                break;
            case BubbleSize.Small:
                _itemWidth = 26; _itemHeight = 20; _fontSize = 14;
                OuterBorder.Padding = new Thickness(4, 4, 4, 4);
                OuterBorder.CornerRadius = new CornerRadius(6);
                break;
            case BubbleSize.Medium:
                _itemWidth = 32; _itemHeight = 24; _fontSize = 18;
                OuterBorder.Padding = new Thickness(6, 6, 6, 6);
                OuterBorder.CornerRadius = new CornerRadius(8);
                break;
            case BubbleSize.Large:
                _itemWidth = 42; _itemHeight = 32; _fontSize = 22;
                OuterBorder.Padding = new Thickness(8, 8, 8, 8);
                OuterBorder.CornerRadius = new CornerRadius(10);
                break;
            case BubbleSize.ExtraLarge:
                _itemWidth = 52; _itemHeight = 40; _fontSize = 28;
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
    }

    protected override void OnSourceInitialized(EventArgs e)
    {
        base.OnSourceInitialized(e);

        var hwnd = new WindowInteropHelper(this).Handle;
        int exStyle = NativeMethods.GetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE);

        NativeMethods.SetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE,
            exStyle | NativeMethods.WS_EX_TRANSPARENT | NativeMethods.WS_EX_TOOLWINDOW);

        // Hook WndProc to intercept WM_DPICHANGED — WPF's default handler
        // repositions the window using a "suggested rect" that overrides our
        // SetWindowPos placement. We let WPF update its DPI state, then
        // immediately re-apply our desired physical position.
        var source = HwndSource.FromHwnd(hwnd);
        source?.AddHook(WndProc);

        _fadeOutStoryboard = (Storyboard)FindResource("FadeOut");
        _fadeOutStoryboard.Completed += OnFadeOutCompleted;
    }

    internal void ShowBubble(IReadOnlyList<LayoutInfo> layouts, int selectedIndex,
        CaretPositionService.ScreenPoint? caretPhysical)
    {
        _hideTimer.Stop();
        _fadeOutStoryboard?.Stop(this);

        // Rebuild labels if layout count changed
        if (layouts.Count != _layoutCount)
        {
            BuildLabels(layouts);
        }

        // Clamp index
        if (selectedIndex < 0 || selectedIndex >= _labels.Count)
            selectedIndex = 0;

        bool shouldSlide = UseSlideAnimation && _previousSelectedIndex >= 0
            && _previousSelectedIndex != selectedIndex && _labels.Count > 1;

        if (UseSlideAnimation && _labels.Count > 1)
        {
            // --- Carousel mode ---
            CarouselCanvas.Visibility = Visibility.Visible;

            // Size the canvas to show one item width
            CarouselCanvas.Width = _itemWidth;
            CarouselCanvas.Height = _itemHeight;
            Width = _itemWidth + 16 + 1; // 16 = padding (8*2), 1 = border

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

            if (shouldSlide)
            {
                // Already visible — just slide
                Opacity = 1;

                // Animate the row position
                var slideAnim = new DoubleAnimation
                {
                    To = targetX,
                    Duration = TimeSpan.FromMilliseconds(200),
                    EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
                };
                RowTranslate.BeginAnimation(TranslateTransform.XProperty, slideAnim);

                // Animate old selected label dimming
                if (_previousSelectedIndex >= 0 && _previousSelectedIndex < _labels.Count)
                {
                    var dimAnim = new DoubleAnimation(0.3, TimeSpan.FromMilliseconds(200));
                    _labels[_previousSelectedIndex].BeginAnimation(OpacityProperty, dimAnim);
                }

                // Animate new selected label brightening
                var brightAnim = new DoubleAnimation(1.0, TimeSpan.FromMilliseconds(200));
                _labels[selectedIndex].BeginAnimation(OpacityProperty, brightAnim);
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
                Foreground = System.Windows.Media.Brushes.White,
                FontSize = _fontSize,
                FontFamily = new System.Windows.Media.FontFamily("Segoe UI Semibold"),
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

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
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
        NativeMethods.SetWindowPos(hwnd, IntPtr.Zero, x, y, 0, 0,
            NativeMethods.SWP_NOSIZE | NativeMethods.SWP_NOZORDER | NativeMethods.SWP_NOACTIVATE);
    }

    private void OnHideTimerTick(object? sender, EventArgs e)
    {
        _hideTimer.Stop();
        _fadeOutStoryboard?.Begin(this, true);
    }

    private void OnFadeOutCompleted(object? sender, EventArgs e)
    {
        Hide();
    }
}

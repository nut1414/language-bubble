using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using LanguageBubble.Native;
using LanguageBubble.Services;

namespace LanguageBubble;

public partial class BubbleWindow : Window
{
    private readonly DispatcherTimer _hideTimer;
    private Storyboard? _fadeInStoryboard;
    private Storyboard? _fadeOutStoryboard;

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

        // Make click-through and hide from Alt+Tab
        NativeMethods.SetWindowLong(hwnd, NativeMethods.GWL_EXSTYLE,
            exStyle | NativeMethods.WS_EX_TRANSPARENT | NativeMethods.WS_EX_TOOLWINDOW);

        _fadeInStoryboard = (Storyboard)FindResource("FadeIn");
        _fadeOutStoryboard = (Storyboard)FindResource("FadeOut");
        _fadeOutStoryboard.Completed += OnFadeOutCompleted;
    }

    internal void ShowBubble(string text, CaretPositionService.ScreenPoint? caretPhysical)
    {
        // Stop any running timer/animation
        _hideTimer.Stop();
        _fadeOutStoryboard?.Stop(this);

        LanguageLabel.Text = text;

        // Show first (invisible) so we can measure AND get a valid PresentationSource
        Opacity = 0;
        Show();
        UpdateLayout();

        // Position the bubble
        if (caretPhysical.HasValue)
        {
            PositionAtCaret(caretPhysical.Value);
        }
        else
        {
            // Fallback: center of screen
            Left = (SystemParameters.PrimaryScreenWidth - ActualWidth) / 2;
            Top = (SystemParameters.PrimaryScreenHeight - ActualHeight) / 2;
        }

        // Animate in
        _fadeInStoryboard?.Begin(this, true);

        // Start auto-hide timer
        _hideTimer.Start();
    }

    private void PositionAtCaret(CaretPositionService.ScreenPoint physicalPt)
    {
        // Convert physical screen pixels to WPF DIPs using THIS window's PresentationSource
        // (this works because the window is already shown at this point)
        double dipX = physicalPt.X;
        double dipY = physicalPt.Y;

        var source = PresentationSource.FromVisual(this);
        if (source?.CompositionTarget != null)
        {
            var transform = source.CompositionTarget.TransformFromDevice;
            var dip = transform.Transform(new System.Windows.Point(physicalPt.X, physicalPt.Y));
            dipX = dip.X;
            dipY = dip.Y;
        }

        // Center bubble horizontally on the caret, position below
        Left = dipX - (ActualWidth / 2);
        Top = dipY + 4;

        // Clamp to screen bounds
        double sw = SystemParameters.PrimaryScreenWidth;
        double sh = SystemParameters.PrimaryScreenHeight;

        if (Left + ActualWidth > sw - 10)
            Left = sw - ActualWidth - 10;
        if (Left < 10)
            Left = 10;
        if (Top + ActualHeight > sh - 50) // leave room for taskbar
            Top = dipY - ActualHeight - 4; // flip above caret
        if (Top < 10)
            Top = 10;
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

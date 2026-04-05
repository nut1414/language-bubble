using System.Windows;
using System.Windows.Controls;
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
    private Storyboard? _fadeOutStoryboard;

    private const double ItemWidth = 40;
    private const double ItemHeight = 24;
    private int _previousSelectedIndex = -1;
    private int _layoutCount;
    private readonly List<TextBlock> _labels = new();

    public bool UseSlideAnimation { get; set; } = true;

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
            CarouselCanvas.Width = ItemWidth;
            CarouselCanvas.Height = ItemHeight;
            Width = ItemWidth + 28 + 1; // 28 = padding (14*2), 1 = border

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

            double targetX = -selectedIndex * ItemWidth;

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
            CarouselCanvas.Width = ItemWidth;
            CarouselCanvas.Height = ItemHeight;
            Width = ItemWidth + 28 + 1;

            // Show only selected label — clear any leftover animations first
            for (int i = 0; i < _labels.Count; i++)
            {
                _labels[i].BeginAnimation(OpacityProperty, null);
                _labels[i].Opacity = (i == selectedIndex) ? 1.0 : 0.0;
            }

            RowTranslate.BeginAnimation(TranslateTransform.XProperty, null);
            RowTranslate.X = -selectedIndex * ItemWidth;

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
                FontSize = 18,
                FontFamily = new System.Windows.Media.FontFamily("Segoe UI Semibold"),
                TextAlignment = TextAlignment.Center,
                VerticalAlignment = VerticalAlignment.Center,
                Width = ItemWidth,
                Height = ItemHeight,
                LineHeight = ItemHeight,
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
        Left = (SystemParameters.PrimaryScreenWidth - ActualWidth) / 2;
        Top = (SystemParameters.PrimaryScreenHeight - ActualHeight) / 2;
    }

    private void PositionAtCaret(CaretPositionService.ScreenPoint physicalPt)
    {
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

        Left = dipX - (ActualWidth / 2);
        Top = dipY + 4;

        double sw = SystemParameters.PrimaryScreenWidth;
        double sh = SystemParameters.PrimaryScreenHeight;

        if (Left + ActualWidth > sw - 10)
            Left = sw - ActualWidth - 10;
        if (Left < 10)
            Left = 10;
        if (Top + ActualHeight > sh - 50)
            Top = dipY - ActualHeight - 4;
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

using System.Globalization;
using LanguageBubble.Native;

namespace LanguageBubble.Services;

internal sealed class LayoutInfo
{
    public IntPtr Hkl { get; init; }
    public CultureInfo Culture { get; init; } = CultureInfo.InvariantCulture;
    public string DisplayName => Culture.TwoLetterISOLanguageName.ToUpperInvariant();
    public string NativeDisplayName => Culture.NativeName;

    public string BubbleText
    {
        get
        {
            // For CJK and Thai languages, show native name (shorter form)
            string twoLetter = Culture.TwoLetterISOLanguageName;
            return twoLetter switch
            {
                "en" => "A",    // Show "EN" instead of "US" for English
                "ja" => "\u3042",    // Hiragana 'a' - recognizable Japanese symbol
                "zh" => "\u4e2d",    // Chinese character for 'middle/China'
                "ko" => "\uac00",    // Korean character 'ga'
                "th" => "\u0e01",    // Thai character 'ko kai'
                _ => twoLetter.ToUpperInvariant()
            };
        }
    }
}

internal sealed class LanguageService
{
    private List<LayoutInfo> _layouts = new();

    public IReadOnlyList<LayoutInfo> Layouts => _layouts;

    public void RefreshLayouts()
    {
        int count = NativeMethods.GetKeyboardLayoutList(0, null);
        if (count <= 0)
        {
            _layouts = new List<LayoutInfo>();
            return;
        }

        var hkls = new IntPtr[count];
        NativeMethods.GetKeyboardLayoutList(count, hkls);

        _layouts = hkls.Select(hkl =>
        {
            int langId = (int)hkl & 0xFFFF;
            CultureInfo culture;
            try
            {
                culture = new CultureInfo(langId);
            }
            catch
            {
                culture = CultureInfo.InvariantCulture;
            }

            return new LayoutInfo
            {
                Hkl = hkl,
                Culture = culture
            };
        }).ToList();
    }

    public LayoutInfo? GetCurrentLayout()
    {
        IntPtr hwnd = NativeMethods.GetForegroundWindow();
        if (hwnd == IntPtr.Zero)
            return _layouts.FirstOrDefault();

        uint threadId = NativeMethods.GetWindowThreadProcessId(hwnd, out _);
        IntPtr currentHkl = NativeMethods.GetKeyboardLayout(threadId);

        return _layouts.FirstOrDefault(l => l.Hkl == currentHkl)
            ?? CreateLayoutInfoFromHkl(currentHkl);
    }

    public LayoutInfo? SwitchToNextLayout()
    {
        if (_layouts.Count <= 1)
            return _layouts.FirstOrDefault();

        var current = GetCurrentLayout();
        int currentIndex = current != null
            ? _layouts.FindIndex(l => l.Hkl == current.Hkl)
            : -1;

        int nextIndex = (currentIndex + 1) % _layouts.Count;
        var target = _layouts[nextIndex];

        IntPtr hwnd = NativeMethods.GetForegroundWindow();

        // 1. Send to the foreground window (works for apps with text input)
        if (hwnd != IntPtr.Zero)
        {
            NativeMethods.PostMessage(hwnd, NativeMethods.WM_INPUTLANGCHANGEREQUEST, IntPtr.Zero, target.Hkl);
        }

        // 2. Broadcast to all top-level windows (catches system UI, Start Menu, etc.)
        NativeMethods.PostMessage(NativeMethods.HWND_BROADCAST, NativeMethods.WM_INPUTLANGCHANGEREQUEST, IntPtr.Zero, target.Hkl);

        // 3. Activate for the current process (ensures subsequent windows use it)
        NativeMethods.ActivateKeyboardLayout(target.Hkl, NativeMethods.KLF_SETFORPROCESS);

        return target;
    }

    private static LayoutInfo CreateLayoutInfoFromHkl(IntPtr hkl)
    {
        int langId = (int)hkl & 0xFFFF;
        CultureInfo culture;
        try
        {
            culture = new CultureInfo(langId);
        }
        catch
        {
            culture = CultureInfo.InvariantCulture;
        }

        return new LayoutInfo
        {
            Hkl = hkl,
            Culture = culture
        };
    }
}

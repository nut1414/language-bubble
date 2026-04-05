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
    private IntPtr _lastNonEnglishHkl = IntPtr.Zero;

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

    public void RecordLayoutUsage(IntPtr hkl)
    {
        if (hkl == IntPtr.Zero)
            return;
        var layout = _layouts.FirstOrDefault(l => l.Hkl == hkl);
        if (layout != null && layout.Culture.TwoLetterISOLanguageName != "en")
            _lastNonEnglishHkl = hkl;
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
        ActivateLayout(target);
        return target;
    }

    public LayoutInfo? SwitchToMruLayout()
    {
        if (_layouts.Count <= 1)
            return _layouts.FirstOrDefault();

        var current = GetCurrentLayout();
        bool currentIsEnglish = current != null
            && current.Culture.TwoLetterISOLanguageName == "en";

        if (currentIsEnglish)
        {
            // Switch to the last non-English language
            if (_lastNonEnglishHkl != IntPtr.Zero)
            {
                var target = _layouts.FirstOrDefault(l => l.Hkl == _lastNonEnglishHkl);
                if (target != null)
                {
                    ActivateLayout(target);
                    return target;
                }
            }
            // No non-English history yet — pick the first non-English layout
            var fallback = _layouts.FirstOrDefault(l => l.Culture.TwoLetterISOLanguageName != "en");
            if (fallback != null)
            {
                ActivateLayout(fallback);
                return fallback;
            }
        }
        else
        {
            // Current is non-English — always switch back to English
            var english = _layouts.FirstOrDefault(l => l.Culture.TwoLetterISOLanguageName == "en");
            if (english != null)
            {
                ActivateLayout(english);
                return english;
            }
        }

        return SwitchToNextLayout();
    }

    private void ActivateLayout(LayoutInfo target)
    {
        IntPtr hwnd = NativeMethods.GetForegroundWindow();

        if (hwnd != IntPtr.Zero)
        {
            NativeMethods.PostMessage(hwnd, NativeMethods.WM_INPUTLANGCHANGEREQUEST, IntPtr.Zero, target.Hkl);
        }

        NativeMethods.PostMessage(NativeMethods.HWND_BROADCAST, NativeMethods.WM_INPUTLANGCHANGEREQUEST, IntPtr.Zero, target.Hkl);
        NativeMethods.ActivateKeyboardLayout(target.Hkl, NativeMethods.KLF_SETFORPROCESS);
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

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
            string twoLetter = Culture.TwoLetterISOLanguageName;
            return twoLetter switch
            {
                // Latin — special case
                "en" => "A",

                // CJK
                "ja" => "\u3042",    // あ  Hiragana 'a'
                "zh" => "\u4e2d",    // 中  'middle/China'
                "ko" => "\uac00",    // 가  'ga'

                // Southeast Asian
                "th" => "\u0e01",    // ก   Thai 'ko kai'
                "km" => "\u1780",    // ក   Khmer 'ka'
                "lo" => "\u0ea5",    // ລ   Lao 'lo'
                "my" => "\u1000",    // က   Myanmar 'ka'

                // South Asian — Devanagari
                "hi" => "\u0905",    // अ   Devanagari 'a' (Hindi)
                "mr" => "\u092e",    // म   Devanagari 'ma' (Marathi)
                "ne" => "\u0928",    // न   Devanagari 'na' (Nepali)
                "sa" => "\u0938",    // स   Devanagari 'sa' (Sanskrit)

                // South Asian — other scripts
                "bn" => "\u0985",    // অ   Bengali 'a'
                "as" => "\u0985",    // অ   Assamese 'a' (shared Bengali script)
                "gu" => "\u0a97",    // ગ   Gujarati 'ga'
                "pa" => "\u0a2a",    // ਪ   Gurmukhi 'pa' (Punjabi)
                "ta" => "\u0ba4",    // த   Tamil 'ta'
                "te" => "\u0c24",    // త   Telugu 'ta'
                "kn" => "\u0c95",    // ಕ   Kannada 'ka'
                "ml" => "\u0d2e",    // മ   Malayalam 'ma'
                "si" => "\u0dc3",    // ස   Sinhala 'sa'
                "or" => "\u0b13",    // ଓ   Odia 'o'

                // Urdu / Arabic script
                "ur" => "\u0627",    // ا   Urdu 'alif'
                "ar" => "\u0639",    // ع   Arabic 'ain'
                "fa" => "\u0641",    // ف   Persian/Farsi 'fe'
                "ps" => "\u067e",    // پ   Pashto 'pe'
                "ug" => "\u0626",    // ئ   Uyghur 'hamza ye'
                "sd" => "\u0633",    // س   Sindhi 'seen'
                "ku" => "\u06a9",    // ک   Kurdish 'keheh'

                // Hebrew
                "he" => "\u05d0",    // א   Hebrew 'alef'
                "yi" => "\u05d9",    // י   Yiddish 'yod'

                // Cyrillic
                "ru" => "\u0410",    // А   Cyrillic 'A' (Russian)
                "uk" => "\u0423",    // У   Cyrillic 'U' (Ukrainian)
                "bg" => "\u0411",    // Б   Cyrillic 'B' (Bulgarian)
                "sr" => "\u0421",    // С   Cyrillic 'S' (Serbian)
                "mk" => "\u041c",    // М   Cyrillic 'M' (Macedonian)
                "kk" => "\u049a",    // Қ   Cyrillic 'Qa' (Kazakh)
                "ky" => "\u041a",    // К   Cyrillic 'K' (Kyrgyz)
                "mn" => "\u041c",    // М   Cyrillic 'M' (Mongolian)
                "tg" => "\u0422",    // Т   Cyrillic 'T' (Tajik)

                // Greek
                "el" => "\u0391",    // Α   Greek 'Alpha'

                // Caucasian
                "ka" => "\u10d0",    // ა   Georgian 'ani'
                "hy" => "\u0531",    // Ա   Armenian 'ayb'

                // Tibetan
                "bo" => "\u0f56",    // བ   Tibetan 'ba'

                // Ethiopic
                "am" => "\u12a0",    // አ   Amharic/Ethiopic 'a'
                "ti" => "\u1275",    // ት   Tigrinya 'ti'

                // Canadian Indigenous
                "iu" => "\u1403",    // ᐃ   Inuktitut syllabic 'i'
                "cr" => "\u1431",    // ᐱ   Cree syllabic 'pi'

                // Cherokee
                "chr" => "\u13e3",   // Ꮳ   Cherokee 'tsa'

                // All Latin-script languages fall through (FR, DE, ES, PT, etc.)
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

    public IReadOnlyList<LayoutInfo> GetMruLayouts()
    {
        var result = new List<LayoutInfo>();
        var english = _layouts.FirstOrDefault(l => l.Culture.TwoLetterISOLanguageName == "en");
        if (english != null)
            result.Add(english);

        LayoutInfo? nonEnglish = null;
        if (_lastNonEnglishHkl != IntPtr.Zero)
            nonEnglish = _layouts.FirstOrDefault(l => l.Hkl == _lastNonEnglishHkl);
        nonEnglish ??= _layouts.FirstOrDefault(l => l.Culture.TwoLetterISOLanguageName != "en");

        if (nonEnglish != null)
            result.Add(nonEnglish);

        return result.Count > 0 ? result : _layouts;
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

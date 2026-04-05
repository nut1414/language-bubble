using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows.Automation;
using System.Windows.Automation.Text;
using LanguageBubble.Native;

namespace LanguageBubble.Services;

/// <summary>
/// Returns caret position in PHYSICAL SCREEN PIXELS (not DIPs).
/// The caller (BubbleWindow) handles DPI conversion.
/// </summary>
internal static class CaretPositionService
{
    internal struct ScreenPoint
    {
        public int X;
        public int Y;
    }

    public static ScreenPoint? GetCaretScreenPosition()
    {
        // Strategy 1: Win32 GetGUIThreadInfo (Notepad, WordPad, classic Win32 apps)
        var result = TryGetGUIThreadInfo();
        if (result.HasValue)
        {
            Debug.WriteLine($"[Caret] GetGUIThreadInfo: ({result.Value.X}, {result.Value.Y})");
            return result;
        }

        // Strategy 2: MSAA IAccessible OBJID_CARET (Chrome, many apps)
        result = TryAccessibilityCaret();
        if (result.HasValue)
        {
            Debug.WriteLine($"[Caret] MSAA: ({result.Value.X}, {result.Value.Y})");
            return result;
        }

        // Strategy 3: UI Automation TextPattern (Office, Edge)
        result = TryUIAutomation();
        if (result.HasValue)
        {
            Debug.WriteLine($"[Caret] UIAutomation: ({result.Value.X}, {result.Value.Y})");
            return result;
        }

        Debug.WriteLine("[Caret] All strategies failed");
        return null;
    }

    private static ScreenPoint? TryGetGUIThreadInfo()
    {
        try
        {
            IntPtr hwnd = NativeMethods.GetForegroundWindow();
            if (hwnd == IntPtr.Zero) return null;

            uint threadId = NativeMethods.GetWindowThreadProcessId(hwnd, out _);

            var gui = new NativeMethods.GUITHREADINFO();
            gui.cbSize = Marshal.SizeOf<NativeMethods.GUITHREADINFO>();

            if (!NativeMethods.GetGUIThreadInfo(threadId, ref gui))
                return null;

            if (gui.hwndCaret == IntPtr.Zero)
                return null;

            int w = gui.rcCaret.Right - gui.rcCaret.Left;
            int h = gui.rcCaret.Bottom - gui.rcCaret.Top;
            if (w <= 0 && h <= 0) return null;

            var pt = new NativeMethods.POINT { X = gui.rcCaret.Left, Y = gui.rcCaret.Bottom };
            if (!NativeMethods.ClientToScreen(gui.hwndCaret, ref pt))
                return null;

            return new ScreenPoint { X = pt.X, Y = pt.Y };
        }
        catch { return null; }
    }

    private static ScreenPoint? TryAccessibilityCaret()
    {
        try
        {
            IntPtr hwnd = NativeMethods.GetForegroundWindow();
            if (hwnd == IntPtr.Zero) return null;

            uint threadId = NativeMethods.GetWindowThreadProcessId(hwnd, out _);
            var gui = new NativeMethods.GUITHREADINFO();
            gui.cbSize = Marshal.SizeOf<NativeMethods.GUITHREADINFO>();
            NativeMethods.GetGUIThreadInfo(threadId, ref gui);

            IntPtr target = gui.hwndFocus != IntPtr.Zero ? gui.hwndFocus : hwnd;

            var iid = NativeMethods.IID_IAccessible;
            int hr = NativeMethods.AccessibleObjectFromWindow(
                target, NativeMethods.OBJID_CARET, ref iid, out object obj);

            if (hr != 0 || obj == null) return null;

            var acc = (IAccessibleInterop)obj;
            acc.accLocation(out int left, out int top, out int width, out int height, 0);

            if (left == 0 && top == 0 && width == 0 && height == 0)
                return null;

            return new ScreenPoint { X = left, Y = top + height };
        }
        catch { return null; }
    }

    private static ScreenPoint? TryUIAutomation()
    {
        try
        {
            var focused = AutomationElement.FocusedElement;
            if (focused == null) return null;

            if (!focused.TryGetCurrentPattern(TextPattern.Pattern, out object obj))
                return null;

            var tp = (TextPattern)obj;
            var sel = tp.GetSelection();
            if (sel.Length == 0) return null;

            var range = sel[0];
            var rects = range.GetBoundingRectangles();
            if (rects.Length > 0 && !rects[0].IsEmpty && rects[0].Height > 0)
            {
                // UIAutomation returns screen coords (physical pixels)
                return new ScreenPoint { X = (int)rects[0].X, Y = (int)(rects[0].Y + rects[0].Height) };
            }

            range.ExpandToEnclosingUnit(TextUnit.Character);
            rects = range.GetBoundingRectangles();
            if (rects.Length > 0 && !rects[0].IsEmpty && rects[0].Height > 0)
            {
                return new ScreenPoint { X = (int)rects[0].X, Y = (int)(rects[0].Y + rects[0].Height) };
            }
        }
        catch { /* UIAutomation can throw */ }

        return null;
    }

    [ComImport]
    [Guid("618736E0-3C3D-11CF-810C-00AA00389B71")]
    [InterfaceType(ComInterfaceType.InterfaceIsIDispatch)]
    private interface IAccessibleInterop
    {
        void accLocation(out int pxLeft, out int pyTop, out int pcxWidth, out int pcyHeight, object varChild);
    }
}

using System.Diagnostics;
using System.Runtime.InteropServices;
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
        // Isolated in a separate class so the heavy System.Windows.Automation
        // assemblies are only loaded when this fallback is actually needed.
        result = UIAutomationCaretHelper.TryGetCaret();
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
            Debug.WriteLine($"[Caret] GUIThreadInfo client=({pt.X},{pt.Y}) hwndCaret={gui.hwndCaret}");

            // Temporarily match the target window's DPI awareness so ClientToScreen
            // converts correctly across DPI awareness boundaries (e.g., our PerMonitorV2
            // thread calling into a DPI-unaware window on a 150% monitor).
            IntPtr targetCtx = NativeMethods.GetWindowDpiAwarenessContext(gui.hwndCaret);
            IntPtr prevCtx = NativeMethods.SetThreadDpiAwarenessContext(targetCtx);
            try
            {
                if (!NativeMethods.ClientToScreen(gui.hwndCaret, ref pt))
                    return null;
            }
            finally
            {
                NativeMethods.SetThreadDpiAwarenessContext(prevCtx);
            }
            Debug.WriteLine($"[Caret] After ClientToScreen (matched DPI ctx): ({pt.X},{pt.Y})");

            // Convert from the target window's screen coordinate space to physical pixels.
            // No-op for PerMonitorV2 targets (already physical), correctly scales for
            // DPI-unaware targets.
            NativeMethods.LogicalToPhysicalPointForPerMonitorDPI(gui.hwndCaret, ref pt);
            Debug.WriteLine($"[Caret] After LogicalToPhysical: ({pt.X},{pt.Y})");

            return new ScreenPoint { X = pt.X, Y = pt.Y };
        }
        catch { return null; }
    }

    private static ScreenPoint? TryAccessibilityCaret()
    {
        try
        {
            // Force PerMonitorV2 context before COM calls — COM marshaling
            // can change the thread's DPI awareness as a side effect.
            NativeMethods.SetThreadDpiAwarenessContext(
                NativeMethods.DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

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

            try
            {
                var acc = (IAccessibleInterop)obj;
                acc.accLocation(out int left, out int top, out int width, out int height, 0);

                // Restore PerMonitorV2 — the COM call above may have changed it.
                NativeMethods.SetThreadDpiAwarenessContext(
                    NativeMethods.DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

                if (left == 0 && top == 0 && width == 0 && height == 0)
                    return null;

                return new ScreenPoint { X = left, Y = top + height };
            }
            finally
            {
                Marshal.ReleaseComObject(obj);
            }
        }
        catch { return null; }
    }

    [ComImport]
    [Guid("618736E0-3C3D-11CF-810C-00AA00389B71")]
    [InterfaceType(ComInterfaceType.InterfaceIsIDispatch)]
    private interface IAccessibleInterop
    {
        void accLocation(out int pxLeft, out int pyTop, out int pcxWidth, out int pcyHeight, object varChild);
    }
}

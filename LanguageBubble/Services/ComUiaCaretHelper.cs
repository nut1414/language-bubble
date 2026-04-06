using System.Diagnostics;
using System.Runtime.InteropServices;

namespace LanguageBubble.Services;

/// <summary>
/// Uses the COM UIA API directly to call ITextPattern2.GetCaretRange() and
/// ITextPattern.GetSelection(), which give the actual caret position for
/// modern controls (Explorer address bar, rename box, XAML text boxes, etc.).
/// </summary>
internal static class ComUiaCaretHelper
{
    public static CaretPositionService.ScreenPoint? TryGetCaret()
    {
        object? uiaObj = null;
        IUIAutomationElement? focused = null;

        try
        {
            var type = Type.GetTypeFromCLSID(s_clsidCUIAutomation8);
            if (type == null) return null;

            uiaObj = Activator.CreateInstance(type);
            if (uiaObj == null) return null;

            var uia = (IUIAutomation)uiaObj;

            int hr = uia.GetFocusedElement(out focused);
            if (hr != 0 || focused == null) return null;

            // Try focused element directly
            var result = TryGetCaretFromElement(focused);
            if (result.HasValue) return result;

            // Walk up the UIA tree — some controls expose the text pattern
            // on a parent rather than the focused leaf element.
            hr = uia.get_ControlViewWalker(out var walker);
            if (hr != 0 || walker == null) return null;

            try
            {
                IUIAutomationElement? current = focused;
                for (int i = 0; i < 4; i++)
                {
                    hr = walker.GetParentElement(current, out var parent);
                    if (current != focused)
                        Marshal.ReleaseComObject(current!);

                    if (hr != 0 || parent == null) break;
                    current = parent;

                    result = TryGetCaretFromElement(current);
                    if (result.HasValue)
                    {
                        Marshal.ReleaseComObject(current);
                        return result;
                    }
                }
                if (current != focused && current != null)
                    Marshal.ReleaseComObject(current);
            }
            finally
            {
                Marshal.ReleaseComObject(walker);
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[Caret] COM UIA exception: {ex.Message}");
        }
        finally
        {
            if (focused != null) Marshal.ReleaseComObject(focused);
            if (uiaObj != null) Marshal.ReleaseComObject(uiaObj);
        }

        return null;
    }

    /// <summary>
    /// Try TextPattern2.GetCaretRange first (best), then fall back to
    /// TextPattern.GetSelection (still gives caret position for most controls).
    /// </summary>
    private static CaretPositionService.ScreenPoint? TryGetCaretFromElement(IUIAutomationElement element)
    {
        // --- Attempt 1: TextPattern2.GetCaretRange ---
        var result = TryTextPattern2(element);
        if (result.HasValue)
        {
            Debug.WriteLine($"[Caret] COM UIA TextPattern2: ({result.Value.X}, {result.Value.Y})");
            return result;
        }

        // --- Attempt 2: TextPattern.GetSelection ---
        result = TryTextPattern(element);
        if (result.HasValue)
        {
            Debug.WriteLine($"[Caret] COM UIA TextPattern: ({result.Value.X}, {result.Value.Y})");
            return result;
        }

        return null;
    }

    private static CaretPositionService.ScreenPoint? TryTextPattern2(IUIAutomationElement element)
    {
        object? patternObj = null;
        IUIAutomationTextRange? caretRange = null;

        try
        {
            int hr = element.GetCurrentPattern(UIA_TextPattern2Id, out patternObj);
            if (hr != 0 || patternObj == null) return null;

            var tp2 = patternObj as IUIAutomationTextPattern2;
            if (tp2 == null) return null;

            hr = tp2.GetCaretRange(out _, out caretRange);
            if (hr != 0 || caretRange == null) return null;

            return PointFromRange(caretRange);
        }
        catch { return null; }
        finally
        {
            if (caretRange != null) Marshal.ReleaseComObject(caretRange);
            if (patternObj != null) Marshal.ReleaseComObject(patternObj);
        }
    }

    private static CaretPositionService.ScreenPoint? TryTextPattern(IUIAutomationElement element)
    {
        object? patternObj = null;
        IUIAutomationTextRangeArray? ranges = null;
        IUIAutomationTextRange? range = null;

        try
        {
            int hr = element.GetCurrentPattern(UIA_TextPatternId, out patternObj);
            if (hr != 0 || patternObj == null) return null;

            var tp = patternObj as IUIAutomationTextPattern;
            if (tp == null) return null;

            hr = tp.GetSelection(out ranges);
            if (hr != 0 || ranges == null) return null;

            hr = ranges.get_Length(out int length);
            if (hr != 0 || length == 0) return null;

            hr = ranges.GetElement(0, out range);
            if (hr != 0 || range == null) return null;

            return PointFromRange(range);
        }
        catch { return null; }
        finally
        {
            if (range != null) Marshal.ReleaseComObject(range);
            if (ranges != null) Marshal.ReleaseComObject(ranges);
            if (patternObj != null) Marshal.ReleaseComObject(patternObj);
        }
    }

    private static CaretPositionService.ScreenPoint? PointFromRange(IUIAutomationTextRange range)
    {
        int hr = range.GetBoundingRectangles(out double[] rects);
        if (hr == 0 && rects is { Length: >= 4 } && rects[3] > 0)
        {
            return new CaretPositionService.ScreenPoint
            {
                X = (int)rects[0],
                Y = (int)(rects[1] + rects[3])
            };
        }

        // Degenerate (empty) range — expand to a character and retry
        range.ExpandToEnclosingUnit(0 /* TextUnit.Character */);
        hr = range.GetBoundingRectangles(out rects);
        if (hr == 0 && rects is { Length: >= 4 } && rects[3] > 0)
        {
            return new CaretPositionService.ScreenPoint
            {
                X = (int)rects[0],
                Y = (int)(rects[1] + rects[3])
            };
        }

        return null;
    }

    // ---------------------------------------------------------------
    //  Constants
    // ---------------------------------------------------------------
    private static readonly Guid s_clsidCUIAutomation8 =
        new("E22AD333-B25F-460C-83D0-0581107395C9");

    private const int UIA_TextPatternId = 10014;
    private const int UIA_TextPattern2Id = 10024;

    // ---------------------------------------------------------------
    //  Minimal COM interface definitions.
    //  Vtable-slot placeholders for methods we never call are
    //  named _Reservedxx.  Only methods we actually invoke have
    //  their real signatures.
    // ---------------------------------------------------------------

    // --- IUIAutomation ---

    [ComImport, Guid("30CBE57D-D9D0-452A-AB13-7AC5AC4825EE")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomation
    {
        void _Reserved00(); // CompareElements
        void _Reserved01(); // CompareRuntimeIds
        void _Reserved02(); // GetRootElement
        void _Reserved03(); // ElementFromHandle
        void _Reserved04(); // ElementFromPoint

        [PreserveSig]
        int GetFocusedElement(
            [MarshalAs(UnmanagedType.Interface)] out IUIAutomationElement element);

        void _Reserved05(); // CreateTreeWalker

        [PreserveSig]
        int get_ControlViewWalker(
            [MarshalAs(UnmanagedType.Interface)] out IUIAutomationTreeWalker walker);
    }

    // --- IUIAutomationElement ---

    [ComImport, Guid("D22108AA-8AC5-49A5-837B-37BBB3D7591E")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomationElement
    {
        void _Reserved00(); // SetFocus
        void _Reserved01(); // GetRuntimeId
        void _Reserved02(); // FindFirst
        void _Reserved03(); // FindAll
        void _Reserved04(); // FindFirstBuildCache
        void _Reserved05(); // FindAllBuildCache
        void _Reserved06(); // BuildUpdatedCache
        void _Reserved07(); // GetCurrentPropertyValue
        void _Reserved08(); // GetCurrentPropertyValueEx
        void _Reserved09(); // GetCachedPropertyValue
        void _Reserved10(); // GetCachedPropertyValueEx
        void _Reserved11(); // GetCurrentPatternAs
        void _Reserved12(); // GetCachedPatternAs

        [PreserveSig]
        int GetCurrentPattern(
            int patternId,
            [MarshalAs(UnmanagedType.IUnknown)] out object patternObject);
    }

    // --- IUIAutomationTextPattern ---

    [ComImport, Guid("32EBA289-3583-42C9-9C59-3B6D9A1E9B6A")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomationTextPattern
    {
        void _Reserved00(); // RangeFromPoint
        void _Reserved01(); // RangeFromChild

        [PreserveSig]
        int GetSelection(
            [MarshalAs(UnmanagedType.Interface)] out IUIAutomationTextRangeArray ranges);
    }

    // --- IUIAutomationTextPattern2 ---

    [ComImport, Guid("506A921A-FCC9-409F-B23B-37EB74106872")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomationTextPattern2
    {
        // inherited from IUIAutomationTextPattern (6 slots)
        void _Reserved00(); // RangeFromPoint
        void _Reserved01(); // RangeFromChild
        void _Reserved02(); // GetSelection
        void _Reserved03(); // GetVisibleRanges
        void _Reserved04(); // get_DocumentRange
        void _Reserved05(); // get_SupportedTextSelection

        // IUIAutomationTextPattern2 own methods
        void _Reserved06(); // RangeFromAnnotation

        [PreserveSig]
        int GetCaretRange(
            [MarshalAs(UnmanagedType.Bool)] out bool isActive,
            [MarshalAs(UnmanagedType.Interface)] out IUIAutomationTextRange range);
    }

    // --- IUIAutomationTextRange ---

    [ComImport, Guid("A543CC6A-F4AE-494B-8239-C814481187A8")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomationTextRange
    {
        void _Reserved00(); // Clone
        void _Reserved01(); // Compare
        void _Reserved02(); // CompareEndpoints

        [PreserveSig]
        int ExpandToEnclosingUnit(int textUnit);

        void _Reserved03(); // FindAttribute
        void _Reserved04(); // FindText
        void _Reserved05(); // GetAttributeValue

        [PreserveSig]
        int GetBoundingRectangles(
            [MarshalAs(UnmanagedType.SafeArray, SafeArraySubType = VarEnum.VT_R8)]
            out double[] boundingRects);
    }

    // --- IUIAutomationTextRangeArray ---

    [ComImport, Guid("CE4AE76A-E717-4C98-81EA-47371D028EB6")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomationTextRangeArray
    {
        [PreserveSig]
        int get_Length(out int length);

        [PreserveSig]
        int GetElement(
            int index,
            [MarshalAs(UnmanagedType.Interface)] out IUIAutomationTextRange element);
    }

    // --- IUIAutomationTreeWalker ---

    [ComImport, Guid("4042C624-389C-4AFC-A630-9DF854A541FC")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IUIAutomationTreeWalker
    {
        [PreserveSig]
        int GetParentElement(
            [MarshalAs(UnmanagedType.Interface)] IUIAutomationElement element,
            [MarshalAs(UnmanagedType.Interface)] out IUIAutomationElement parent);
    }
}

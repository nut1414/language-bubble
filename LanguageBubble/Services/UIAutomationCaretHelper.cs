using System.Diagnostics;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Automation.Text;

namespace LanguageBubble.Services;

/// <summary>
/// Isolated into its own class so the heavy System.Windows.Automation assemblies
/// are only loaded by the JIT when this strategy is actually called.
/// </summary>
internal static class UIAutomationCaretHelper
{
    // TextPattern2 AutomationPattern — not exposed as a static field in the .NET API,
    // so we look it up by its well-known GUID.
    private static readonly AutomationPattern? s_textPattern2 = LookupTextPattern2();

    private static AutomationPattern? LookupTextPattern2()
    {
        try
        {
            // TextPattern2 pattern id GUID
            return AutomationPattern.LookupById(10024); // UIA_TextPattern2Id
        }
        catch { return null; }
    }

    public static CaretPositionService.ScreenPoint? TryGetCaret()
    {
        try
        {
            var focused = AutomationElement.FocusedElement;
            if (focused == null) return null;

            // Strategy A: TextPattern (classic)
            var result = TryFromTextPattern(focused);
            if (result.HasValue) return result;

            // Strategy B: TextPattern2 — exposes GetCaretRange() for controls
            // that support it (Explorer address bar, modern XAML text boxes, etc.)
            result = TryFromTextPattern2(focused);
            if (result.HasValue) return result;

            // Strategy C: Walk up the UIA tree looking for a TextPattern/TextPattern2
            // ancestor — some controls place the pattern on the parent, not the
            // focused leaf element.
            var walker = TreeWalker.ControlViewWalker;
            var parent = walker.GetParent(focused);
            for (int depth = 0; depth < 4 && parent != null; depth++)
            {
                result = TryFromTextPattern(parent);
                if (result.HasValue) return result;

                result = TryFromTextPattern2(parent);
                if (result.HasValue) return result;

                parent = walker.GetParent(parent);
            }

            // Last resort: ValuePattern + BoundingRectangle.
            // This only gives the element's bounding rect (not the caret),
            // but at least places the bubble near the text box.
            result = TryFromValuePattern(focused);
            if (result.HasValue) return result;
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[Caret] UIAutomation exception: {ex.Message}");
        }

        return null;
    }

    private static CaretPositionService.ScreenPoint? TryFromTextPattern(AutomationElement element)
    {
        try
        {
            if (!element.TryGetCurrentPattern(TextPattern.Pattern, out object obj))
                return null;

            var tp = (TextPattern)obj;
            var sel = tp.GetSelection();
            if (sel.Length == 0) return null;

            return PointFromRange(sel[0]);
        }
        catch { return null; }
    }

    private static CaretPositionService.ScreenPoint? TryFromTextPattern2(AutomationElement element)
    {
        try
        {
            if (s_textPattern2 == null) return null;

            if (!element.TryGetCurrentPattern(s_textPattern2, out object obj))
                return null;

            // TextPattern2 extends TextPattern — GetCaretRange is available via
            // the COM interface. We can cast to TextPattern and use GetSelection
            // which still works for caret position when selection is collapsed.
            if (obj is TextPattern tp)
            {
                var sel = tp.GetSelection();
                if (sel.Length == 0) return null;
                return PointFromRange(sel[0]);
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[Caret] TextPattern2 failed: {ex.Message}");
        }
        return null;
    }

    private static CaretPositionService.ScreenPoint? TryFromValuePattern(AutomationElement element)
    {
        try
        {
            if (!element.TryGetCurrentPattern(ValuePattern.Pattern, out _))
                return null;

            var rect = element.Current.BoundingRectangle;
            if (rect == Rect.Empty || rect.Height <= 0)
                return null;

            // Use the horizontal center of the element — rough but better than
            // hard-left, and places the bubble visually over the text box.
            return new CaretPositionService.ScreenPoint
            {
                X = (int)(rect.X + rect.Width / 2),
                Y = (int)(rect.Y + rect.Height)
            };
        }
        catch { return null; }
    }

    private static CaretPositionService.ScreenPoint? PointFromRange(TextPatternRange range)
    {
        var rects = range.GetBoundingRectangles();
        if (rects.Length > 0 && !rects[0].IsEmpty && rects[0].Height > 0)
        {
            return new CaretPositionService.ScreenPoint
            {
                X = (int)rects[0].X,
                Y = (int)(rects[0].Y + rects[0].Height)
            };
        }

        range.ExpandToEnclosingUnit(TextUnit.Character);
        rects = range.GetBoundingRectangles();
        if (rects.Length > 0 && !rects[0].IsEmpty && rects[0].Height > 0)
        {
            return new CaretPositionService.ScreenPoint
            {
                X = (int)rects[0].X,
                Y = (int)(rects[0].Y + rects[0].Height)
            };
        }

        return null;
    }
}

using System.Windows.Automation;
using System.Windows.Automation.Text;

namespace LanguageBubble.Services;

/// <summary>
/// Isolated into its own class so the heavy System.Windows.Automation assemblies
/// are only loaded by the JIT when this strategy is actually called.
/// </summary>
internal static class UIAutomationCaretHelper
{
    public static CaretPositionService.ScreenPoint? TryGetCaret()
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
                return new CaretPositionService.ScreenPoint { X = (int)rects[0].X, Y = (int)(rects[0].Y + rects[0].Height) };
            }

            range.ExpandToEnclosingUnit(TextUnit.Character);
            rects = range.GetBoundingRectangles();
            if (rects.Length > 0 && !rects[0].IsEmpty && rects[0].Height > 0)
            {
                return new CaretPositionService.ScreenPoint { X = (int)rects[0].X, Y = (int)(rects[0].Y + rects[0].Height) };
            }
        }
        catch { /* UIAutomation can throw */ }

        return null;
    }
}

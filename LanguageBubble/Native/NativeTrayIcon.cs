using System.Runtime.InteropServices;
using System.Windows.Interop;

namespace LanguageBubble.Native;

/// <summary>
/// Lightweight native tray icon using Shell_NotifyIcon.
/// Replaces WinForms.NotifyIcon to avoid loading the entire WinForms framework.
/// </summary>
internal sealed class NativeTrayIcon : IDisposable
{
    private const uint CallbackMessage = NativeMethods.WM_USER + 1;

    private HwndSource? _hwndSource;
    private IntPtr _hIcon;
    private bool _iconAdded;

    public event Action? RightClick;

    public void Create(string tooltip, string? iconPath)
    {
        // Create a message-only window to receive tray icon callbacks
        var parameters = new HwndSourceParameters("LanguageBubbleTray")
        {
            Width = 0,
            Height = 0,
            WindowStyle = 0,
            ParentWindow = new IntPtr(-3) // HWND_MESSAGE
        };
        _hwndSource = new HwndSource(parameters);
        _hwndSource.AddHook(WndProc);

        // Load icon
        if (iconPath != null && System.IO.File.Exists(iconPath))
        {
            _hIcon = NativeMethods.LoadImage(
                IntPtr.Zero, iconPath, NativeMethods.IMAGE_ICON,
                0, 0, NativeMethods.LR_LOADFROMFILE | NativeMethods.LR_DEFAULTSIZE);
        }

        // Add tray icon
        var nid = MakeNid();
        nid.uFlags = NativeMethods.NIF_MESSAGE | NativeMethods.NIF_TIP
                   | (_hIcon != IntPtr.Zero ? NativeMethods.NIF_ICON : 0);
        nid.uCallbackMessage = CallbackMessage;
        nid.hIcon = _hIcon;
        nid.szTip = tooltip;

        _iconAdded = NativeMethods.Shell_NotifyIcon(NativeMethods.NIM_ADD, ref nid);
    }

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (msg == (int)CallbackMessage)
        {
            uint mouseMsg = (uint)(lParam.ToInt64() & 0xFFFF);
            if (mouseMsg == NativeMethods.WM_RBUTTONUP)
            {
                handled = true;
                // SetForegroundWindow is required for the context menu to dismiss properly
                NativeMethods.SetForegroundWindow(_hwndSource!.Handle);
                RightClick?.Invoke();
            }
        }
        return IntPtr.Zero;
    }

    private NativeMethods.NOTIFYICONDATA MakeNid()
    {
        var nid = new NativeMethods.NOTIFYICONDATA();
        nid.cbSize = Marshal.SizeOf<NativeMethods.NOTIFYICONDATA>();
        nid.hWnd = _hwndSource!.Handle;
        nid.uID = 1;
        nid.szTip = "";
        return nid;
    }

    public void Dispose()
    {
        if (_iconAdded && _hwndSource != null)
        {
            var nid = MakeNid();
            NativeMethods.Shell_NotifyIcon(NativeMethods.NIM_DELETE, ref nid);
            _iconAdded = false;
        }

        if (_hIcon != IntPtr.Zero)
        {
            NativeMethods.DestroyIcon(_hIcon);
            _hIcon = IntPtr.Zero;
        }

        _hwndSource?.Dispose();
        _hwndSource = null;
    }
}

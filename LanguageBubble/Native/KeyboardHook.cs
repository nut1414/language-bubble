using System.Diagnostics;
using System.Runtime.InteropServices;
using LanguageBubble.Models;

namespace LanguageBubble.Native;

internal sealed class KeyboardHook : IDisposable
{
    private IntPtr _hookId = IntPtr.Zero;
    private readonly NativeMethods.LowLevelKeyboardProc _hookProc;
    private bool _disposed;

    // Tag injected events so the hook passes them through
    private const int SELF_INJECTED_TAG = 0x4C42;

    // Modifier tracking
    private bool _winHeld;
    private bool _altHeld;
    private bool _shiftHeld;
    private bool _winUsedForCombo;

    public bool SuppressSelfGenerated { get; set; }

    // Per-key mode (set from App.xaml.cs, read from hook callback)
    public SwitchMode CapsLockMode { get; set; } = SwitchMode.AllLanguage;
    public SwitchMode WinSpaceMode { get; set; } = SwitchMode.Unused;
    public SwitchMode AltShiftMode { get; set; } = SwitchMode.Unused;

    public event Action<HookKeyCombo>? SwitchKeyPressed;
    public event Action? KeyPressed;

    public KeyboardHook()
    {
        _hookProc = HookCallback;
    }

    public void Install()
    {
        if (_hookId != IntPtr.Zero)
            return;

        using var process = Process.GetCurrentProcess();
        using var module = process.MainModule!;
        var hMod = NativeMethods.GetModuleHandle(module.ModuleName);

        _hookId = NativeMethods.SetWindowsHookEx(
            NativeMethods.WH_KEYBOARD_LL,
            _hookProc,
            hMod,
            0);

        if (_hookId == IntPtr.Zero)
        {
            int error = Marshal.GetLastWin32Error();
            throw new InvalidOperationException($"Failed to install keyboard hook. Error code: {error}");
        }
    }

    public void Uninstall()
    {
        if (_hookId != IntPtr.Zero)
        {
            NativeMethods.UnhookWindowsHookEx(_hookId);
            _hookId = IntPtr.Zero;
        }
    }

    private IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
    {
        if (nCode < 0)
            return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);

        if (SuppressSelfGenerated)
            return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);

        var hookStruct = Marshal.PtrToStructure<NativeMethods.KBDLLHOOKSTRUCT>(lParam);

        // Pass through events we injected ourselves (tagged via dwExtraInfo)
        if (hookStruct.dwExtraInfo == (IntPtr)SELF_INJECTED_TAG)
            return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);

        int msg = wParam.ToInt32();
        bool isKeyDown = msg == NativeMethods.WM_KEYDOWN || msg == NativeMethods.WM_SYSKEYDOWN;
        bool isKeyUp = msg == NativeMethods.WM_KEYUP || msg == NativeMethods.WM_SYSKEYUP;
        int vk = (int)hookStruct.vkCode;

        // --- Win key (LWin / RWin) ---
        if (vk == NativeMethods.VK_LWIN || vk == NativeMethods.VK_RWIN)
        {
            if (isKeyDown)
            {
                _winHeld = true;
            }
            else if (isKeyUp)
            {
                _winHeld = false;
                if (_winUsedForCombo)
                {
                    _winUsedForCombo = false;
                    // Suppress the real Win key-up, then inject:
                    // 1) A harmless Ctrl tap to "dirty" the Win sequence (prevents Start menu)
                    // 2) A synthetic Win key-up so the OS properly releases Win (prevents stuck key)
                    // All injected events are tagged so our hook passes them through.
                    var extra = (UIntPtr)SELF_INJECTED_TAG;
                    NativeMethods.keybd_event(0xA2, 0, 0, extra);                                    // Ctrl down
                    NativeMethods.keybd_event(0xA2, 0, NativeMethods.KEYEVENTF_KEYUP, extra);        // Ctrl up
                    NativeMethods.keybd_event((byte)vk, 0, NativeMethods.KEYEVENTF_KEYUP, extra);    // Win up
                    return (IntPtr)1; // Suppress the real Win key-up
                }
            }
            return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);
        }

        // --- Space (Win+Space combo) ---
        if (vk == NativeMethods.VK_SPACE)
        {
            if (_winHeld && WinSpaceMode != SwitchMode.Unused)
            {
                if (isKeyDown)
                {
                    _winUsedForCombo = true;
                    SwitchKeyPressed?.Invoke(HookKeyCombo.WinSpace);
                }
                return (IntPtr)1; // Suppress Space both down and up
            }
            // Not a combo — fall through to generic handler
        }

        // --- Alt key (LMenu / RMenu / Menu) ---
        if (vk == NativeMethods.VK_LMENU || vk == NativeMethods.VK_RMENU || vk == NativeMethods.VK_MENU)
        {
            if (isKeyDown)
            {
                _altHeld = true;
                if (_shiftHeld && AltShiftMode != SwitchMode.Unused)
                {
                    // Suppress Alt key-down to prevent Windows native Alt+Shift switch
                    SwitchKeyPressed?.Invoke(HookKeyCombo.AltShift);
                    return (IntPtr)1;
                }
            }
            else if (isKeyUp)
            {
                _altHeld = false;
                // Always pass through Alt key-up to avoid stuck Alt
            }
            return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);
        }

        // --- Shift key (LShift / RShift / Shift) ---
        if (vk == NativeMethods.VK_LSHIFT || vk == NativeMethods.VK_RSHIFT || vk == NativeMethods.VK_SHIFT)
        {
            if (isKeyDown)
            {
                _shiftHeld = true;
                if (_altHeld && AltShiftMode != SwitchMode.Unused)
                {
                    // Suppress Shift key-down to prevent Windows native Alt+Shift switch
                    SwitchKeyPressed?.Invoke(HookKeyCombo.AltShift);
                    return (IntPtr)1;
                }
            }
            else if (isKeyUp)
            {
                _shiftHeld = false;
                // Always pass through Shift key-up to avoid stuck Shift
            }
            return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);
        }

        // --- CapsLock ---
        if (vk == NativeMethods.VK_CAPITAL)
        {
            if (CapsLockMode == SwitchMode.Unused)
                return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);

            if (isKeyDown)
                SwitchKeyPressed?.Invoke(HookKeyCombo.CapsLock);

            return (IntPtr)1; // Suppress both down and up
        }

        // --- All other keys ---
        if (isKeyDown)
            KeyPressed?.Invoke();

        return NativeMethods.CallNextHookEx(_hookId, nCode, wParam, lParam);
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            Uninstall();
            _disposed = true;
        }
    }
}

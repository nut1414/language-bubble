using System.Diagnostics;
using System.Runtime.InteropServices;
using LanguageBubble.Models;

namespace LanguageBubble.Native;

internal sealed class KeyboardHook : IDisposable
{
    private IntPtr _hookId = IntPtr.Zero;
    private readonly NativeMethods.LowLevelKeyboardProc _hookProc;
    private bool _disposed;

    // Modifier tracking
    private bool _winHeld;
    private bool _altHeld;
    private bool _shiftHeld;
    private bool _winUsedForCombo;
    private bool _altShiftFired;

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
                    return (IntPtr)1; // Suppress Win key-up to prevent Start menu
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
                return (IntPtr)1; // Suppress both down and up
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
                    _altShiftFired = true;
                    SwitchKeyPressed?.Invoke(HookKeyCombo.AltShift);
                    return (IntPtr)1;
                }
            }
            else if (isKeyUp)
            {
                _altHeld = false;
                if (_altShiftFired)
                {
                    _altShiftFired = false;
                    return (IntPtr)1; // Suppress to prevent Windows native Alt+Shift switch
                }
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
                    _altShiftFired = true;
                    SwitchKeyPressed?.Invoke(HookKeyCombo.AltShift);
                    return (IntPtr)1;
                }
            }
            else if (isKeyUp)
            {
                _shiftHeld = false;
                if (_altShiftFired)
                {
                    _altShiftFired = false;
                    return (IntPtr)1; // Suppress to prevent Windows native Alt+Shift switch
                }
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

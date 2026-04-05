using LanguageBubble.Native;

namespace LanguageBubble.Services;

internal static class CapsLockService
{
    public static void EnsureCapsLockOff(KeyboardHook hook)
    {
        // Check if Caps Lock is currently ON (bit 0 of GetKeyState)
        bool isOn = (NativeMethods.GetKeyState(NativeMethods.VK_CAPITAL) & 0x0001) != 0;

        if (!isOn)
            return;

        // Temporarily suppress self-generated keystrokes so our hook doesn't intercept them
        hook.SuppressSelfGenerated = true;
        try
        {
            // Simulate Caps Lock press and release to toggle it OFF
            NativeMethods.keybd_event(
                (byte)NativeMethods.VK_CAPITAL,
                0x45,
                NativeMethods.KEYEVENTF_EXTENDEDKEY,
                UIntPtr.Zero);

            NativeMethods.keybd_event(
                (byte)NativeMethods.VK_CAPITAL,
                0x45,
                NativeMethods.KEYEVENTF_EXTENDEDKEY | NativeMethods.KEYEVENTF_KEYUP,
                UIntPtr.Zero);
        }
        finally
        {
            hook.SuppressSelfGenerated = false;
        }
    }
}

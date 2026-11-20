using Microsoft.Win32.SafeHandles;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Principal;

namespace Muster.Windows;

internal static class DesktopInstance
{
    delegate bool WindowVisitor(nint window, nint context);
    [DllImport("user32.dll")] static extern bool EnumWindows(WindowVisitor visitor, nint context);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern nint GetProp(nint window, string name);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool SetProp(nint window, string name, nint value);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern nint RemoveProp(nint window, string name);
    [DllImport("user32.dll")] static extern bool IsIconic(nint window);
    [DllImport("user32.dll")] static extern bool ShowWindowAsync(nint window, int command);
    [DllImport("user32.dll")] static extern nint GetLastActivePopup(nint window);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(nint window);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(nint window);
    [DllImport("user32.dll")] static extern bool FlashWindow(nint window, bool invert);

    static nint FindMarkedWindow(string marker)
    {
        nint found = 0;
        EnumWindows((window, _) =>
        {
            if (GetProp(window, marker) == 0) return true;
            found = window;
            return false;
        }, 0);
        return found;
    }

    static void Activate(nint window)
    {
        const int swRestore = 9;
        if (IsIconic(window)) ShowWindowAsync(window, swRestore);
        var popup = GetLastActivePopup(window);
        var target = popup != 0 && IsWindowVisible(popup) ? popup : window;
        if (!SetForegroundWindow(target)) FlashWindow(target, true);
    }

    [DllImport("advapi32.dll")]
    static extern bool OpenProcessToken(SafeProcessHandle process, uint access, out SafeAccessTokenHandle token);

    static bool SameUser(Process process, SecurityIdentifier user)
    {
        const uint tokenQuery = 8;
        if (!OpenProcessToken(process.SafeHandle, tokenQuery, out var token)) return false;
        using (token)
        using (var identity = new WindowsIdentity(token.DangerousGetHandle()))
            return identity.User is {} owner && user.Equals(owner);
    }

    // A pre-single-instance installed GUI may still be running.
    static nint FindInstalledWindow(string path, SecurityIdentifier user)
    {
        using var self = Process.GetCurrentProcess();
        foreach (var process in Process.GetProcessesByName("Muster"))
        {
            using (process)
            {
                try
                {
                    if (process.Id != self.Id && process.SessionId == self.SessionId &&
                        process.MainWindowHandle != 0 && process.MainWindowTitle == "Muster" &&
                        string.Equals(process.MainModule?.FileName, path, StringComparison.OrdinalIgnoreCase) && SameUser(process, user))
                        return process.MainWindowHandle;
                }
                catch (InvalidOperationException) { } // Exited during enumeration.
                catch (Win32Exception) { } // Inaccessible process.
            }
        }
        return 0;
    }

    static bool Acquire(Mutex gate, int milliseconds)
    {
        try { return gate.WaitOne(milliseconds); }
        catch (AbandonedMutexException) { return true; }
    }

    public static void Run(string name, Func<Form> create, string? installedPath = null)
    {
        using var user = WindowsIdentity.GetCurrent();
        var marker = name + "." + user.User!.Value;
        var scope = new NamedWaitHandleOptions { CurrentUserOnly = true, CurrentSessionOnly = true };
        using var gate = new Mutex(false, name, scope);
        var wait = Stopwatch.StartNew();
        while (!Acquire(gate, 50))
        {
            var window = FindMarkedWindow(marker);
            if (window != 0) { Activate(window); return; }
            if (wait.Elapsed >= TimeSpan.FromSeconds(10))
                throw new TimeoutException("Muster is already starting in this session.");
        }
        try
        {
            var legacy = installedPath == null ? 0 : FindInstalledWindow(installedPath, user.User!);
            if (legacy != 0)
            {
                Activate(legacy);
                return;
            }
            using var form = create();
            nint marked = 0;
            void Mark()
            {
                marked = form.Handle;
                if (!SetProp(marked, marker, 1)) throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            form.HandleCreated += (_, _) => Mark();
            form.HandleDestroyed += (_, _) => { if (marked != 0) RemoveProp(marked, marker); marked = 0; };
            if (form.IsHandleCreated) Mark();
            form.WindowState = FormWindowState.Maximized;
            Application.Run(form);
        }
        finally { gate.ReleaseMutex(); }
    }
}

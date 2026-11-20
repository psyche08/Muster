using System.Diagnostics;
using System.Runtime.InteropServices;
using Muster.Windows;

internal static class Program
{
    [DllImport("user32.dll")] static extern bool IsIconic(nint window);
    [DllImport("user32.dll")] static extern bool IsZoomed(nint window);
    [DllImport("user32.dll")] static extern bool ShowWindowAsync(nint window, int command);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongW")] static extern int GetWindowStyle(nint window, int index);

    static void Until(Func<bool> observed, string failure)
    {
        if (!SpinWait.SpinUntil(observed, TimeSpan.FromSeconds(12))) throw new InvalidOperationException(failure);
    }

    [STAThread]
    static void Main(string[] args)
    {
        ApplicationConfiguration.Initialize();
        if (args is [var id, var record])
        {
            DesktopInstance.Run(id, () => {
                File.AppendAllText(record, $"{Environment.ProcessId}\n");
                return new Form { Text = id };
            });
            return;
        }
        string name = "Muster.Test." + Guid.NewGuid().ToString("N");
        string file = Path.GetTempFileName();
        var children = new List<Process>();
        Process Launch()
        {
            var start = new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = false };
            start.ArgumentList.Add(name);
            start.ArgumentList.Add(file);
            var child = Process.Start(start)!;
            children.Add(child);
            return child;
        }
        try
        {
            for (int i = 0; i < 4; i++) Launch();
            Until(() => children.Count(p => !p.HasExited) == 1 && File.ReadAllLines(file).Length == 1, "Concurrent launches did not converge");
            var owner = children.Single(p => !p.HasExited);
            Until(() => { owner.Refresh(); return owner.MainWindowHandle != 0 && IsZoomed(owner.MainWindowHandle); }, "First window was not maximized");
            nint hwnd = owner.MainWindowHandle;
            const int captionAndFrame = 0x00C40000;
            if ((GetWindowStyle(hwnd, -16) & captionAndFrame) != captionAndFrame)
                throw new InvalidOperationException("Window lacks native title bar or sizing frame");

            ShowWindowAsync(hwnd, 6); // SW_MINIMIZE
            Until(() => IsIconic(hwnd), "Minimize failed");
            bool opened = DesktopInstance.Run(name, () => throw new InvalidOperationException("Second GUI created"), activateExisting: false);
            if (opened || !IsIconic(hwnd)) throw new InvalidOperationException("Isolated launch activated the live owner");
            var second = Launch();
            Until(() => second.HasExited && !IsIconic(hwnd) && IsZoomed(hwnd), "Repeated launch did not restore the maximized owner");
            ShowWindowAsync(hwnd, 9); // SW_RESTORE
            Until(() => !IsZoomed(hwnd), "Could not restore the window by hand");
            var third = Launch();
            Until(() => third.HasExited, "Repeated launch left another process");
            if (IsZoomed(hwnd) || File.ReadAllLines(file).Length != 1)
                throw new InvalidOperationException("Repeated launch resized or replaced the window");

            owner.CloseMainWindow();
            Until(() => owner.HasExited, "Owner did not shut down");
            var replacement = Launch();
            Until(() => { replacement.Refresh(); return replacement.MainWindowHandle != 0 && IsZoomed(replacement.MainWindowHandle); }, "Fresh launch after shutdown failed");
            Console.WriteLine("Single window: concurrent start, native maximize, restore, relaunch and clean restart passed");
        }
        finally
        {
            foreach (var child in children)
            {
                if (!child.HasExited && (!child.CloseMainWindow() || !child.WaitForExit(3000))) child.Kill();
                child.Dispose();
            }
            File.Delete(file);
        }
    }
}

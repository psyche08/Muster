using Microsoft.Win32;
using Muster.Windows;
using System.Runtime.InteropServices;

internal static class PowerMenuTests
{
    [DllImport("kernel32.dll")]
    static extern uint SetThreadExecutionState(uint flags);

    public static void Run()
    {
        const uint continuous = 0x80000000;
        var subkey = @"Software\Muster\Tests\" + Guid.NewGuid().ToString("N");
        var key = @"HKEY_CURRENT_USER\" + subkey;
        uint initial = SetThreadExecutionState(continuous);
        void Expect(uint wanted)
        {
            uint previous = SetThreadExecutionState(continuous | wanted);
            if (previous != (continuous | wanted))
                throw new InvalidOperationException($"Expected power flags {wanted}, observed {previous:x8}");
        }
        static (ToolStripMenuItem Awake, ToolStripMenuItem Screen) Controls(PowerMenu menu)
        {
            var items = ((ToolStripMenuItem)menu.Items[0]).DropDownItems;
            return ((ToolStripMenuItem)items[0], (ToolStripMenuItem)items[1]);
        }
        try
        {
            using (var menu = new PowerMenu(key))
            {
                var (awake, screen) = Controls(menu);
                if (!awake.Checked || screen.Checked || !screen.Enabled) throw new Exception("Unexpected defaults");
                Expect(1);
                screen.PerformClick();
                Expect(3);
                awake.PerformClick();
                Expect(0);
                if (awake.Checked || screen.Enabled || !screen.Checked) throw new Exception("Lost screen preference");
            }
            Expect(0);
            using (var menu = new PowerMenu(key))
            {
                var (awake, screen) = Controls(menu);
                Expect(0);
                if (awake.Checked || !screen.Checked || screen.Enabled) throw new Exception("Incorrect restored menu");
                awake.PerformClick();
                Expect(3);
                screen.PerformClick();
                Expect(1);
            }
            Expect(0);
        }
        finally
        {
            SetThreadExecutionState(initial);
            Registry.CurrentUser.DeleteSubKey(subkey, throwOnMissingSubKey: false);
        }
    }
}

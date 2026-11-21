namespace Muster.Windows;

internal static class Program
{
    [STAThread]
    static int Main(string[] args)
    {
        ApplicationConfiguration.Initialize();
        bool attach = args is ["--connect-installed"];
        string? fixture = null;
#if MUSTER_ISOLATED_DEVELOPMENT
        fixture = args is ["--connect-isolated", var socket] ? socket : null;
        if (fixture != null)
        {
            // The development daemon prints a new directory under the user's temporary directory.
            string temp = Path.TrimEndingDirectorySeparator(Path.GetTempPath()) + Path.DirectorySeparatorChar;
            if (!Path.IsPathFullyQualified(fixture) || !Path.GetFullPath(fixture).StartsWith(temp, StringComparison.OrdinalIgnoreCase)
                || Path.GetFileName(fixture) != "musterd.sock") return 1;
            string directory = Path.GetDirectoryName(Path.GetFullPath(fixture))!;
            if (!Directory.Exists(directory)) return 1;
            Environment.SetEnvironmentVariable("MUSTER_DIR", Path.Combine(directory, "client"));
        }
#endif
        if (args.Length != 0 && !attach && fixture == null)
        {
            MessageBox.Show("Usage: Muster.exe [--connect-installed]", "Muster");
            return 1;
        }
        var installed = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Muster", "Muster.exe");
        try
        {
            bool opened = DesktopInstance.Run("Muster.Windows.Desktop.v1",
                () =>
                {
                    var desktop = new Desktop(attach, fixture);
                    if (!attach && fixture == null)
                    {
                        var menu = new PowerMenu(PowerMenu.PreferenceKey);
                        desktop.MainMenuStrip = menu;
                        desktop.Controls.Add(menu);
                    }
                    return desktop;
                }, installed, activateExisting: fixture == null);
            return !opened && fixture != null ? 2 : 0;
        }
        catch (TimeoutException ex)
        {
            if (fixture != null) return 2;
            MessageBox.Show(ex.Message, "Muster");
            return 1;
        }
    }
}

namespace Muster.Windows;

internal static class Program
{
    [STAThread]
    static void Main(string[] args)
    {
        ApplicationConfiguration.Initialize();
        bool attach = args is ["--connect-installed"];
        if (args.Length != 0 && !attach)
        {
            MessageBox.Show("Usage: Muster.exe [--connect-installed]", "Muster");
            return;
        }
        var installed = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Muster", "Muster.exe");
        try { DesktopInstance.Run("Muster.Windows.Desktop.v1", () => new Desktop(attach), installed); }
        catch (TimeoutException ex) { MessageBox.Show(ex.Message, "Muster"); }
    }
}

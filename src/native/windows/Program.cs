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
        Application.Run(new Desktop(attach));
    }
}

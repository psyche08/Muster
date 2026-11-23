using Muster.V1;
using Muster.Windows;

internal static class DesktopViewerTests
{
    internal static void Run()
    {
        using var viewer = new DesktopViewerForm("Test host");
        ulong confirmed = 0;
        viewer.TakeoverRequested += token => confirmed = token;
        viewer.Show();
        if (viewer.Owner != null || viewer.TopMost || !viewer.ShowInTaskbar ||
            viewer.WindowState != FormWindowState.Normal || viewer.FormBorderStyle != FormBorderStyle.Sizable)
            throw new Exception("Viewer must open as an independent normal taskbar window");
        var choice = viewer.Controls.OfType<FlowLayoutPanel>().Single();
        var prompt = new DesktopTakeoverPrompt { Controller = "Test controller", Confirmation = 42 };
        viewer.Display(prompt);
        if (!choice.Visible || !choice.Controls.OfType<Label>().Single().Text.Contains(prompt.Controller) || confirmed != 0)
            throw new Exception("Showing the named occupant must wait for a choice");
        choice.Controls.OfType<Button>().Single(b => b.Text == "Disconnect and connect").PerformClick();
        if (choice.Visible || confirmed != prompt.Confirmation) throw new Exception("Stale takeover choice");
        confirmed = 0;
        viewer.Display(prompt);
        choice.Controls.OfType<Button>().Single(b => b.Text == "Keep current session").PerformClick();
        if (confirmed != 0 || viewer.Visible) throw new Exception("Keeping the old viewer must cancel locally");
    }
}

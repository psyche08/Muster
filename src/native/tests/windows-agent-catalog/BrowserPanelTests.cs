using Muster.App;
using Muster.Windows;

internal static class BrowserPanelTests
{
    static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static void Run()
    {
        using var form = new Form { ClientSize = new(900, 650), StartPosition = FormStartPosition.Manual,
            Location = new(-20000, -20000), ShowInTaskbar = false };
        var launches = new List<System.Diagnostics.ProcessStartInfo>();
        string? copied = null;
        bool clipboardBusy = true;
        void Copy(string value)
        {
            if (clipboardBusy) throw new System.Runtime.InteropServices.ExternalException();
            copied = value;
        }
        using var panel = new BrowserPanel(launches.Add, Copy) { Dock = DockStyle.Fill };
        form.Controls.Add(panel);
        form.Show();
        var buttons = panel.Controls.OfType<TableLayoutPanel>().Single().Controls.OfType<FlowLayoutPanel>()
            .SelectMany(row => row.Controls.OfType<Button>()).ToArray();
        Button Button(string label) => buttons.Single(button => button.Text == label);
        var request = (TextBox)panel.Controls.Find("BrowserRequest", true).Single();
        var result = (TextBox)panel.Controls.Find("BrowserResult", true).Single();
        var path = panel.Controls.OfType<TableLayoutPanel>().Single().Controls.OfType<TextBox>()
            .Single(box => box.AccessibleName == "Extension folder");
        var sent = new List<Intent>();
        panel.Intent += sent.Add;
        var local = new BrowserView { Device = "host-a", Local = true, Draft = "{\"op\":\"tabs\"}",
            Status = new() { Installed = true, ExtensionPath = AppContext.BaseDirectory } };
        panel.Display(local);
        Check(request.Text == local.Draft && path.Visible && path.Text == local.Status.ExtensionPath &&
            Button("Show extension folder").Enabled, "Local extension path and draft");
        Button("Show extension folder").PerformClick();
        Check(Path.GetFileName(launches.Single().FileName) == "explorer.exe" &&
            launches.Single().ArgumentList.Single() == local.Status.ExtensionPath, "Folder shortcut changed its target");
        var copy = Button("Copy extensions address");
        copy.PerformClick();
        Check(copied == null && copy.Text == "Clipboard busy — retry", "Busy clipboard should allow a retry");
        clipboardBusy = false;
        copy.PerformClick();
        Check(copied == "chrome://extensions/" && copy.Text == "Address copied" && launches.Count == 1,
            "Copy must put the management address on the clipboard");
        Button("Connection test").PerformClick();
        panel.Display(local.Clone());
        Check(sent.Count == 0 && request.Text == "{\"op\":\"ping\"}", "A preset ran or a status render lost the edit");
        Button("Check connection").PerformClick();
        Check(sent.Single().BrowserSetup is { Install: false }, "Check must only query setup");
        Button("Run").PerformClick();
        Check(sent.Count == 2 && sent.Last().BrowserRun?.Json == request.Text, "Run used the wrong draft");
        var busy = local.Clone();
        busy.Busy = true;
        panel.Display(busy);
        Button("Run").PerformClick();
        Check(!request.Enabled && !Button("Check connection").Enabled && sent.Count == 2, "Busy allowed another operation");
        var remote = new BrowserView { Device = "host-b", Draft = "remote draft", Result = "test reply",
            Status = new() { Connected = true, ExtensionPath = @"C:\Other\AgentHaloBrowser" } };
        panel.Display(remote);
        Check(request.Text == remote.Draft && result.Text == remote.Result && !path.Visible && path.Text.Length == 0 &&
            !Button("Show extension folder").Visible && !copy.Visible, "Remote target inherited local setup");
        Button("List tabs").PerformClick();
        Check(request.Text == local.Draft && sent.Count == 2, "List tabs unexpectedly sent a request");
        Button("Run & authenticate").PerformClick();
        Check(sent.Count == 3 && sent.Last().BrowserRun?.Json == local.Draft, "Remote request skipped shared intent");
        remote.Status = null;
        panel.Display(remote);
        Check(!result.Bounds.IntersectsWith(request.Bounds) && result.Height >= result.Font.Height &&
            request.Height >= request.Font.Height, "Browser form fields are clipped");
        panel.Display(null);
        panel.Display(local);
        Check(copy.Visible && copy.Text == "Copy extensions address" && request.Text == local.Draft && result.Text == "" && panel.Visible, "Reopening used a stale target");
        Console.WriteLine($"Passed Windows browser setup, busy gate, drafts and target switching at {panel.DeviceDpi} DPI.");
    }
}

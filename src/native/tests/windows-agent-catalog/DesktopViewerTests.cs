using Muster.App;
using Muster.V1;

internal static class DesktopViewerTests
{
    internal static void Run()
    {
        using var host = new Form();
        using var remote = new Muster.Windows.RemoteWorkspace { Dock = DockStyle.Fill };
        host.Controls.Add(remote);
        host.Show();
        Intent? sent = null;
        remote.Intent += intent => sent = intent;
        var vm = new RemoteWorkspaceView { SelectedDevice = "test-host", CanConnect = true };
        vm.Devices.Add(new RemoteDevice { DeviceId = "test-host", Name = "Test host", Connected = true });
        Button Button(string label) => remote.Controls.OfType<FlowLayoutPanel>().SelectMany(p => p.Controls.OfType<Button>()).Single(b => b.Text == label);
        remote.Display(vm);
        if (sent != null || remote.Surface.TopLevelControl != host) throw new Exception("Selecting a peer must only prepare the embedded surface");
        Button("Connect").PerformClick();
        var handle = remote.Surface.Handle;
        if (sent?.DesktopSession?.ViewInWindow != (ulong)handle.ToInt64()) throw new Exception("Connect must use the child surface");
        vm.SessionOpen = true;
        vm.CanConnect = false;
        remote.Display(vm);
        remote.Hide();
        remote.Show();
        if (remote.Surface.Handle != handle || remote.FindForm() != host) throw new Exception("Switching tabs recreated the viewer");
        vm.Takeover = new DesktopTakeoverPrompt { Controller = "Test controller", Confirmation = 42 };
        sent = null;
        remote.Display(vm);
        if (sent != null || !remote.Controls.OfType<FlowLayoutPanel>().SelectMany(p => p.Controls.OfType<Label>())
                .Any(l => l.Visible && l.Text.Contains("Test controller"))) throw new Exception("Must display occupant and wait for explicit action");
        Button("Keep current session").PerformClick();
        if (sent?.EndDesktopSession != "test-host") throw new Exception("Cancellation must close only this request");
        Button("Disconnect and connect").PerformClick();
        if (sent?.DesktopSession?.ConfirmTakeover != 42) throw new Exception("Wrong confirmation");
        vm.Takeover = null;
        remote.Display(vm);
        Button("Disconnect").PerformClick();
        if (sent?.EndDesktopSession != "test-host") throw new Exception("Disconnect did not end the selected lease");
    }
}

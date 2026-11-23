using Muster.App;

namespace Muster.Windows;

internal sealed class RemoteWorkspace : UserControl
{
    sealed record Device(string Id, string Name, bool Online)
    {
        public override string ToString() => Online ? Name : $"{Name} · offline";
    }

    readonly ComboBox devices = new() { AccessibleName = "Paired desktop", DropDownStyle = ComboBoxStyle.DropDownList, Width = 230 };
    readonly Button connect = new() { Text = "Connect", AutoSize = true };
    readonly Button disconnect = new() { Text = "Disconnect", AutoSize = true };
    readonly Label status = new() { AutoSize = true, Anchor = AnchorStyles.Left, UseMnemonic = false };
    readonly Label occupant = new() { AutoSize = true, Anchor = AnchorStyles.Left, MaximumSize = new Size(460, 0), UseMnemonic = false };
    readonly FlowLayoutPanel choice = new() { Dock = DockStyle.Top, AutoSize = true, Visible = false };
    internal Panel Surface { get; } = new() { Dock = DockStyle.Fill, BackColor = Color.Black, AccessibleName = "Remote desktop" };
    internal event Action<Intent>? Intent;
    RemoteWorkspaceView? current;

    internal RemoteWorkspace()
    {
        var bar = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true, Padding = new Padding(0, 3, 0, 6) };
        bar.Controls.AddRange([new Label { Text = "Device", AutoSize = true, Anchor = AnchorStyles.Left }, devices, connect, disconnect, status]);
        var keep = new Button { Text = "Keep current session", AutoSize = true };
        var replace = new Button { Text = "Disconnect and connect", AutoSize = true };
        foreach (var b in new[] { connect, disconnect, keep, replace }) WorkspaceStyle.StyleButton(b);
        choice.Controls.AddRange([occupant, keep, replace]);
        Controls.Add(Surface);
        Controls.Add(choice);
        Controls.Add(bar);
        devices.SelectionChangeCommitted += (_, _) => {
            if (devices.SelectedItem is Device d)
            {
                connect.Enabled = false;
                Intent?.Invoke(new() { OpenRemote = new() { DeviceId = d.Id } });
            }
        };
        connect.Click += (_, _) => {
            if (current is { CanConnect: true } vm && devices.SelectedItem is Device d && d.Id == vm.SelectedDevice)
                Intent?.Invoke(new() { DesktopSession = new() { Device = vm.SelectedDevice, ViewInWindow = (ulong)Surface.Handle.ToInt64() } });
        };
        disconnect.Click += (_, _) => EndSession();
        keep.Click += (_, _) => EndSession();
        replace.Click += (_, _) => {
            if (current?.Takeover is {} prompt)
                Intent?.Invoke(new() { DesktopSession = new() { Device = current.SelectedDevice, ConfirmTakeover = prompt.Confirmation } });
        };
    }

    void EndSession()
    {
        if (current is { SessionOpen: true } vm) Intent?.Invoke(new() { EndDesktopSession = vm.SelectedDevice });
    }

    internal void Display(RemoteWorkspaceView? vm)
    {
        current = vm;
        var rows = vm?.Devices.Select(d => new Device(d.DeviceId, d.Name, d.Connected)).ToArray() ?? [];
        if (!devices.Items.Cast<Device>().SequenceEqual(rows))
        {
            devices.BeginUpdate();
            devices.Items.Clear();
            devices.Items.AddRange(rows);
            devices.EndUpdate();
        }
        devices.SelectedIndex = Array.FindIndex(rows, d => d.Id == vm?.SelectedDevice);
        bool open = vm?.SessionOpen == true;
        devices.Enabled = !open && rows.Length > 0;
        connect.Visible = !open;
        connect.Enabled = vm?.CanConnect == true;
        disconnect.Visible = open;
        status.Text = vm?.Status.Length > 0 ? vm.Status : rows.Length == 0 ? "No paired desktops" : "";
        choice.Visible = vm?.Takeover != null;
        if (vm?.Takeover is {} prompt)
            occupant.Text = $"{prompt.Controller} is using this desktop. Disconnect it and connect this device?";
        Surface.Visible = open && vm?.Takeover == null;
    }
}

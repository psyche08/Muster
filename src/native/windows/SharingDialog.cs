using Muster.V1;

namespace Muster.Windows;

internal sealed class SharingDialog : Form
{
    readonly CheckBox agent = Option("Share agent sessions and chats");
    readonly CheckBox terminal = Option("Share terminals");
    readonly CheckBox tasks = Option("Accept external scheduler tasks");
    readonly CheckBox locked = Option("Allow control while the screen is locked");
    readonly CheckBox clearPorts = Option("Clear saved local open ports");
    readonly TextBox origin = new() { Width = 400, AccessibleName = "Shared web origin", PlaceholderText = "https://example.org" };
    readonly TextBox apps = new() { Multiline = true, Width = 400, Height = 84,
        AccessibleName = "Allowed application identifiers", ScrollBars = ScrollBars.Vertical };
    static CheckBox Option(string text) => new() { AutoSize = true, Text = text };

    public SharingDialog(Policy original, bool basic)
    {
        Text = "Local service permissions";
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(470, 440);
        var fields = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoScroll = true,
            FlowDirection = FlowDirection.TopDown, WrapContents = false, Padding = new Padding(16) };
        agent.Checked = original.OfferAgent;
        terminal.Checked = original.OfferTerminal;
        tasks.Checked = original.OfferTasks;
        locked.Checked = original.AllowLockedControl;
        origin.Text = original.WebWindow?.Origin ?? "";
        apps.Lines = original.AxAllowlist.ToArray();
        fields.Controls.AddRange([agent, terminal, tasks, locked,
            new Label { Text = "Shared web origin (empty to close)", AutoSize = true }, origin,
            new Label { Text = "Allowed application identifiers, one per line", AutoSize = true }, apps]);
        if (basic && original.OpenPorts.Count > 0)
        {
            fields.Controls.Add(new Label { Text = "Saved open ports: " + string.Join(", ", original.OpenPorts),
                AutoSize = true, MaximumSize = new Size(400, 0) });
            fields.Controls.Add(clearPorts);
        }
        var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true,
            FlowDirection = FlowDirection.RightToLeft, Padding = new Padding(8) };
        var save = new Button { Text = "Save", DialogResult = DialogResult.OK, AutoSize = true };
        var cancel = new Button { Text = "Cancel", DialogResult = DialogResult.Cancel, AutoSize = true };
        buttons.Controls.AddRange([save, cancel]);
        Controls.Add(fields);
        Controls.Add(buttons);
        AcceptButton = save;
        CancelButton = cancel;
    }

    public Policy Apply(Policy original)
    {
        var policy = original.Clone();
        if (clearPorts.Checked) policy.OpenPorts.Clear();
        policy.OfferAgent = agent.Checked;
        policy.OfferTerminal = terminal.Checked;
        policy.OfferTasks = tasks.Checked;
        policy.AllowLockedControl = locked.Checked;
        string updatedOrigin = origin.Text.Trim();
        if ((original.WebWindow?.Origin ?? "") != updatedOrigin)
            policy.WebWindow = updatedOrigin.Length == 0 ? null : new() { Origin = updatedOrigin };
        policy.AxAllowlist.Clear();
        policy.AxAllowlist.Add(apps.Lines.Select(line => line.Trim()).Where(line => line.Length > 0));
        return policy;
    }
}

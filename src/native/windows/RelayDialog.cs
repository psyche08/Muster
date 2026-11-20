namespace Muster.Windows;

internal sealed class RelayDialog : Form
{
    readonly TextBox endpoint = new() { Dock = DockStyle.Top, AccessibleName = "Relay address" };
    public string Endpoint => endpoint.Text.Trim();

    public RelayDialog(string current)
    {
        Text = "Change local relay";
        StartPosition = FormStartPosition.CenterParent;
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MinimizeBox = MaximizeBox = false;
        AutoScaleMode = AutoScaleMode.Font;
        AutoSize = true;
        AutoSizeMode = AutoSizeMode.GrowAndShrink;

        var fields = new TableLayoutPanel { AutoSize = true, Dock = DockStyle.Fill,
            Padding = new Padding(12), ColumnCount = 1 };
        fields.Controls.Add(new Label { Text = "Relay address (host:port) for this device", AutoSize = true });
        endpoint.MinimumSize = new Size(340, 0);
        endpoint.Text = current;
        fields.Controls.Add(endpoint);
        var buttons = new FlowLayoutPanel { AutoSize = true, Anchor = AnchorStyles.Right, WrapContents = false };
        var apply = new Button { Text = "Use this relay", AutoSize = true, DialogResult = DialogResult.OK, Enabled = false };
        var cancel = new Button { Text = "Cancel", AutoSize = true, DialogResult = DialogResult.Cancel };
        endpoint.TextChanged += (_, _) => apply.Enabled = Endpoint.Length > 0 && Endpoint != current;
        buttons.Controls.Add(apply);
        buttons.Controls.Add(cancel);
        fields.Controls.Add(buttons);
        Controls.Add(fields);
        AcceptButton = apply;
        CancelButton = cancel;
        ActiveControl = endpoint;
    }
}

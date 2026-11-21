namespace Muster.Windows;

internal sealed class OpenPortsDialog : Form
{
    readonly TextBox rules = new() { Name = "PortRules", AccessibleName = "Open port rules",
        Multiline = true, AcceptsReturn = true, ScrollBars = ScrollBars.Vertical,
        WordWrap = false, Dock = DockStyle.Fill };
    public string[] Rules => rules.Lines.Select(line => line.Trim()).Where(line => line.Length > 0).ToArray();

    public OpenPortsDialog(IEnumerable<string> ports)
    {
        Text = "Edit open ports";
        StartPosition = FormStartPosition.CenterParent;
        MinimizeBox = MaximizeBox = false;
        AutoScaleMode = AutoScaleMode.Font;
        ClientSize = new Size(440, 260);
        MinimumSize = new Size(320, 220);

        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(12), RowCount = 3, ColumnCount = 1 };
        layout.ColumnStyles.Add(new(SizeType.Percent, 100));
        layout.RowStyles.Add(new(SizeType.AutoSize));
        layout.RowStyles.Add(new(SizeType.Percent, 100));
        layout.RowStyles.Add(new(SizeType.AutoSize));
        layout.Controls.Add(new Label { AutoSize = true, Text = "One rule per line, for example tcp/22." }, 0, 0);
        string[] initial = ports.ToArray();
        rules.Lines = initial;
        layout.Controls.Add(rules, 0, 1);
        var buttons = new FlowLayoutPanel { AutoSize = true, Anchor = AnchorStyles.Right, WrapContents = false };
        var save = new Button { Text = "Save", AutoSize = true, DialogResult = DialogResult.OK, Enabled = false };
        var cancel = new Button { Text = "Cancel", AutoSize = true, DialogResult = DialogResult.Cancel };
        rules.TextChanged += (_, _) => save.Enabled = !Rules.SequenceEqual(initial);
        buttons.Controls.Add(save);
        buttons.Controls.Add(cancel);
        layout.Controls.Add(buttons, 0, 2);
        Controls.Add(layout);
        CancelButton = cancel;
        ActiveControl = rules;
    }
}

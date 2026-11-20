namespace Muster.Windows;

internal sealed class PasteInviteDialog : Form
{
    readonly TextBox invite = new() { Dock = DockStyle.Fill, Multiline = true, ScrollBars = ScrollBars.Vertical,
        AccessibleName = "Invitation text", PlaceholderText = "Paste the invitation text here" };
    public string Invitation => invite.Text;

    public PasteInviteDialog()
    {
        Text = "Join with an invite";
        StartPosition = FormStartPosition.CenterParent;
        MinimizeBox = MaximizeBox = false;
        ClientSize = new Size(480, 170);
        Padding = new Padding(12);
        var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, FlowDirection = FlowDirection.RightToLeft, AutoSize = true };
        var cancel = new Button { Text = "Cancel", AutoSize = true, DialogResult = DialogResult.Cancel };
        var join = new Button { Text = "Join", AutoSize = true, Enabled = false, DialogResult = DialogResult.OK };
        invite.TextChanged += (_, _) => join.Enabled = !string.IsNullOrWhiteSpace(invite.Text);
        buttons.Controls.Add(cancel);
        buttons.Controls.Add(join);
        Controls.Add(invite);
        Controls.Add(buttons);
        AcceptButton = join;
        CancelButton = cancel;
        ActiveControl = invite;
    }
}

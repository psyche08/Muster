using Muster.App;

namespace Muster.Windows;

internal sealed class AccessDialog : Form
{
    readonly List<(CheckBox Item, string Scope, CheckBox? Required)> grants = new();
    public IEnumerable<string> Scopes =>
        grants.Where(row => row.Item.Checked && row.Required?.Checked != false).Select(row => row.Scope);

    public AccessDialog(string title, string identity, AccessView? access)
    {
        Text = title;
        StartPosition = FormStartPosition.CenterParent;
        Size = new Size(540, 600);
        var sections = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown,
                                             WrapContents = false, AutoScroll = true, Padding = new Padding(12) };
        sections.Controls.Add(new Label { Text = identity, AutoSize = true, MaximumSize = new Size(475, 0) });
        CheckBox Add(AccessItem item, CheckBox? required)
        {
            var box = new CheckBox { Text = item.Label, Checked = item.Granted, AutoSize = true };
            if (required != null)
            {
                box.Enabled = required.Checked;
                required.CheckedChanged += (_, _) => box.Enabled = required.Checked;
            }
            sections.Controls.Add(box);
            grants.Add((box, item.Scope, required));
            return box;
        }
        if (access != null)
        {
            foreach (var section in access.Sections)
            {
                sections.Controls.Add(new Label { Text = section.Title, AutoSize = true });
                var toggle = section.Toggle == null ? null : Add(section.Toggle, null);
                foreach (var item in section.Items)
                    Add(item, item.RequiresToggle ? toggle : null);
            }
        }
        var buttons =
            new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true, FlowDirection = FlowDirection.RightToLeft };
        var cancel = new Button { Text = "Cancel", DialogResult = DialogResult.Cancel };
        var save = new Button { Text = title, DialogResult = DialogResult.OK, AutoSize = true };
        buttons.Controls.Add(cancel);
        buttons.Controls.Add(save);
        Controls.Add(sections);
        Controls.Add(buttons);
        CancelButton = cancel;
    }
}

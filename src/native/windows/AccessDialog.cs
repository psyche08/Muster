using Muster.App;

namespace Muster.Windows;

internal sealed class AccessDialog : Form
{
    readonly List<(CheckBox Item, string Scope, CheckBox? Required)> grants = new();
    public IEnumerable<string> Scopes =>
        grants.Where(row => row.Item.Checked && (row.Scope != "multica.control" || row.Required?.Checked != false)).Select(row => row.Scope);

    public bool ClearSavedPortRules { get; private set; }
    public string ChangedRole { get; private set; } = "";

    public AccessDialog(string title, string identity, AccessView? access, string? memberRole = null)
    {
        Text = title;
        StartPosition = FormStartPosition.CenterParent;
        Size = new Size(540, 600);
        var sections = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown,
                                             WrapContents = false, AutoScroll = true, Padding = new Padding(12) };
        sections.Controls.Add(new Label { Text = identity, AutoSize = true, MaximumSize = new Size(475, 0) });
        if (memberRole != null)
        {
            sections.Controls.Add(new Label { Text = "Network role", AutoSize = true });
            var role = new ComboBox { AccessibleName = "Network role", DropDownStyle = ComboBoxStyle.DropDownList };
            role.Items.AddRange(["member", "admin"]);
            role.SelectedItem = memberRole;
            role.SelectedIndexChanged += (_, _) =>
                ChangedRole = role.SelectedItem is string choice && choice != memberRole ? choice : "";
            sections.Controls.Add(role);
        }
        CheckBox Add(FlowLayoutPanel panel, AccessItem item, CheckBox? required)
        {
            var box = new CheckBox { Text = item.Label, Checked = item.Granted, AutoSize = true };
            if (required != null)
            {
                box.Enabled = required.Checked;
                required.CheckedChanged += (_, _) => box.Enabled = required.Checked;
            }
            panel.Controls.Add(box);
            grants.Add((box, item.Scope, required));
            return box;
        }
        if (access != null)
        {
            foreach (var section in access.Sections)
            {
                var group = new GroupBox { Text = section.Title, AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink,
                    Padding = new Padding(10, 6, 10, 8), MinimumSize = new Size(440, 0) };
                var items = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown,
                    WrapContents = false, AutoSizeMode = AutoSizeMode.GrowAndShrink };
                group.Controls.Add(items);
                sections.Controls.Add(group);
                var toggle = section.Toggle == null ? null : Add(items, section.Toggle, null);
                foreach (var item in section.Items)
                    Add(items, item, item.RequiresToggle ? toggle : null);
                if (section.Id == "network" && !access.NetworkAdvanced && access.HasSavedPortRules && memberRole != null)
                {
                    var clear = new CheckBox { Text = "Clear saved port rules", AutoSize = true };
                    clear.CheckedChanged += (_, _) => ClearSavedPortRules = clear.Checked;
                    items.Controls.Add(clear);
                }
                bool provider = section.Id is "chats" or "agents";
                if (provider || section.Id == "network" && access.NetworkAdvanced)
                {
                    var entry = new FlowLayoutPanel { AutoSize = true, WrapContents = false };
                    var input = new TextBox { PlaceholderText = provider ? "Provider identifier" : "tcp/443", Width = 160 };
                    var add = new Button { Text = provider ? "Add provider" : "Add port", AutoSize = true };
                    if (provider && toggle != null)
                    {
                        entry.Enabled = toggle.Checked;
                        toggle.CheckedChanged += (_, _) => entry.Enabled = toggle.Checked;
                    }
                    add.Click += (_, _) =>
                    {
                        var value = input.Text.Trim();
                        if (value.Length == 0) return;
                        var scope = (provider ? section.Id : "net") + ":" + value;
                        var existing = grants.FindIndex(row => row.Scope == scope);
                        if (existing >= 0) grants[existing].Item.Checked = true;
                        else Add(items, new AccessItem { Scope = scope, Label = value, Granted = true }, provider ? toggle : null);
                        input.Clear();
                    };
                    entry.Controls.AddRange([input, add]);
                    items.Controls.Add(entry);
                }
                if (section.Footer.Length > 0)
                    items.Controls.Add(new Label { Text = section.Footer, AutoSize = true, MaximumSize = new Size(420, 0) });
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

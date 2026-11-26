using Muster.V1;

namespace Muster.Windows;

internal sealed class ServiceGrantsDialog : Form
{
    readonly ComboBox audience = new() { DropDownStyle = ComboBoxStyle.DropDownList, Dock = DockStyle.Fill };
    readonly FlowLayoutPanel rows = new() { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown,
        WrapContents = false, AutoScroll = true };
    readonly List<DeviceGrant> rules;
    readonly MemberView[] members;
    readonly string wholeScope, placeholder;

    public GrantSet? Grants => audience.SelectedIndex switch
    {
        1 => new() { AllDevices = true, Rules = { rules } },
        2 => new() { Rules = { rules } },
        _ => null,
    };

    public ServiceGrantsDialog(string title, string wholeScope, string placeholder,
                               IEnumerable<MemberView> members, GrantSet? initial)
    {
        this.wholeScope = wholeScope;
        this.placeholder = placeholder;
        this.members = members.ToArray();
        rules = initial?.Rules.Select(rule => rule.Clone()).ToList() ?? [];
        Text = title + " access rules";
        StartPosition = FormStartPosition.CenterParent;
        ClientSize = new Size(500, 520);
        MinimumSize = new Size(425, 300);
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(12),
            RowCount = 3, ColumnCount = 1 };
        layout.ColumnStyles.Add(new(SizeType.Percent, 100));
        layout.RowStyles.Add(new(SizeType.AutoSize));
        layout.RowStyles.Add(new(SizeType.Percent, 100));
        layout.RowStyles.Add(new(SizeType.AutoSize));
        audience.AccessibleName = "Allow access for";
        audience.Items.AddRange(["Existing policy", wholeScope == "desktop.mouse" ? "All paired desktops" : "All paired devices", "Assigned devices"]);
        audience.SelectedIndex = initial == null ? 0 : initial.AllDevices ? 1 : 2;
        audience.SelectedIndexChanged += (_, _) => RenderRules();
        layout.Controls.Add(audience, 0, 0);
        layout.Controls.Add(rows, 0, 1);
        var buttons = new FlowLayoutPanel { AutoSize = true, Anchor = AnchorStyles.Right, WrapContents = false };
        var save = new Button { Text = "Save", AutoSize = true, DialogResult = DialogResult.OK };
        var cancel = new Button { Text = "Cancel", AutoSize = true, DialogResult = DialogResult.Cancel };
        buttons.Controls.Add(save);
        buttons.Controls.Add(cancel);
        layout.Controls.Add(buttons, 0, 2);
        Controls.Add(layout);
        AcceptButton = save;
        CancelButton = cancel;
        RenderRules();
    }

    void RenderRules()
    {
        rows.SuspendLayout();
        while (rows.Controls.Count > 0) rows.Controls[0].Dispose();
        if (audience.SelectedIndex == 2)
        {
            foreach (var rule in rules.Where(rule => placeholder.Length > 0 || rule.Scope == wholeScope))
                AddRuleRow(rule);
            var add = new Button { Text = "Add rule", AutoSize = true };
            add.Click += (_, _) =>
            {
                rules.Add(new DeviceGrant { Scope = wholeScope });
                RenderRules();
            };
            rows.Controls.Add(add);
        }
        rows.ResumeLayout();
    }

    void AddRuleRow(DeviceGrant rule)
    {
        var group = new FlowLayoutPanel { AutoSize = true, FlowDirection = FlowDirection.TopDown,
            WrapContents = false, Padding = new Padding(4, 8, 4, 8) };
        if (placeholder.Length == 0)
            group.Controls.Add(new Label { Text = "Whole network", AutoSize = true });
        else
        {
            var selector = new ComboBox { AccessibleName = "Service selector", Width = 365,
                DropDownStyle = wholeScope is "desktop.mouse" or "terminals" ? ComboBoxStyle.DropDownList : ComboBoxStyle.DropDown };
            string[] choices = wholeScope switch
            {
                "desktop.mouse" => ["desktop.view", "desktop.mouse"],
                "terminals" => ["terminals", "terminal.view", "terminal.control"],
                _ => [wholeScope]
            };
            selector.Items.AddRange(choices);
            if (!selector.Items.Contains(rule.Scope)) selector.Items.Add(rule.Scope);
            selector.SelectedItem = rule.Scope;
            if (selector.DropDownStyle == ComboBoxStyle.DropDown)
                group.Controls.Add(new Label { AutoSize = true, Text = placeholder });
            selector.TextChanged += (_, _) => rule.Scope = selector.Text.Trim();
            group.Controls.Add(selector);
            if (wholeScope == "desktop.mouse")
            {
                var keyboard = new CheckBox { Text = "Keyboard on unlocked desktop", AutoSize = true,
                    Checked = rule.Keyboard, Enabled = rule.Scope == "desktop.mouse" };
                keyboard.CheckedChanged += (_, _) => rule.Keyboard = keyboard.Checked;
                selector.TextChanged += (_, _) =>
                {
                    keyboard.Enabled = selector.Text.Trim() == "desktop.mouse";
                    if (!keyboard.Enabled) keyboard.Checked = false;
                };
                group.Controls.Add(keyboard);
            }
        }
        foreach (var member in members)
        {
            var box = new CheckBox { Text = member.Name, AutoSize = true,
                Checked = rule.DeviceIds.Contains(member.DeviceId) };
            box.CheckedChanged += (_, _) =>
            {
                rule.DeviceIds.Remove(member.DeviceId);
                if (box.Checked) rule.DeviceIds.Add(member.DeviceId);
            };
            group.Controls.Add(box);
        }
        var remove = new Button { Text = "Remove rule", AutoSize = true };
        remove.Click += (_, _) =>
        {
            rules.Remove(rule);
            RenderRules();
        };
        group.Controls.Add(remove);
        rows.Controls.Add(group);
    }
}

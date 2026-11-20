using Muster.App;
using Muster.V1;

namespace Muster.Windows;

internal sealed class AgentSessionDialog : Form
{
    readonly ComboBox device = Picker("Device"), provider = Picker("Provider");
    readonly TextBox directory = new() { Name = "Directory", Dock = DockStyle.Fill };
    readonly TextBox prompt = new() { Name = "Prompt", Dock = DockStyle.Fill, Multiline = true,
                                      AcceptsReturn = true, ScrollBars = ScrollBars.Vertical };
    readonly bool desktop;

    static ComboBox Picker(string name) => new()
        { Name = name, Dock = DockStyle.Fill, DropDownStyle = ComboBoxStyle.DropDownList };

    public AgentSessionDialog(AgentsView agents, bool desktop)
    {
        this.desktop = desktop;
        Text = desktop ? "New desktop session" : "New CLI session";
        StartPosition = FormStartPosition.CenterParent;
        MinimizeBox = MaximizeBox = false;
        ShowInTaskbar = false;
        AutoScaleMode = AutoScaleMode.Font;
        ClientSize = new Size(480, desktop ? 410 : 270);
        MinimumSize = Size;
        var fields = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(14),
                                            ColumnCount = 1, AutoScroll = true };
        fields.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        void Add(Control control, bool stretch = false)
        {
            int row = fields.RowCount++;
            fields.RowStyles.Add(new RowStyle(stretch ? SizeType.Percent : SizeType.AutoSize, stretch ? 100 : 0));
            fields.Controls.Add(control, 0, row);
        }
        void Field(string label, Control control, bool stretch = false)
        {
            Add(new Label { Text = label, AutoSize = true });
            control.AccessibleName = label;
            Add(control, stretch);
        }
        Field(desktop ? "Mac" : "Device", device);
        Field(desktop ? "App" : "CLI agent", provider);
        Field("Absolute directory on the selected device", directory);
        if (desktop) Field("First message", prompt, true);
        Add(new Label { Text = desktop
            ? "Create and send opens a Code session on the selected Mac and sends the first message."
            : "The directory must exist and the CLI agent must be installed and signed in on the selected device.",
            AutoSize = true, MaximumSize = new Size(430, 0) });

        var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true,
                                           FlowDirection = FlowDirection.RightToLeft, Padding = new Padding(6) };
        var submit = new Button { Name = "Submit", Text = desktop ? "Create and send" : "Create",
                                  DialogResult = DialogResult.OK, AutoSize = true };
        var cancel = new Button { Text = "Cancel", DialogResult = DialogResult.Cancel, AutoSize = true };
        buttons.Controls.Add(submit);
        buttons.Controls.Add(cancel);
        Controls.Add(fields);
        Controls.Add(buttons);
        CancelButton = cancel;
        if (!desktop) AcceptButton = submit;

        var draft = desktop ? agents.NativeCreateDraft : null;
        device.DisplayMember = nameof(MemberView.Name);
        device.Items.AddRange((desktop ? agents.NativeCreateHosts : agents.CreateHosts).ToArray());
        provider.Items.AddRange((desktop ? agents.NativeCreateApps : agents.CreateAgents).ToArray());
        if (draft != null)
        {
            device.SelectedItem = device.Items.Cast<MemberView>().FirstOrDefault(m => m.DeviceId == draft.DeviceId);
            provider.SelectedItem = draft.Provider;
            directory.Text = draft.Directory;
            prompt.Text = draft.Prompt;
        }
        else
        {
            if (device.Items.Count > 0) device.SelectedIndex = 0;
            if (provider.Items.Count > 0) provider.SelectedIndex = 0;
        }
        void EnableSubmit() => submit.Enabled = device.SelectedItem != null && provider.SelectedItem != null
            && !string.IsNullOrWhiteSpace(directory.Text) && (!desktop || !string.IsNullOrWhiteSpace(prompt.Text));
        device.SelectedIndexChanged += (_, _) => EnableSubmit();
        provider.SelectedIndexChanged += (_, _) => EnableSubmit();
        directory.TextChanged += (_, _) => EnableSubmit();
        prompt.TextChanged += (_, _) => EnableSubmit();
        EnableSubmit();
    }

    public Intent Request()
    {
        string target = ((MemberView)device.SelectedItem!).DeviceId;
        string app = (string)provider.SelectedItem!;
        return desktop
            ? new Intent { NativeCreate = new() { DeviceId = target, Provider = app, Directory = directory.Text, Prompt = prompt.Text } }
            : new Intent { OrcaCreate = new() { DeviceId = target, Agent = app, Directory = directory.Text } };
    }
}

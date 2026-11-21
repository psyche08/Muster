using Muster.App;

namespace Muster.Windows;

internal sealed class WorkspaceWelcome : UserControl
{
    readonly PictureBox mark = new() { SizeMode = PictureBoxSizeMode.Zoom, TabStop = false };
    readonly Label heading = new() { AutoSize = true, TextAlign = ContentAlignment.MiddleCenter };
    readonly Label description = new() { AutoSize = true, TextAlign = ContentAlignment.MiddleCenter };
    readonly Button create = new() { Text = "New chat", AutoSize = true };
    readonly TableLayoutPanel card = new() { AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink,
        ColumnCount = 1, BackColor = Color.Transparent };
    public event Action? NewChat;

    public WorkspaceWelcome()
    {
        Dock = DockStyle.Fill;
        BackColor = WorkspaceStyle.Canvas;
        using var resource = typeof(WorkspaceWelcome).Assembly.GetManifestResourceStream("Muster.WorkspaceMark")!;
        using var image = Image.FromStream(resource);
        mark.Image = new Bitmap(image);
        mark.Size = new Size(76, 76);
        heading.Font = new Font("Segoe UI", 19, FontStyle.Bold);
        heading.ForeColor = WorkspaceStyle.Ink;
        description.ForeColor = WorkspaceStyle.Muted;
        description.MaximumSize = new Size(340, 0);
        WorkspaceStyle.StyleButton(create);
        create.BackColor = WorkspaceStyle.Selected;
        create.ForeColor = WorkspaceStyle.SelectedText;
        create.Click += (_, _) => NewChat?.Invoke();
        foreach (Control control in new Control[] { mark, heading, description, create })
        {
            control.Anchor = AnchorStyles.None;
            control.Margin = new Padding(4, 4, 4, 6);
            card.Controls.Add(control, 0, card.RowCount++);
        }
        Controls.Add(card);
        card.SizeChanged += (_, _) => CenterCard();
        SizeChanged += (_, _) =>
        {
            description.MaximumSize = new Size(Math.Max(1, Math.Min(LogicalToDeviceUnits(340), ClientSize.Width - LogicalToDeviceUnits(24))), 0);
            CenterCard();
        };
        Disposed += (_, _) => { mark.Image.Dispose(); heading.Font.Dispose(); };
    }

    void CenterCard() => card.Location = new Point(Math.Max(0, (ClientSize.Width - card.Width) / 2),
                                                    Math.Max(0, (ClientSize.Height - card.Height) / 2));

    public void Display(bool chats, DesktopChatsView? directory)
    {
        string? provider = directory?.Choices.FirstOrDefault(p => p.Id == directory.Provider)?.Label;
        heading.Text = chats ? string.IsNullOrEmpty(provider) ? "Your chats" : "Chat with " + provider : "Your agents";
        description.Text = chats ? "Choose a conversation on the left, or start a new chat."
                                 : "Choose a session on the left to read the conversation and reply.";
        create.Visible = chats;
        create.Enabled = directory is { DeviceId.Length: > 0, Opening.Length: 0 };
    }
}

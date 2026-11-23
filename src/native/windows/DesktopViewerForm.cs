using Muster.V1;

namespace Muster.Windows;

internal sealed class DesktopViewerForm : Form
{
    internal Panel Surface { get; } = new() { Dock = DockStyle.Fill, BackColor = Color.Black, AccessibleName = "Remote desktop" };
    internal event Action<ulong>? TakeoverRequested;
    readonly FlowLayoutPanel choice = new() { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown,
        WrapContents = false, AutoScroll = true, Padding = new Padding(20), Visible = false };
    readonly Label occupant = new() { AutoSize = true, MaximumSize = new Size(420, 0), UseMnemonic = false };
    DesktopTakeoverPrompt? current;

    internal DesktopViewerForm(string name)
    {
        Text = $"Desktop · {name}";
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(960, 540);
        MinimumSize = new Size(480, 300);
        var keep = new Button { Text = "Keep current session", AutoSize = true };
        var replace = new Button { Text = "Disconnect and connect", AutoSize = true };
        keep.Click += (_, _) => Close();
        replace.Click += (_, _) => {
            if (current is {} prompt)
            {
                Display(null);
                TakeoverRequested?.Invoke(prompt.Confirmation);
            }
        };
        choice.Controls.AddRange([occupant, keep, replace]);
        Controls.Add(Surface);
        Controls.Add(choice);
        choice.BringToFront();
    }

    internal void Display(DesktopTakeoverPrompt? prompt)
    {
        current = prompt;
        if (prompt != null)
            occupant.Text = $"{prompt.Controller} is using this desktop. Disconnect it and connect this device?";
        choice.Visible = prompt != null;
    }
}

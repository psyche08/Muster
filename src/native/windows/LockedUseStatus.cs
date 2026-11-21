using Muster.App;

namespace Muster.Windows;

internal sealed class LockedUseStatus : TableLayoutPanel
{
    readonly Label message = new() { Name = "LockedUseMessage", AutoSize = true,
        Anchor = AnchorStyles.Left | AnchorStyles.Right, UseMnemonic = false };
    readonly Button cancel = new() { Text = "Cancel", AccessibleName = "Cancel Locked Use operation", AutoSize = true };
    public event Action<Intent>? Intent;

    public LockedUseStatus()
    {
        Name = "LockedUseStatus";
        AccessibleName = "Locked Use";
        Dock = DockStyle.Bottom;
        Visible = false;
        AutoSize = true;
        AutoSizeMode = AutoSizeMode.GrowAndShrink;
        ColumnCount = 2;
        ColumnStyles.Add(new(SizeType.Percent, 100));
        ColumnStyles.Add(new(SizeType.AutoSize));
        RowStyles.Add(new(SizeType.AutoSize));
        Controls.Add(message, 0, 0);
        Controls.Add(cancel, 1, 0);
        cancel.Click += (_, _) =>
        {
            if (cancel.Visible && cancel.Enabled) Intent?.Invoke(new() { CancelLockedUse = new() });
        };
    }

    public void Display(ChatView? chat)
    {
        message.Text = chat?.LockedUseStatus ?? "";
        cancel.Visible = chat?.CanCancelLockedUse == true && message.Text.Length > 0;
        Visible = message.Text.Length > 0;
    }
}

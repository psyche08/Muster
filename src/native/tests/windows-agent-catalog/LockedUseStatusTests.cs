using Muster.App;
using Muster.Windows;

internal static class LockedUseStatusTests
{
    public static void Run(string? captures)
    {
        static void Check(bool ok, string name)
        {
            if (!ok) throw new InvalidOperationException(name);
        }
        using var form = new Form { ShowInTaskbar = false, StartPosition = FormStartPosition.Manual,
            Location = new Point(-20000, -20000), ClientSize = new Size(470, 260) };
        var status = new LockedUseStatus();
        form.Controls.Add(status);
        form.Show();
        var intents = new List<Intent>();
        status.Intent += intents.Add;
        var message = status.Controls.OfType<Label>().Single();
        var cancel = status.Controls.OfType<Button>().Single();
        status.Display(null);
        Check(!status.Visible, "no status when chat is absent");
        var chat = new ChatView { LockedUseStatus = "Confirm this operation on your device", CanCancelLockedUse = true };
        status.Display(chat);
        Check(status.Visible && cancel.Visible && message.Text == chat.LockedUseStatus, "waiting for confirmation");
        cancel.PerformClick();
        Check(intents.Single().ItCase == Intent.ItOneofCase.CancelLockedUse && status.Visible, "cancel only emits an intent");
        chat.LockedUseStatus = "Mac operation in progress; waiting for confirmed relock";
        status.Display(chat);
        Check(message.Text == chat.LockedUseStatus && message.Right <= cancel.Left &&
            message.Height >= message.GetPreferredSize(new Size(message.Width, 0)).Height &&
            status.ClientRectangle.Contains(cancel.Bounds), "waiting for relock fits narrow window");
        if (captures != null)
        {
            using var image = new Bitmap(status.Width, status.Height);
            status.DrawToBitmap(image, status.ClientRectangle);
            image.Save(Path.Combine(captures, "locked-use-status.png"), System.Drawing.Imaging.ImageFormat.Png);
        }
        chat.CanCancelLockedUse = false;
        status.Display(chat);
        cancel.PerformClick();
        Check(message.Visible && !cancel.Visible && intents.Count == 1, "status without cancellation");
        chat.LockedUseStatus = "";
        status.Display(chat);
        Check(!status.Visible && !cancel.Visible && message.Text.Length == 0, "completed operation clears status");
        Console.WriteLine($"Passed Locked Use message, cancellation intent, removal and layout at {form.DeviceDpi} DPI.");
    }
}

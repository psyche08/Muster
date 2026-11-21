using System.Drawing.Imaging;
using Muster.App;
using Muster.V1;
using Muster.Windows;

internal static class QueuedMessagesTests
{
    public static void Run(string? captures)
    {
        static void Check(bool ok, string name)
        {
            if (!ok) throw new InvalidOperationException(name);
        }
        using var form = new Form { ShowInTaskbar = false, StartPosition = FormStartPosition.Manual,
            Location = new Point(-20000, -20000), ClientSize = new Size(680, 410) };
        var panel = new QueuedMessages();
        form.Controls.Add(panel);
        form.Show();
        var intents = new List<Intent>();
        panel.Intent += intents.Add;
        var list = (ListBox)panel.Controls.Find("QueuedMessageList", true).Single();
        var prompt = (TextBox)panel.Controls.Find("QueuedPrompt", true).Single();
        var error = (TextBox)panel.Controls.Find("QueueError", true).Single();
        var options = prompt.Parent!.Controls.OfType<FlowLayoutPanel>().Single();
        Button Option(string label) => options.Controls.OfType<Button>().Single(b => b.Text == label);
        var first = new PendingInteraction { Id = "queued:first", Prompt = "Check the example report",
            Options = { "Send now", "Delete" } };
        var second = first.Clone();
        second.Id = "queued:second";
        second.Options.Remove("Send now");
        var chat = new ChatView { DeviceId = "host-a", Session = new() { Id = "session" },
            Queue = new() { Messages = { first, second } } };
        panel.Display(chat);
        Check(panel.Visible && list.Items.Count == 2 && list.SelectedIndex == 0 &&
            prompt.Text == first.Prompt && prompt.ReadOnly, "visible queue with complete selected prompt");
        Check(prompt.Height >= prompt.Font.Height && options.Bottom <= options.Parent!.ClientSize.Height &&
            list.Height >= list.ItemHeight * 2, "queue viewport and actions fit");
        Option("Send now").PerformClick();
        Check(intents.Single().AgentRespond is { Interaction: "queued:first", Option: "Send now" },
            "send now carries projected identity and option");
        Check(list.Items.Count == 2, "action alone cannot remove the row");
        list.SelectedIndex = 1;
        Check(options.Controls.Count == 1, "only selected row options");
        Option("Delete").PerformClick();
        Check(intents.Last().AgentRespond is { Interaction: "queued:second", Option: "Delete" }, "equal prompts on different rows");
        var reordered = chat.Clone();
        reordered.Queue.Messages.RemoveAt(0);
        reordered.Queue.Messages.Add(first);
        panel.Display(reordered);
        Check(list.SelectedIndex == 0 && ((PendingInteraction)list.SelectedItem!).Id == second.Id, "stable selection after reorder");
        if (captures != null)
        {
            using var bitmap = new Bitmap(panel.Width, panel.Height);
            panel.DrawToBitmap(bitmap, panel.ClientRectangle);
            bitmap.Save(Path.Combine(captures, "queued-messages.png"), ImageFormat.Png);
        }
        var busy = reordered.Clone();
        busy.Queue.Submitting = true;
        panel.Display(busy);
        Option("Delete").PerformClick();
        Check(!Option("Delete").Enabled && intents.Count == 2, "submission gated");
        var unknown = reordered.Clone();
        unknown.Queue.OutcomeUnknown = true;
        unknown.Queue.Error = "Fixture could not confirm the operation";
        panel.Display(unknown);
        Option("Delete").PerformClick();
        Check(!Option("Delete").Enabled && intents.Count == 2 && error.Visible && error.Text == unknown.Queue.Error,
            "unknown outcome remains gated and readable");
        var removed = reordered.Clone();
        removed.Queue.Messages.RemoveAt(0);
        panel.Display(removed);
        Check(list.SelectedIndex == -1 && options.Controls.Count == 0, "removing selected row cannot retarget action");
        var other = chat.Clone();
        other.DeviceId = "host-b";
        other.Queue.Messages[0].Options.Clear();
        other.Queue.Messages[0].Prompt = "Read-only fixture\r\nsecond line";
        panel.Display(other);
        Check(list.SelectedIndex == 0 && prompt.Text == other.Queue.Messages[0].Prompt && options.Controls.Count == 0,
            "scope change and read-only multiline prompt");
        unknown.Queue.Messages.Clear();
        panel.Display(unknown);
        Check(panel.Visible && error.Visible && !list.Visible && error.Height >= error.Font.Height, "empty queue with error");
        panel.Display(null);
        Check(!panel.Visible && list.Items.Count == 0 && error.Text.Length == 0, "leaving the conversation");
        Console.WriteLine($"Passed provider queue identity, options, outcome gates, scope and layout at {form.DeviceDpi} DPI.");
    }
}

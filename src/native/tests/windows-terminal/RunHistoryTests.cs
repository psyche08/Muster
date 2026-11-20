using Muster.App;
using Muster.Windows;

internal static class RunHistoryTests
{
    static void Check(bool ok, string message)
    {
        if (!ok) throw new InvalidOperationException(message);
    }

    public static void Run(string[] args)
    {
        var chat = new ChatView { DeviceId = "fixture", Session = new() { Id = "conversation" } };
        var pending = new CodeRunView { Run = new() { Id = "pending", Code = "echo example", State = "running" } };
        var finished = new CodeRunView { Run = new() { Id = "done", Code = "exit 7", Language = "sh",
            Cwd = "/fixture", State = "completed", Output = "example output", Truncated = true, ExitKnown = true,
            ExitCode = 7 }, CanReturn = true, Delivery = "ready" };
        chat.CodeRuns.Add(new[] { pending, finished });
        var sent = new List<Intent>();
        using var form = new RunHistoryDialog(chat, sent.Add);
        form.Show();
        Application.DoEvents();
        var list = (ComboBox)form.Controls.Find("Runs", true).Single();
        var details = (TextBox)form.Controls.Find("RunDetails", true).Single();
        var button = (Button)form.Controls.Find("ReturnResult", true).Single();
        Check(list.SelectedIndex == 0 && !button.Visible && !details.Text.Contains("Exit code"),
            "Unknown exit shown as a completed result");
        list.SelectedIndex = 1;
        foreach (var word in new[] { "/fixture", "Language: sh", "exit 7", "example output", "Exit code: 7",
                                     "Execution: completed", "Result delivery: ready", "Output (truncated)" })
            Check(details.Text.Contains(word), $"Missing detail {word}");
        if (args is ["--screenshots", var dir])
        {
            using var image = new Bitmap(form.Width, form.Height);
            form.DrawToBitmap(image, new Rectangle(Point.Empty, image.Size));
            image.Save(Path.Combine(dir, "recent-runs.png"));
        }
        var next = chat.Clone();
        next.CodeRuns.Clear();
        next.CodeRuns.Add(new[] { finished.Clone(), pending.Clone() });
        form.Display(next);
        Check(list.SelectedIndex == 0 && sent.Count == 0 && button.Visible, "Inspection changed selection or sent an intent");
        button.PerformClick();
        Check(sent.Count == 1 && sent[0].CodeAction is { Id: "done", Action: CodeAction.Types.Action.Return },
            "Manual return did not address the selected run");
        var rejected = next.Clone();
        rejected.CodeRuns[0].CanReturn = false;
        rejected.CodeRuns[0].Delivery = "rejected";
        rejected.CodeRuns[0].Error = "fixture refusal";
        rejected.CodeError = "fixture conversation error";
        form.Display(rejected);
        button.PerformClick();
        Check(!button.Visible && sent.Count == 1 && details.Text.Contains("Result delivery: rejected")
            && details.Text.Contains("fixture refusal") && details.Text.Contains(rejected.CodeError), "Stale return control or error");
        var removed = next.Clone();
        removed.CodeRuns.RemoveAt(0);
        form.Display(removed);
        Check(list.SelectedIndex == -1 && !details.Text.Contains("exit 7") && !button.Visible, "Selected a replacement run");
        var other = chat.Clone();
        other.DeviceId = "other-fixture";
        form.Display(other);
        Check(!form.Visible && sent.Count == 1, "History remained bound across devices");
        using var second = new RunHistoryDialog(chat, sent.Add);
        second.Show();
        other = chat.Clone();
        other.Session.Id = "other-chat";
        second.Display(other);
        Check(!second.Visible, "History remained bound across conversations");
        Console.WriteLine("Passed recent runs, scoped selection, live refresh and explicit result return.");
    }
}

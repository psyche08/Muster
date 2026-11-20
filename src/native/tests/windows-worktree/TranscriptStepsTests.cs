using Muster.App;
using Muster.Windows;

internal static class TranscriptStepsTests
{
    public static void Run()
    {
        static void Check(bool yes, string name)
        {
            if (!yes) throw new InvalidOperationException(name);
        }
        var view = new ChatView { DeviceId = "one", Session = new() { Id = "sample" }, Loading = true };
        var steps = new TranscriptSteps();
        steps.Display(view);
        Check(steps.Lines().Single() == "Loading conversation…", "loading a conversation");
        view.Loading = false;
        Check(steps.Lines().Single() == "No messages yet", "observed empty");
        view.Session.TranscriptNotice = "History is unavailable on this source.";
        Check(steps.Lines().Single() == view.Session.TranscriptNotice, "transcript notice");
        view.LoadError = "Cannot read history";
        Check(!steps.Lines().Any(), "history error must not have empty placeholder");
        view.MessageCount = 8;
        view.Timeline.Add(new ChatBlock { Id = "prompt", Kind = "message", Message = new() { Role = "user", Text = "Review A & B" } });
        var first = new ChatBlock { Id = "first", Kind = "working", Title = "Worked", Details = "unloaded" };
        view.Timeline.Add(first);
        var sent = new List<Intent>();
        int updates = 0;
        ContextMenuStrip Menu() => steps.Menu(sent.Add, () => updates++);
        static ToolStripMenuItem Group(ContextMenuStrip menu, int i = 0) => (ToolStripMenuItem)menu.Items[i];
        using (var menu = Menu())
        {
            Check(Group(menu).Text!.Contains("Review A && B"), "prompt context");
            Group(menu).DropDownItems[0].PerformClick();
        }
        Check(updates == 1 && sent.Single().AgentLoadWork.Block == "first", "expand with exact Rust block identity");
        first.Details = "loading";
        using (var menu = Menu())
            Check(Group(menu).DropDownItems.Count == 1 && steps.Lines().Contains("Loading steps…"), "pending turn");
        first.Details = "failed";
        first.DetailsError = "Source unavailable";
        using (var menu = Menu())
        {
            Check(steps.Lines().Contains("Source unavailable"), "host error");
            Group(menu).DropDownItems[1].PerformClick();
        }
        Check(sent.Count == 2 && sent[1].AgentLoadWork.Block == "first", "retry exact turn");
        first.Details = "";
        var activity = new ChatBlock { Id = "activity", Kind = "activity", Title = "Tools",
            ActivityRows = { new ChatActivityRow { Id = "tool", Title = "Read sample", Detail = "sample.txt",
                Status = "completed", Output = "first result", ToolName = "Read" } } };
        first.Children.Add(activity);
        var second = first.Clone();
        second.Id = "second";
        second.DefaultExpanded = true;
        second.Children[0].ActivityRows[0].Output = "second result";
        view.Timeline.Add(second);
        using (var menu = Menu()) Group(menu).DropDownItems[1].PerformClick();
        string rendered = string.Join("\n", steps.Lines());
        Check(rendered.Contains("Read sample · completed") && rendered.Contains("sample.txt") &&
              rendered.Contains("first result") && !rendered.Contains("second result"), "native detail disclosure by turn");
        steps.Display(view.Clone());
        Check(steps.Lines().Any(line => line.Contains("first result")), "disclosure survives refresh");
        using var stale = Menu();
        var next = view.Clone();
        next.DeviceId = "another";
        steps.Display(next);
        steps.Display(view);
        int prior = updates;
        Group(stale).DropDownItems[0].PerformClick();
        Group(stale).DropDownItems[1].PerformClick();
        Check(sent.Count == 2 && prior == updates && !steps.Lines().Any(line => line.Contains("first result")),
              "stale action after navigating away and back");
        using (var menu = Menu()) Group(menu, 1).DropDownItems[0].PerformClick();
        Check(!steps.Lines().Contains("Read sample · completed"), "collapse Rust-default-expanded steps");
        Console.WriteLine("Passed native work disclosure, lazy loading, retry, context isolation and transcript notices.");
    }
}

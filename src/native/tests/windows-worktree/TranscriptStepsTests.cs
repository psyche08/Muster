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
        CopyCodeBlocks();
        Console.WriteLine("Passed native work disclosure, lazy loading, retry, context isolation and transcript notices.");
    }

    static void CopyCodeBlocks()
    {
        static void Check(bool yes, string name)
        {
            if (!yes) throw new InvalidOperationException(name);
        }
        var steps = new TranscriptSteps();
        var copied = new List<string>();
        var view = new ChatView { DeviceId = "sample", Session = new() { Id = "one" }, MessageCount = 2 };
        const string source = "  echo 'sample & 🙂'\r\n\r\n  echo done\n";
        var block = new ChatBlock { Id = "block", Kind = "message",
            Message = new() { Id = "message", Role = "assistant", Text = "legacy transcript" },
            Parts = { new MessagePart { Kind = "markdown", Text = "Example" },
                      new MessagePart { Kind = "code", Language = "sh", Text = source } } };
        view.Timeline.Add(block);
        steps.Display(view);
        Check(steps.Lines().SequenceEqual(new[] { "assistant", "Example", "Code · sh", source.ReplaceLineEndings(Environment.NewLine) }), "ordered shared parts");
        Check(steps.HasCode && steps.CopyCode(copied.Add) == null && copied.Single() == source, "exact single-block copy");
        var child = new ChatBlock { Id = "child", Kind = "message", Message = new() { Id = "nested", Role = "user" },
            Parts = { new MessagePart { Kind = "code", Language = "python", Text = "    value = 2\n" } } };
        view.Timeline.Add(new ChatBlock { Id = "work", Kind = "working", Children = { child } });
        steps.Display(view);
        Check(steps.CopyCode(copied.Add) == null && copied.Count == 2, "collapsed work exposed hidden code");
        view.Timeline[1].DefaultExpanded = true;
        steps.Display(view);
        using var menu = steps.CopyCode(copied.Add)!;
        Check(menu.Items.Count == 2 && menu.Items[0].Text!.Contains("&&") && copied.Count == 2, "menu selection or label");
        steps.Display(view.Clone());
        menu.Items[1].PerformClick();
        Check(copied.Last() == child.Parts[0].Text, "nested source after refresh");
        var edited = view.Clone();
        edited.Timeline[0].Parts[1].Text += "change";
        steps.Display(edited);
        menu.Items[0].PerformClick();
        Check(copied.Count == 3, "stale code was copied");
        var other = view.Clone();
        other.DeviceId = "other";
        steps.Display(other);
        steps.Display(view);
        menu.Items[1].PerformClick();
        Check(copied.Count == 3, "stale menu survived navigation");
        view.Timeline.RemoveAt(1);
        block.Parts.Clear();
        steps.Display(view);
        Check(!steps.HasCode && steps.CopyCode(copied.Add) == null &&
            steps.Lines().Single().Contains("legacy transcript"), "older history fallback");
        Console.WriteLine("Passed desktop code presentation, exact copy, disclosure and stale clipboard action checks.");
    }
}

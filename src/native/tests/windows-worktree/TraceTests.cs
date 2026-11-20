using Muster.App;
using Muster.Windows;

internal static class TraceTests
{
    public static void Run(string? captures)
    {
        static void Check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        using var host = new Form { Size = new Size(850, 480), ShowInTaskbar = false,
            StartPosition = FormStartPosition.Manual, Location = new Point(-20000, -20000) };
        var trace = new SessionTrace { Dock = DockStyle.Fill };
        host.Controls.Add(trace);
        host.Show();
        var tree = (TreeView)trace.Controls.Find("TraceLedger", true).Single();
        var search = (TextBox)trace.Controls.Find("TraceSearch", true).Single();
        var details = (TextBox)trace.Controls.Find("TraceDetails", true).Single();
        var chat = new ChatView { DeviceId = "fixture", Session = new() { Id = "synthetic" } };
        chat.Trace.Add(new TraceSpan { Id = "leading", Kind = "status", Title = "Turn status", Status = "unknown" });
        chat.Trace.Add(new TraceSpan { Id = "first", Kind = "turn", Title = "User message", Detail = "Review", StartedUnix = 50 });
        chat.Trace.Add(new TraceSpan { Id = "tool", Kind = "tool", Title = "Read", Detail = "fixture output",
            Status = "completed", StartedUnix = 51, EndedUnix = 55, DurationKnown = true, DurationSeconds = 4 });
        chat.Trace.Add(new TraceSpan { Id = "second", Kind = "turn", Title = "User message", Detail = "Follow-up", StartedUnix = 60 });
        chat.Trace.Add(new TraceSpan { Id = "think", Kind = "reasoning", Title = "Reviewing", Status = "running", StartedUnix = 60 });
        trace.Display(chat);
        Check(tree.Nodes.Count == 3 && tree.Nodes[0].Text == "Before first turn", "Trace turn grouping");
        tree.SelectedNode = tree.Nodes.Find("tool", true).Single();
        Check(details.Text.Contains("4 s") && details.Text.Contains("fixture output"), "Paired tool details");
        var updated = chat.Clone();
        updated.Trace[2].Detail = "updated output";
        trace.Display(updated);
        Check(tree.SelectedNode?.Name == "tool" && details.Text.Contains("updated output"), "Refresh selected record by ID");
        var toolbar = trace.Controls.OfType<FlowLayoutPanel>().Single();
        tree.Nodes[0].Collapse();
        var mode = toolbar.Controls.OfType<Button>().Single(b => b.Text.StartsWith("Duration:"));
        mode.PerformClick();
        Check(mode.Text == "Duration: Recorded time", "Recorded-time mode");
        search.Text = "UPDATED";
        Check(tree.Nodes.Count == 1 && tree.Nodes[0].Nodes.Count == 1 && tree.Nodes[0].Text == "Turn 1", "Filter details and retain numbering");
        search.Clear();
        Check(!tree.Nodes[0].IsExpanded && tree.Nodes[1].IsExpanded, "Filtering retains collapsed turns");
        toolbar.Controls.OfType<Button>().Single(b => b.Text == "Collapse all").PerformClick();
        Check(tree.Nodes.Cast<TreeNode>().All(n => !n.IsExpanded), "Collapse visible turns");
        toolbar.Controls.OfType<Button>().Single(b => b.Text == "Expand all").PerformClick();
        Check(tree.Nodes.Cast<TreeNode>().All(n => n.IsExpanded), "Expand visible turns");
        tree.SelectedNode = tree.Nodes.Find("think", true).Single();
        Check(details.Text.Contains("Duration: Unknown") && details.Text.Contains("Ended: Unknown"), "Running trace without a finish");
        var bars = TraceOverview.LayoutBars(chat.Trace.Where(s => s.StartedUnix > 0).ToArray(), true);
        Check(bars[1].Lane == 2 && bars[1].Start == 0.1 && bars[1].Width == 0.4 && bars[^1].Width == 0, "Timed lane geometry");
        var equal = TraceOverview.LayoutBars([chat.Trace[^1], chat.Trace[^1]], true);
        Check(equal[1].Start == 0.5 && equal[1].Width == 0.5, "Coincident times");
        if (captures != null)
        {
            using var bitmap = new Bitmap(trace.Width, trace.Height);
            trace.DrawToBitmap(bitmap, trace.ClientRectangle);
            bitmap.Save(Path.Combine(captures, "session-trace.png"));
        }
        search.Text = "missing";
        Check(tree.Nodes.Count == 0 && details.Text.Length == 0, "Filtered trace clears inspector");
        var other = new ChatView { DeviceId = "other", Session = chat.Session.Clone(), LoadError = "Read unavailable" };
        trace.Display(other);
        Check(search.TextLength == 0 && tree.Nodes.Count == 0 && details.TextLength == 0
            && trace.Controls.OfType<Label>().Single().Text == other.LoadError, "Device change clears old trace and shows the read failure");
        trace.Display(null);
    }
}

using Muster.App;

namespace Muster.Windows;

internal sealed class SessionTrace : UserControl
{
    readonly TextBox search = new() { Name = "TraceSearch", AccessibleName = "Search trace", PlaceholderText = "Search trace" };
    readonly Button mode = new() { AutoSize = true, Text = "Duration: Equal width" };
    readonly TreeView ledger = new() { Name = "TraceLedger", AccessibleName = "Trace records", Dock = DockStyle.Fill, HideSelection = false };
    readonly TextBox details = new() { Name = "TraceDetails", AccessibleName = "Trace details", Dock = DockStyle.Fill,
        Multiline = true, ReadOnly = true, WordWrap = true, ScrollBars = ScrollBars.Vertical };
    readonly Label state = new() { AutoSize = true, Dock = DockStyle.Top };
    readonly TraceOverview overview = new() { Dock = DockStyle.Top };
    ChatView? current;
    readonly HashSet<string> folded = new();
    bool timed;
    bool rebuilding;

    public SessionTrace()
    {
        var toolbar = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true, WrapContents = true };
        var expand = new Button { AutoSize = true, Text = "Expand all" };
        var collapse = new Button { AutoSize = true, Text = "Collapse all" };
        toolbar.Controls.AddRange([mode, expand, collapse, search]);
        expand.Click += (_, _) => ledger.ExpandAll();
        collapse.Click += (_, _) => ledger.CollapseAll();
        mode.Click += (_, _) =>
        {
            timed = !timed;
            mode.Text = timed ? "Duration: Recorded time" : "Duration: Equal width";
            overview.Display(current?.Trace, timed);
        };
        search.TextChanged += (_, _) => Rebuild();
        ledger.AfterCollapse += (_, e) => { if (!rebuilding && e.Node != null) folded.Add(e.Node.Name); };
        ledger.AfterExpand += (_, e) => { if (!rebuilding && e.Node != null) folded.Remove(e.Node.Name); };
        ledger.AfterSelect += (_, _) => Inspect();
        overview.RecordSelected += id =>
        {
            var row = ledger.Nodes.Find(id, true).FirstOrDefault(n => n.Tag is TraceSpan);
            if (row == null && search.TextLength > 0)
            {
                search.Clear();
                row = ledger.Nodes.Find(id, true).FirstOrDefault(n => n.Tag is TraceSpan);
            }
            if (row == null) return;
            ledger.SelectedNode = row;
            row.EnsureVisible();
        };
        var panes = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 1 };
        panes.ColumnStyles.Add(new(SizeType.Percent, 54));
        panes.ColumnStyles.Add(new(SizeType.Percent, 46));
        panes.RowStyles.Add(new(SizeType.Percent, 100));
        panes.Controls.Add(ledger, 0, 0);
        panes.Controls.Add(details, 1, 0);
        Controls.AddRange([panes, state, overview, toolbar]);
    }

    public void Display(ChatView? chat)
    {
        if (ReferenceEquals(current, chat)) return;
        bool changed = current?.Session?.Id != chat?.Session?.Id || current?.DeviceId != chat?.DeviceId;
        bool redraw = changed || current?.Trace.Equals(chat?.Trace) != true || current?.Loading != chat?.Loading
            || current?.LoadError != chat?.LoadError;
        current = chat;
        if (changed)
        {
            folded.Clear();
            ledger.Nodes.Clear();
            search.Clear();
            timed = false;
            mode.Text = "Duration: Equal width";
        }
        if (redraw)
        {
            overview.Display(chat?.Trace, timed);
            Rebuild();
        }
    }

    void Rebuild()
    {
        string? selected = ledger.SelectedNode?.Name;
        string query = search.Text.Trim();
        rebuilding = true;
        ledger.BeginUpdate();
        try
        {
            ledger.Nodes.Clear();
            TreeNode? group = null;
            TreeNode? restore = null;
            int turn = 0;
            string key = "before", label = "Before first turn";
            foreach (var record in current?.Trace.AsEnumerable() ?? [])
            {
                if (record.Kind == "turn" && record.Title == "User message")
                {
                    group = null;
                    key = "turn/" + record.Id;
                    label = $"Turn {++turn}";
                }
                if (query.Length > 0 && !new[] { record.Title, record.Kind, record.Status, record.Detail }
                    .Any(s => s.Contains(query, StringComparison.OrdinalIgnoreCase))) continue;
                group ??= ledger.Nodes.Add(key, label);
                var row = group.Nodes.Add(record.Id, $"{record.Kind} · {record.Title} · {record.Status}");
                row.Tag = record;
                if (selected == record.Id) restore = row;
            }
            foreach (TreeNode node in ledger.Nodes)
                if (!folded.Contains(node.Name)) node.Expand();
            if (restore?.Parent?.IsExpanded == true) ledger.SelectedNode = restore;
        }
        finally { ledger.EndUpdate(); rebuilding = false; }
        Inspect();
        state.Text = current?.LoadError is { Length: > 0 } error ? error
            : ledger.Nodes.Count > 0 ? ""
            : current?.Loading == true ? "Loading trace…"
            : query.Length > 0 ? "No matching records" : "No trace records in the loaded history";
        state.Visible = state.Text.Length > 0;
    }

    void Inspect()
    {
        var span = ledger.SelectedNode?.Tag as TraceSpan;
        overview.SelectRecord(span?.Id);
        string text = span == null ? "" : string.Join(Environment.NewLine,
            span.Title, $"Kind: {span.Kind}", $"Status: {span.Status}", $"Started: {Time(span.StartedUnix)}",
            $"Ended: {Time(span.EndedUnix)}", $"Duration: {(span.DurationKnown ? $"{span.DurationSeconds} s" : "Unknown")}",
            "", span.Detail.ReplaceLineEndings(Environment.NewLine));
        if (details.Text != text) details.Text = text;
    }

    static string Time(ulong seconds) => seconds is > 0 and <= 253402300799
        ? DateTimeOffset.FromUnixTimeSeconds((long)seconds).ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss zzz") : "Unknown";
}

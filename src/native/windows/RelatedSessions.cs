using Muster.App;

namespace Muster.Windows;

internal sealed class RelatedSessions : UserControl
{
    sealed record Row(string Id, string Title, string Relation, string State);
    readonly ListView list = new() { Name = "RelatedList", AccessibleName = "Related sessions",
        Dock = DockStyle.Fill, View = View.Details, FullRowSelect = true, MultiSelect = false,
        HideSelection = false, Activation = ItemActivation.OneClick, HeaderStyle = ColumnHeaderStyle.Nonclickable };
    readonly Label empty = new() { Dock = DockStyle.Fill, TextAlign = ContentAlignment.MiddleCenter };
    (string Device, string Session, SessionTab Tab)? scope;
    Row[] rows = [];
    public event Action<Intent>? Open;

    public RelatedSessions()
    {
        list.Columns.Add("Title");
        list.Columns.Add("Relationship");
        list.Columns.Add("State");
        list.SizeChanged += (_, _) => SizeColumns();
        list.DpiChangedAfterParent += (_, _) => SizeColumns();
        list.ItemActivate += (_, _) =>
        {
            if (scope is {} origin && list.SelectedItems.Count == 1 && list.SelectedItems[0].Tag is Row row)
                Open?.Invoke(new() { OpenAgent = new() { DeviceId = origin.Device, Session = row.Id } });
        };
        Controls.Add(list);
        Controls.Add(empty);
    }

    void SizeColumns()
    {
        int relation = LogicalToDeviceUnits(150), state = LogicalToDeviceUnits(120);
        list.Columns[0].Width = Math.Max(LogicalToDeviceUnits(100), list.ClientSize.Width - relation - state - 4);
        list.Columns[1].Width = relation;
        list.Columns[2].Width = state;
    }

    public void Display(ChatView? chat, SessionTab tab)
    {
        if (chat?.Session == null || tab is not (SessionTab.Subagents or SessionTab.Sessions))
        {
            if (scope == null) return;
            scope = null;
            rows = [];
            list.Items.Clear();
            return;
        }
        var next = (chat.DeviceId, chat.Session.Id, tab);
        var source = tab == SessionTab.Subagents ? chat.Session.Subagents : chat.Session.Subsessions;
        var items = source.Select(s =>
        {
            string relation = s.Relationship switch { "sidechat" => "Side chat", "fork" => "Fork",
                _ => tab == SessionTab.Subagents ? "Subagent" : "Sub-session" };
            if (s.ReadOnly) relation += " · read only";
            return new Row(s.Id, s.Title.Length == 0 ? s.Id : s.Title, relation, s.State);
        }).ToArray();
        if (scope == next && rows.SequenceEqual(items)) return;
        string? selected = scope == next && list.SelectedItems.Count == 1 ? (list.SelectedItems[0].Tag as Row)?.Id : null;
        scope = next;
        rows = items;
        list.BeginUpdate();
        try
        {
            list.Items.Clear();
            foreach (var row in rows)
            {
                var item = new ListViewItem([row.Title, row.Relation, row.State]) { Tag = row };
                list.Items.Add(item);
                if (row.Id == selected) { item.Selected = true; item.Focused = true; }
            }
        }
        finally { list.EndUpdate(); }
        if (list.SelectedItems.Count == 1) list.SelectedItems[0].EnsureVisible();
        empty.Text = tab == SessionTab.Subagents ? "No subagents" : "No sessions";
        empty.Visible = rows.Length == 0;
        list.Visible = rows.Length != 0;
    }
}

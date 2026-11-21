using Muster.App;
using Muster.V1;
using Kind = Muster.App.AgentCatalogAction.Types.Kind;

namespace Muster.Windows;

internal sealed class AgentSidebar : UserControl
{
    sealed record Entry(string Section, string Label, Intent Action, AgentCatalogRow? Row = null)
    {
        public override string ToString() => Label;
        public bool Matches(Entry? other) => other != null && Section == other.Section && Action.Equals(other.Action);
    }

    readonly TextBox query = new() { Name = "AgentSearch", AccessibleName = "Search sessions",
        PlaceholderText = "Search sessions", Dock = DockStyle.Top };
    readonly FlowLayoutPanel buttons = new() { AutoSize = true, Dock = DockStyle.Top };
    readonly Label status = new() { Dock = DockStyle.Top, AutoEllipsis = true, Visible = false };
    readonly ToolTip tips = new();
    readonly ListBox list = new() { Name = "AgentSessions", AccessibleName = "Agent sessions",
        Dock = DockStyle.Fill, HorizontalScrollbar = true, IntegralHeight = false };
    readonly Button cli, desktop, device, clear, more;
    readonly ContextMenuStrip menu = new();
    AgentsView? view;
    IEnumerable<MemberView> devices = [];
    Entry? pressed;
    (string Device, string Session)? shown;

    public event Action<Intent>? Intent;

    public AgentSidebar()
    {
        Padding = new Padding(0, 0, 8, 0);
        AccessibleName = "Agents directory";
        Controls.AddRange([list, status, buttons, query]);
        Button Add(string label, Action click)
        {
            var button = new Button { Text = label, AutoSize = true };
            button.Click += (_, _) => click();
            buttons.Controls.Add(button);
            return button;
        }
        Add("Search", () => Search(query.Text));
        clear = Add("Clear search", () => { query.Clear(); Search(""); });
        more = Add("More results", () => Intent?.Invoke(Catalog(Kind.SearchMore)));
        device = Add("Device", ChooseDevice);
        cli = Add("New CLI session", () => CreateSession(false));
        desktop = Add("New desktop session", () => CreateSession(true));
        query.KeyDown += (_, e) =>
        {
            if (e.KeyData != Keys.Enter) return;
            e.SuppressKeyPress = true;
            Search(query.Text);
        };
        list.KeyDown += (_, e) =>
        {
            if (e.KeyData != Keys.Enter) return;
            e.SuppressKeyPress = true;
            if (list.SelectedItem is Entry row) Intent?.Invoke(row.Action);
        };
        list.MouseDown += (_, e) =>
        {
            int i = list.IndexFromPoint(e.Location);
            pressed = e.Button == MouseButtons.Left && i >= 0 ? (Entry)list.Items[i] : null;
            if (e.Button == MouseButtons.Right) list.SelectedIndex = i;
        };
        list.MouseClick += (_, e) =>
        {
            int i = list.IndexFromPoint(e.Location);
            if (e.Button == MouseButtons.Left && i >= 0 && list.Items[i] is Entry row && row.Matches(pressed))
                Intent?.Invoke(row.Action);
            pressed = null;
        };
        list.ContextMenuStrip = menu;
        menu.Opening += (_, e) =>
        {
            while (menu.Items.Count > 0) menu.Items[0].Dispose();
            if (list.SelectedItem is Entry { Row: {} row })
                AgentRowMenu.Populate(menu, row, action => Intent?.Invoke(action));
            e.Cancel = menu.Items.Count == 0;
        };
        Disposed += (_, _) => { tips.Dispose(); menu.Dispose(); };
    }

    static Intent Catalog(Kind kind, string value = "") =>
        new() { AgentCatalogAction = new() { Kind = kind, Value = value } };

    void Search(string text) => Intent?.Invoke(Catalog(Kind.Search, text));

    void ChooseDevice()
    {
        if (view == null) return;
        DeviceMenu.Create(devices, view.DeviceId, id => Intent?.Invoke(new() { OpenAgents = new() { DeviceId = id } }))
            .ShowBelow(device);
    }

    void CreateSession(bool native)
    {
        if (view == null) return;
        using var dialog = new AgentSessionDialog(view, native);
        if (dialog.ShowDialog(FindForm()) == DialogResult.OK) Intent?.Invoke(dialog.Request());
    }

    static IEnumerable<Entry> Entries(AgentsView agents)
    {
        static Intent Open(AgentSession session) =>
            new() { OpenAgent = new() { Session = session.Id, DeviceId = session.DeviceId } };
        foreach (var section in agents.Sections)
        {
            yield return new(section.Key, $"{(section.Expanded ? "−" : "+")} {section.Title} ({section.Count})",
                Catalog(Kind.ToggleSection, section.Key));
            foreach (var row in section.Rows)
            {
                if (row.Session is not {} session) continue;
                yield return new(section.Key, $"    {session.Title}    {row.SourceLabel}    {row.LaunchLabel}", Open(session), row);
                if (row.Expandable)
                    yield return new(section.Key, row.Expanded ? "      Collapse replies" : "      Expand replies",
                        Catalog(Kind.ToggleFamily, row.Key));
            }
            if (section.Expanded && section.HasMore && !section.LoadingMore)
                yield return new(section.Key, "    Load more", Catalog(Kind.LoadMore, section.Key));
        }
        if (agents.Sections.Count == 0)
            foreach (var session in agents.Sessions)
                yield return new("", $"{session.Title}    {session.DeviceName}", Open(session));
    }

    int Index(Predicate<Entry> matches)
    {
        for (int i = 0; i < list.Items.Count; i++)
            if (matches((Entry)list.Items[i])) return i;
        return -1;
    }

    public void Display(AgentsView? next, IEnumerable<MemberView> hosts, (string Device, string Session)? active)
    {
        var old = view;
        view = next;
        devices = hosts;
        if (next == null)
        {
            if (old != null) { list.Items.Clear(); query.Clear(); }
            pressed = null;
            shown = null;
            return;
        }
        if (old?.Query != next.Query) query.Text = next.Query;
        clear.Visible = next.Query.Length > 0;
        more.Visible = next.SearchHasMore && !next.Searching;
        device.Visible = devices.Any();
        device.Text = "Device: " + DeviceMenu.Label(devices, next.DeviceId);
        cli.Visible = next.CreateHosts.Count > 0;
        cli.Enabled = !next.Creating;
        cli.Text = next.Creating ? "Creating CLI session…" : "New CLI session";
        desktop.Visible = next.NativeCreateHosts.Count > 0;
        desktop.Enabled = !next.NativeCreating;
        desktop.Text = next.NativeCreating ? "Creating desktop session…" : "New desktop session";
        var notices = next.SearchNotices.Concat(next.Errors).ToList();
        if (next.Searching || next.Loading) notices.Add("Loading sessions…");
        status.Visible = notices.Count > 0;
        status.Height = status.Font.Height + LogicalToDeviceUnits(4);
        var message = string.Join("  ", notices);
        if (status.Text != message) { status.Text = message; tips.SetToolTip(status, message); }

        int ActiveIndex() => active is {} key ? Index(e => e.Action.OpenAgent is {} open &&
            open.DeviceId == key.Device && open.Session == key.Session) : -1;
        bool scopeChanged = old?.DeviceId != next.DeviceId || old?.Query != next.Query;
        if (scopeChanged) pressed = null;
        if (scopeChanged || old == null || !old.Sections.Equals(next.Sections) ||
            next.Sections.Count == 0 && !old.Sessions.Equals(next.Sessions))
        {
            var selected = list.SelectedItem as Entry;
            var top = list.TopIndex >= 0 && list.TopIndex < list.Items.Count ? (Entry)list.Items[list.TopIndex] : null;
            list.BeginUpdate();
            try
            {
                list.Items.Clear();
                list.Items.AddRange(Entries(next).ToArray());
                bool keep = !scopeChanged && active == shown;
                int selection = keep ? Index(e => e.Matches(selected)) : -1;
                list.SelectedIndex = selection >= 0 ? selection : ActiveIndex();
                if (keep && top != null)
                {
                    int first = Index(e => e.Matches(top));
                    if (first >= 0) list.TopIndex = first;
                }
            }
            finally { list.EndUpdate(); }
        }
        else if (active != shown && active != null)
            list.SelectedIndex = ActiveIndex();
        shown = active;
    }
}

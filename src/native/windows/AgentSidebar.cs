using Muster.App;
using Muster.V1;
using Kind = Muster.App.AgentCatalogAction.Types.Kind;

namespace Muster.Windows;

internal sealed class AgentSidebar : UserControl
{
    sealed record Entry(string Section, string Label, Intent Action, AgentCatalogRow? Row = null, AgentSession? Session = null)
    {
        public override string ToString() => Label;
        public bool Matches(Entry? other) => other != null && Section == other.Section && Action.Equals(other.Action);
    }

    readonly TextBox query = new() { Name = "AgentSearch", AccessibleName = "Search sessions",
        PlaceholderText = "Search sessions", Dock = DockStyle.Top };
    readonly FlowLayoutPanel buttons = new() { AutoSize = true, Dock = DockStyle.Top };
    readonly TextBox status = new() { Name = "AgentCatalogStatus", AccessibleName = "Catalog status",
        Dock = DockStyle.Top, Multiline = true, ReadOnly = true, WordWrap = true,
        ScrollBars = ScrollBars.Vertical, Visible = false };
    readonly ListBox list = new() { Name = "AgentSessions", AccessibleName = "Agent sessions",
        Dock = DockStyle.Fill, HorizontalScrollbar = true, IntegralHeight = false };
    readonly Button create, device, clear, more;
    readonly ContextMenuStrip menu = new();
    AgentsView? view;
    IEnumerable<MemberView> devices = [];
    Entry? pressed;
    (string Device, string Session)? shown;

    public event Action<Intent>? Intent;

    public AgentSidebar()
    {
        BackColor = WorkspaceStyle.Sidebar;
        Padding = new Padding(12, 12, 12, 10);
        list.BackColor = BackColor;
        list.BorderStyle = BorderStyle.None;
        list.DrawMode = DrawMode.OwnerDrawFixed;
        list.HorizontalScrollbar = false;
        var strong = new Font(list.Font, FontStyle.Bold);
        var secondary = new Font(list.Font.FontFamily, Math.Max(8, list.Font.SizeInPoints - 1));
        void SizeRows() => list.ItemHeight = list.Font.Height * 2 + LogicalToDeviceUnits(8);
        SizeRows();
        DpiChangedAfterParent += (_, _) => SizeRows();
        Disposed += (_, _) => { strong.Dispose(); secondary.Dispose(); };
        list.DrawItem += (_, e) =>
        {
            if (e.Index < 0 || e.Index >= list.Items.Count) return;
            var entry = (Entry)list.Items[e.Index];
            bool selected = (e.State & DrawItemState.Selected) != 0;
            Color ink = selected && SystemInformation.HighContrast ? SystemColors.HighlightText : WorkspaceStyle.Ink;
            using var brush = new SolidBrush(selected ? (SystemInformation.HighContrast ? SystemColors.Highlight : Color.FromArgb(230, 222, 214)) : BackColor);
            e.Graphics.FillRectangle(brush, e.Bounds);
            var area = Rectangle.Inflate(e.Bounds, -LogicalToDeviceUnits(8), 0);
            const TextFormatFlags flags = TextFormatFlags.SingleLine | TextFormatFlags.NoPrefix |
                TextFormatFlags.VerticalCenter | TextFormatFlags.EndEllipsis;
            var session = entry.Row?.Session ?? entry.Session;
            if (session == null)
                TextRenderer.DrawText(e.Graphics, entry.Label.Trim(), entry.Action.AgentCatalogAction?.Kind == Kind.ToggleSection ? strong : list.Font,
                    area, ink, flags);
            else
            {
                string meta = string.Join(" · ", new[] { session.DeviceName, entry.Row?.SourceLabel, entry.Row?.LaunchLabel }
                    .Where(t => !string.IsNullOrWhiteSpace(t)).Distinct());
                var line = area;
                if (meta.Length > 0) { line.Y += LogicalToDeviceUnits(3); line.Height = list.Font.Height + 1; }
                TextRenderer.DrawText(e.Graphics, session.Title, selected ? strong : list.Font, line, ink, flags);
                if (meta.Length > 0)
                {
                    line.Y += line.Height;
                    TextRenderer.DrawText(e.Graphics, meta, secondary, line, selected ? ink : WorkspaceStyle.Muted, flags);
                }
            }
            e.DrawFocusRectangle();
        };
        AccessibleName = "Agents directory";
        Controls.AddRange([list, status, buttons, query, WorkspaceStyle.DirectoryHeading("Sessions")]);
        Button Add(string label, Action click)
        {
            var button = new Button { Text = label, AutoSize = true };
            WorkspaceStyle.StyleButton(button);
            button.Click += (_, _) => click();
            buttons.Controls.Add(button);
            return button;
        }
        var searchRow = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 2 };
        searchRow.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        searchRow.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        var submitSearch = Add("Search", () => Search(query.Text));
        int searchIndex = Controls.GetChildIndex(query);
        Controls.Add(searchRow);
        Controls.SetChildIndex(searchRow, searchIndex);
        query.Anchor = AnchorStyles.Left | AnchorStyles.Right;
        query.Dock = DockStyle.None;
        searchRow.Controls.Add(query, 0, 0);
        searchRow.Controls.Add(submitSearch, 1, 0);
        clear = Add("Clear search", () => { query.Clear(); Search(""); });
        more = Add("More results", () => Intent?.Invoke(Catalog(Kind.SearchMore)));
        device = Add("Device", ChooseDevice);
        create = Add("New session ▾", ShowCreateMenu);
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
        status.BackColor = BackColor;
        status.ForeColor = WorkspaceStyle.Muted;
        status.BorderStyle = BorderStyle.None;
        status.TextChanged += (_, _) => SizeStatus();
        status.FontChanged += (_, _) => SizeStatus();
        Resize += (_, _) => SizeStatus();
        Disposed += (_, _) => menu.Dispose();
    }

    void SizeStatus()
    {
        if (status.TextLength == 0) return;
        int width = Math.Max(1, ClientSize.Width - Padding.Horizontal - SystemInformation.VerticalScrollBarWidth - LogicalToDeviceUnits(8));
        var size = TextRenderer.MeasureText(status.Text, status.Font, new Size(width, int.MaxValue),
            TextFormatFlags.WordBreak | TextFormatFlags.TextBoxControl | TextFormatFlags.NoPrefix);
        status.Height = Math.Clamp(size.Height, status.Font.Height, status.Font.Height * 4) + LogicalToDeviceUnits(8);
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

    void ShowCreateMenu()
    {
        if (view == null) return;
        var choices = new ContextMenuStrip();
        if (view.CreateHosts.Count > 0)
            choices.Items.Add("New CLI session", null, (_, _) => CreateSession(false)).Enabled = !view.Creating;
        if (view.NativeCreateHosts.Count > 0)
            choices.Items.Add("New desktop session", null, (_, _) => CreateSession(true)).Enabled = !view.NativeCreating;
        choices.ShowBelow(create);
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
                yield return new("", $"{session.Title}    {session.DeviceName}", Open(session), Session: session);
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
            if (old != null) { list.Items.Clear(); query.Clear(); status.Clear(); status.Visible = false; }
            pressed = null;
            shown = null;
            return;
        }
        if (old?.Query != next.Query) query.Text = next.Query;
        clear.Visible = next.Query.Length > 0;
        more.Visible = next.SearchHasMore && !next.Searching;
        device.Visible = devices.Any();
        device.Text = "Device: " + DeviceMenu.Label(devices, next.DeviceId);
        create.Visible = next.CreateHosts.Count > 0 || next.NativeCreateHosts.Count > 0;
        create.Enabled = next.CreateHosts.Count > 0 && !next.Creating || next.NativeCreateHosts.Count > 0 && !next.NativeCreating;
        var notices = next.SearchNotices.Concat(next.Errors).ToList();
        if (next.Searching) notices.Add("Searching conversations…");
        else if (next.Loading) notices.Add("Loading sessions…");
        var message = string.Join(Environment.NewLine, notices);
        if (status.Text != message) status.Text = message;
        status.Visible = message.Length > 0;

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

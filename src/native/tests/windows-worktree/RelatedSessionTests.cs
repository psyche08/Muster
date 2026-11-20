using Muster.App;
using Muster.V1;
using Muster.Windows;

internal static class RelatedSessionTests
{
    public static void Run(string? captures)
    {
        static void Check(bool ok, string message)
        {
            if (!ok) throw new InvalidOperationException(message);
        }
        using var host = new Form { Size = new Size(800, 420), StartPosition = FormStartPosition.Manual,
            Location = new Point(-20000, -20000), ShowInTaskbar = false };
        var pane = new RelatedSessions { Dock = DockStyle.Fill };
        var tabs = new SessionTabs();
        host.Controls.Add(pane);
        host.Controls.Add(tabs);
        host.Show();
        var parent = new ChatView { DeviceId = "host-a", Session = new() { Id = "parent" } };
        var reader = new AgentSession { Id = "read", Title = "Review", State = "idle", ReadOnly = true };
        parent.Session.Subagents.Add(reader);
        parent.Session.Subagents.Add(new AgentSession { Id = "other", Title = reader.Title, State = "running" });
        parent.Session.Subsessions.Add(new AgentSession { Id = "side", Relationship = "sidechat", State = "ended" });
        parent.Session.Subsessions.Add(new AgentSession { Id = "fork", Title = "Review", Relationship = "fork" });
        SessionTab? picked = null;
        tabs.Chosen += tab => picked = tab;
        RadioButton Tab(SessionTab id) => tabs.Controls.OfType<RadioButton>().Single(b => Equals(b.Tag, id));
        tabs.Display(parent, SessionTab.Conversation);
        Check(Tab(SessionTab.Conversation).Checked && !Tab(SessionTab.Terminal).Enabled && Tab(SessionTab.Sessions).Text == "Sessions (2)", "Tab projection");
        Tab(SessionTab.Subagents).PerformClick();
        Check(picked == SessionTab.Subagents, "Direct subagents action");
        tabs.Display(parent, SessionTab.Subagents);
        pane.Display(parent, SessionTab.Subagents);
        var list = pane.Controls.OfType<ListView>().Single();
        Check(list.Visible && list.Items.Count == 2 && list.Items[0].SubItems[1].Text == "Subagent · read only", "Read-only projection");
        list.Items[0].Selected = true;
        var changed = parent.Clone();
        changed.Session.Subagents.RemoveAt(0);
        changed.Session.Subagents.Add(reader.Clone());
        pane.Display(changed, SessionTab.Subagents);
        Check(list.SelectedIndices.Count == 1 && list.SelectedIndices[0] == 1, "Stable selection across reordered equal titles");
        Intent? opened = null;
        pane.Open += intent => opened = intent;
        void Activate() => typeof(ListView).GetMethod("OnItemActivate", System.Reflection.BindingFlags.Instance
            | System.Reflection.BindingFlags.NonPublic)!.Invoke(list, [EventArgs.Empty]);
        Activate();
        Check(opened?.OpenAgent?.Session == "read" && opened.OpenAgent.DeviceId == "host-a", "Scoped row activation");
        changed.Session.Subagents.RemoveAt(1);
        pane.Display(changed, SessionTab.Subagents);
        opened = null;
        Activate();
        Check(opened == null && list.SelectedItems.Count == 0, "Removed rows cannot activate");
        pane.Display(parent, SessionTab.Sessions);
        tabs.Display(parent, SessionTab.Sessions);
        Check(list.Items.Count == 2 && list.Items[0].Text == "side" && list.Items[0].SubItems[1].Text == "Side chat"
            && list.Items[1].SubItems[1].Text == "Fork", "Independent list with title fallback and relation");
        list.Items[1].Selected = true;
        if (captures != null)
        {
            using var bitmap = new Bitmap(host.ClientSize.Width, host.ClientSize.Height);
            host.DrawToBitmap(bitmap, new Rectangle(Point.Empty, bitmap.Size));
            bitmap.Save(Path.Combine(captures, "related-sessions.png"));
        }
        var peer = parent.Clone();
        peer.DeviceId = "host-b";
        pane.Display(peer, SessionTab.Sessions);
        Check(list.SelectedItems.Count == 0, "Same IDs on another host clear selection");
        list.Items[1].Selected = true;
        Activate();
        Check(opened?.OpenAgent?.DeviceId == "host-b" && opened.OpenAgent.Session == "fork", "Open child on current peer");
        peer.Session.Subsessions.Clear();
        pane.Display(peer, SessionTab.Sessions);
        tabs.Display(peer, SessionTab.Sessions);
        Check(!list.Visible && pane.Controls.OfType<Label>().Single().Text == "No sessions" && Tab(SessionTab.Sessions).Enabled,
            "Empty related tab remains accessible");
        peer.Terminal = new() { Available = true };
        host.Width = 260;
        tabs.Display(peer, SessionTab.Terminal);
        Application.DoEvents();
        Check(Tab(SessionTab.Terminal).Enabled && Tab(SessionTab.Terminal).Checked && tabs.HorizontalScroll.Visible,
            "Narrow strip scrolls to the selected tab");
        pane.Display(null, SessionTab.Conversation);
        Check(list.Items.Count == 0, "Leaving clears rows");
        Console.WriteLine("Passed related-session tabs, empty views, refresh, identity and exact navigation.");
    }
}

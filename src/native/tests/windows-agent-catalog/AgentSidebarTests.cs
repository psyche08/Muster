using System.Reflection;
using Muster.App;
using Muster.V1;
using Muster.Windows;

internal static class AgentSidebarTests
{
    public static void Run()
    {
        static void Check(bool ok, string name)
        {
            if (!ok) throw new InvalidOperationException(name);
        }

        using var host = new Form { ShowInTaskbar = false, StartPosition = FormStartPosition.Manual,
            Location = new Point(-20000, -20000), ClientSize = new Size(320, 420) };
        var sidebar = new AgentSidebar { Dock = DockStyle.Fill };
        host.Controls.Add(sidebar);
        host.Show();
        var intents = new List<Intent>();
        sidebar.Intent += intents.Add;
        var list = (ListBox)sidebar.Controls.Find("AgentSessions", true).Single();
        var query = (TextBox)sidebar.Controls.Find("AgentSearch", true).Single();
        static void Raise(Control control, string method, EventArgs args) =>
            typeof(Control).GetMethod(method, BindingFlags.NonPublic | BindingFlags.Instance)!.Invoke(control, [args]);
        var a = new AgentSession { DeviceId = "host-a", Id = "session", Title = "Planning" };
        var b = a.Clone();
        b.DeviceId = "host-b";
        var view = new AgentsView { DeviceId = "*", Sections = { new AgentCatalogSection
            { Key = "group", Expanded = true, Title = "Codex", Count = 2, Rows = {
                new AgentCatalogRow { Key = "a", Session = a }, new AgentCatalogRow { Key = "b", Session = b }
            } } } };
        sidebar.Display(view, [], ("host-b", "session"));
        Check(list.SelectedIndex == 2 && list.Height > 3 * list.ItemHeight, "selected host in equal-title catalog");
        var point = list.GetItemRectangle(1).Location + new Size(4, 2);
        var click = new MouseEventArgs(MouseButtons.Left, 1, point.X, point.Y, 0);
        Raise(list, "OnMouseDown", click);
        var moved = view.Clone();
        moved.Sections[0].Rows.RemoveAt(0);
        sidebar.Display(moved, [], ("host-b", "session"));
        Raise(list, "OnMouseClick", click);
        Check(intents.Count == 0 && list.SelectedIndex == 1, "do not retarget an old click after refresh");
        Raise(list, "OnMouseDown", click);
        Raise(list, "OnMouseClick", click);
        var opened = intents.Single().OpenAgent;
        Check(opened.DeviceId == b.DeviceId && opened.Session == b.Id,
            "mouse opens exact host and session");
        query.Text = "synthetic query";
        sidebar.Display(moved.Clone(), [], ("host-b", "session"));
        Raise(query, "OnKeyDown", new KeyEventArgs(Keys.Enter));
        Check(intents.Last().AgentCatalogAction is { Kind: AgentCatalogAction.Types.Kind.Search,
            Value: "synthetic query" }, "search survives render and submits independently of composer");
        var flat = new AgentsView { DeviceId = "*", Sessions = { a, b } };
        sidebar.Display(flat, [], ("host-a", "session"));
        Raise(list, "OnKeyDown", new KeyEventArgs(Keys.Enter));
        Check(list.SelectedIndex == 0 && intents.Last().OpenAgent.DeviceId == a.DeviceId,
            "keyboard opens legacy ungrouped session");
        sidebar.Display(null, [], null);
        Check(list.Items.Count == 0, "leaving Agents clears the sidebar");
        Console.WriteLine($"Passed Agents directory search, row identity, gesture refresh and flat catalog at {host.DeviceDpi} DPI.");
    }
}

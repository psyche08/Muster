using Muster.App;
using Muster.V1;
using Muster.Windows;
using Organize = Muster.V1.AgentOrganizeRequest.Types.Action;

internal static class Program
{
    static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    static T Field<T>(Form form, string name) where T : Control =>
        (T)form.Controls.Find(name, true).Single();

    [STAThread]
    static void Main()
    {
        var win = new MemberView { DeviceId = "fixture-w", Name = "Test device" };
        var mac = new MemberView { DeviceId = "fixture-m", Name = "Test Mac" };
        var view = new AgentsView {
            CreateHosts = { win, mac }, CreateAgents = { "codex", "claude" },
            NativeCreateHosts = { mac }, NativeCreateApps = { "claude", "codex" }
        };
        using (var cli = new AgentSessionDialog(view, false))
        {
            var submit = Field<Button>(cli, "Submit");
            Check(!submit.Enabled, "CLI directory required");
            Field<TextBox>(cli, "Directory").Text = @"C:\fixture";
            Field<ComboBox>(cli, "Provider").SelectedItem = "claude";
            Check(submit.Enabled, "complete CLI input");
            var request = cli.Request().OrcaCreate;
            Check(request.DeviceId == win.DeviceId && request.Agent == "claude"
                && request.Directory == @"C:\fixture", "CLI target and path");
            cli.Opacity = 0;
            cli.Shown += (_, _) => cli.BeginInvoke(new Action(() =>
                typeof(Form).GetMethod("ProcessDialogKey", System.Reflection.BindingFlags.Instance |
                    System.Reflection.BindingFlags.NonPublic)!.Invoke(cli, new object[] { Keys.Escape })));
            Check(cli.ShowDialog() == DialogResult.Cancel, "Escape cancels the modal request");
        }

        view.NativeCreateDraft = new() { DeviceId = mac.DeviceId, Provider = "codex",
                                         Directory = "/fixture", Prompt = "Synthetic first message" };
        using (var desktop = new AgentSessionDialog(view, true))
        {
            Check(Field<ComboBox>(desktop, "Device").Items.Count == 1, "projected native hosts only");
            Check(desktop.Request().NativeCreate.Equals(view.NativeCreateDraft), "restore refused request");
            Check(desktop.AcceptButton == null, "Return in the prompt must not send");
            Field<TextBox>(desktop, "Prompt").Text = " ";
            Check(!Field<Button>(desktop, "Submit").Enabled, "first message required");
            Field<TextBox>(desktop, "Prompt").Text = "Revised fixture";
            Check(desktop.Request().NativeCreate.Prompt == "Revised fixture", "submit corrected draft");
        }
        view.NativeCreateDraft.DeviceId = "disconnected";
        using (var desktop = new AgentSessionDialog(view, true))
        {
            Check(!Field<Button>(desktop, "Submit").Enabled, "no implicit retarget on disconnection");
            Check(Field<TextBox>(desktop, "Prompt").Text == view.NativeCreateDraft.Prompt, "retain prompt");
        }

        var row = new AgentCatalogRow {
            Key = "issued-row-key", Session = new() { Id = "child", DeviceId = mac.DeviceId, ParentSessionId = "parent" },
            CanOrganize = true, CanArchive = true
        };
        var sent = new List<Intent>();
        using (var menu = new ContextMenuStrip())
        {
            AgentRowMenu.Populate(menu, row, sent.Add);
            foreach (ToolStripItem item in menu.Items) item.PerformClick();
            Check(sent.Count == 3 && sent[0].AgentOrganize.Action == Organize.Pin
                && sent[1].AgentOrganize.Action == Organize.Archive
                && sent.Take(2).All(i => i.AgentOrganize.RowKey == row.Key)
                && sent[2].OpenAgent.DeviceId == mac.DeviceId && sent[2].OpenAgent.Session == "parent",
                "typed actions use the selected row");
        }
        row.Session.Pinned = true;
        row.CanArchive = false;
        row.Organizing = true;
        using (var menu = new ContextMenuStrip())
        {
            AgentRowMenu.Populate(menu, row, sent.Add);
            Check(menu.Items.Count == 2 && menu.Items[0].Text == "Unpin" && !menu.Items[0].Enabled
                && menu.Items[1].Enabled, "pending organization and independent parent navigation");
        }
        Console.WriteLine("Passed CLI/native forms, refused and unavailable drafts, and catalog row actions.");
    }
}

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
    static void Main(string[] args)
    {
        Application.SetHighDpiMode(HighDpiMode.SystemAware);
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        string? captures = args switch {
            [] => null,
            ["--screenshots", var directory] => Directory.CreateDirectory(directory).FullName,
            _ => throw new ArgumentException("Usage: WindowsAgentCatalog [--screenshots DIRECTORY]")
        };
        ChatOptionsTests.Run();
        Console.WriteLine("Passed conversation options, refresh and scope changes.");
        InvitePanelTests.Run();
        Console.WriteLine("Passed network invite pixel decoding, copy, renewal, expiry and clearing.");
        DesktopChatSidebarTests.Run(captures);
        var win = new MemberView { DeviceId = "fixture-w", Name = "Test device" };
        var mac = new MemberView { DeviceId = "fixture-m", Name = "Test Mac" };
        var devices = new[] { win, mac.Clone() };
        devices[1].Name = win.Name;
        string? selected = null;
        using (var menu = DeviceMenu.Create(devices, mac.DeviceId, id => selected = id))
        {
            Check(menu.Items.Count == 3 && ((ToolStripMenuItem)menu.Items[2]).Checked,
                  "device filter keeps the exact selected host even with duplicate labels");
            menu.Items[1].PerformClick();
            Check(selected == win.DeviceId, "device selection returns its opaque ID");
            menu.Items[0].PerformClick();
            Check(selected == "*", "all-devices filter");
        }
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

        CaptureDesktopForm(view, captures);

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
    static void CaptureDesktopForm(AgentsView view, string? captures)
    {
        view.NativeCreateDraft = null;
        using var form = new AgentSessionDialog(view, true);
        form.ShowInTaskbar = false;
        form.StartPosition = FormStartPosition.Manual;
        form.Location = new Point(-20000, -20000);
        var prompt = Field<TextBox>(form, "Prompt");
        var submit = Field<Button>(form, "Submit");
        void Capture(string name)
        {
            var fields = (TableLayoutPanel)prompt.Parent!;
            var cells = fields.Controls.Cast<Control>().OrderBy(fields.GetRow).ToArray();
            Check(cells.Zip(cells.Skip(1)).All(pair => pair.First.Bottom <= pair.Second.Top),
                  "fields and help must not overlap");
            Check(prompt.ClientSize.Height >= 3 * prompt.Font.Height, "prompt needs three visible lines");
            if (captures == null) return;
            using var bitmap = new Bitmap(form.Width, form.Height);
            form.DrawToBitmap(bitmap, new Rectangle(Point.Empty, form.Size));
            bitmap.Save(Path.Combine(captures, name + ".png"), System.Drawing.Imaging.ImageFormat.Png);
        }
        Exception? captureError = null;
        form.Shown += (_, _) => form.BeginInvoke(new Action(() =>
        {
            try
            {
                Check(!submit.Enabled, "empty desktop form");
                Capture("desktop-session-empty");
                Field<TextBox>(form, "Directory").Text = "/work/example";
                Field<ComboBox>(form, "Provider").SelectedItem = "codex";
                prompt.Text = "Synthetic screenshot fixture.";
                Check(submit.Enabled, "filled desktop form");
                Capture("desktop-session-filled");
                form.Size = form.MinimumSize;
                form.PerformLayout();
                var viewport = (ScrollableControl)prompt.Parent!.Parent!;
                Check(viewport.VerticalScroll.Visible && submit.Visible, "compact form keeps scrolling and submission");
                submit.PerformClick();
            }
            catch (Exception ex)
            {
                captureError = ex;
                form.DialogResult = DialogResult.Cancel;
            }
        }));
        var result = form.ShowDialog();
        if (captureError != null) throw captureError;
        Check(result == DialogResult.OK, "click submits form");
        var request = form.Request().NativeCreate;
        Check(request.Directory == "/work/example" && request.Prompt == "Synthetic screenshot fixture."
              && request.Provider == "codex", "captured controls produce the intended request");
        Console.WriteLine($"Captured form at {form.DeviceDpi} DPI using {typeof(AgentsView).Assembly.Location}");
    }

}

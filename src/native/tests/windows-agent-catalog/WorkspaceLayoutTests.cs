using System.Diagnostics;
using System.Drawing.Imaging;
using Muster.App;
using Muster.V1;
using Muster.Windows;
using Page = Muster.App.Screen;

internal static class WorkspaceLayoutTests
{
    public static void Run(string? captures)
    {
        // A nonexistent fixture socket keeps this real form independent of the installed node.
        using var form = new Desktop(true, Path.Combine(Path.GetTempPath(), Guid.NewGuid().ToString("N"), "musterd.sock"));
        form.ShowInTaskbar = false;
        form.StartPosition = FormStartPosition.Manual;
        form.Location = new Point(-20000, -20000);
        form.Show();
        var timeout = Stopwatch.StartNew();
        while (!form.Controls.Find("WorkspaceDetail", true).Single().Controls.OfType<Label>()
                   .Any(label => label.Text == "Muster could not connect"))
        {
            Application.DoEvents();
            if (timeout.Elapsed.TotalSeconds > 12) throw new Exception("Isolated form did not finish startup");
            Thread.Sleep(20);
        }
        // Project the connected workspace after the isolated startup failure.
        form.Controls.OfType<TableLayoutPanel>().Single().Enabled = true;
        var directory = new AgentsView { DeviceId = "*" };
        var group = new AgentCatalogSection { Key = "recent", Title = "Recent sessions", Count = 4, Expanded = true };
        foreach (string title in new[] { "Review the onboarding flow", "Release checklist", "Investigate a slow query", "Plan the next iteration" })
            group.Rows.Add(new AgentCatalogRow { Key = title, SourceLabel = "Codex", LaunchLabel = "CLI",
                Session = new() { Id = title, Title = title, DeviceId = "sample", DeviceName = "Workstation" } });
        directory.Sections.Add(group);
        var member = new MemberView { DeviceId = "sample", Name = "Workstation" };
        directory.CreateHosts.Add(member);
        directory.NativeCreateHosts.Add(member);
        var vm = new ViewModel { Screen = Page.Agents, Workspace = Page.Agents, Agents = directory, ServiceDevices = { member } };
        var workspace = form.Controls.Find("Workspace", true).Single();
        var detail = form.Controls.Find("WorkspaceDetail", true).Single();
        var sidebar = workspace.Controls.OfType<AgentSidebar>().Single();
        var composer = detail.Controls.OfType<ComposerSurface>().Single().Controls.OfType<ChatInput>().Single();
        void Capture(string name)
        {
            if (captures == null) return;
            using var bitmap = new Bitmap(form.ClientSize.Width, form.ClientSize.Height);
            // Capture the whole client, including native child controls, without the offscreen title bar.
            using var whole = new Bitmap(form.Width, form.Height);
            form.DrawToBitmap(whole, new Rectangle(Point.Empty, form.Size));
            var origin = form.PointToScreen(Point.Empty) - new Size(form.Left, form.Top);
            using (var g = Graphics.FromImage(bitmap))
                g.DrawImage(whole, new Rectangle(Point.Empty, bitmap.Size), new Rectangle(origin, bitmap.Size), GraphicsUnit.Pixel);
            bitmap.Save(Path.Combine(captures, name), ImageFormat.Png);
        }
        foreach (int width in new[] { 1160, 760 })
        {
            form.ClientSize = form.LogicalToDeviceUnits(new Size(width, 650));
            vm.Screen = Page.Agents;
            form.Render(vm);
            Application.DoEvents();
            if (!sidebar.Visible || detail.Left != sidebar.Right || detail.Height != sidebar.Height ||
                !detail.Controls.OfType<Panel>().SelectMany(panel => panel.Controls.OfType<WorkspaceWelcome>()).Single().Visible)
                throw new Exception("Directory and welcome did not split the workspace");
            if (width == 1160) Capture("workspace-agents.png");
            vm.Screen = Page.Chat;
            vm.Chat = new ChatView { DeviceId = "sample", Session = group.Rows[0].Session,
                MessageCount = 2, Draft = new() { Text = "Review the first-run screen next." }, ComposerPlaceholder = "Reply to this session",
                Timeline = {
                    new ChatBlock { Id = "q", Kind = "message", Message = new AgentMessage
                        { Id = "q", Role = "user", Kind = "text", Text = "Which part of onboarding should we review first?" } },
                    new ChatBlock { Id = "a", Kind = "message", Message = new AgentMessage
                        { Id = "a", Role = "assistant", Kind = "text", Text = "## First run\n\nStart with the **first-run window**. Check the `Continue` button.\n\n- Open the next screen\n- Return to the conversation." } }
                } };
            form.Render(vm);
            Application.DoEvents();
            var transcript = detail.Controls.OfType<Panel>().SelectMany(panel => panel.Controls.OfType<HistoryTranscript>()).Single();
            if (!transcript.Visible || !transcript.Text.Contains("Start with the first-run window."))
                throw new Exception("Conversation body missing from the reader");
            int emphasis = transcript.Text.IndexOf("first-run window", StringComparison.Ordinal);
            transcript.Select(emphasis, "first-run window".Length);
            if (transcript.SelectionFont?.Bold != true) throw new Exception("Markdown emphasis lost");
            transcript.Select(transcript.Text.IndexOf("Continue", StringComparison.Ordinal), 8);
            if (transcript.SelectionFont?.Name != "Consolas") throw new Exception("Inline code font lost");
            transcript.Select(0, 0);
            var first = transcript.GetPositionFromCharIndex(0);
            var last = transcript.GetPositionFromCharIndex(transcript.TextLength - 1);
            if (first.Y < 0 || last.Y >= transcript.ClientSize.Height)
                throw new Exception($"Short conversation outside viewport at {width}: {first}..{last}, size {transcript.ClientSize}");
            if (!composer.Visible || sidebar.RectangleToScreen(sidebar.ClientRectangle).IntersectsWith(
                    composer.RectangleToScreen(composer.ClientRectangle)) || detail.Height != sidebar.Height)
                throw new Exception("Composer displaced or overlapped the directory");
            var submit = composer.Parent!.Controls.OfType<Button>().Single();
            if (!submit.Visible || !submit.Enabled || submit.Width < submit.PreferredSize.Width)
                throw new Exception("Send action unavailable or clipped");
            Capture(width == 1160 ? "workspace-conversation.png" : "workspace-compact.png");
        }
        Console.WriteLine($"Passed full workspace split, welcome and composer at two widths, {form.DeviceDpi} DPI.");
    }
}

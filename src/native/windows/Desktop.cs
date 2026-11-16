using Google.Protobuf;
using Muster.App;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;
using Page = Muster.App.Screen;

namespace Muster.Windows;

internal sealed class Desktop : Form
{
    sealed record Row(string Label, Intent Action)
    {
        public override string ToString() => Label;
    }
    readonly FlowLayoutPanel nav = Bar(), actions = Bar();
    readonly Label title = new() { Dock = DockStyle.Top, Height = 46, TextAlign = ContentAlignment.MiddleLeft,
                                   Font = new Font("Segoe UI", 17) };
    readonly Label status = new() { Dock = DockStyle.Bottom, Height = 30, AutoEllipsis = true };
    readonly ListBox list = new() { Dock = DockStyle.Left, Width = 310, IntegralHeight = false };
    readonly TextBox output = new() { Dock = DockStyle.Fill, Multiline = true, ScrollBars = ScrollBars.Both,
                                      ReadOnly = true, WordWrap = false };
    readonly TextBox input =
        new() { Dock = DockStyle.Bottom, Height = 58, Multiline = true, ScrollBars = ScrollBars.Vertical };
    readonly System.Windows.Forms.Timer timer = new() { Interval = 120 };
    readonly ConcurrentQueue<(ulong, Effect)> effects = new();
    readonly Core core;
    ViewModel? pending, current;
    bool opening, ready;
    (string Key, string Text, string Id)? lastSend;
    string inputContext = "";
    static FlowLayoutPanel Bar() => new() { Dock = DockStyle.Top, AutoSize = true, WrapContents = true,
                                            Padding = new Padding(0, 2, 0, 2) };
    static void ClearBar(FlowLayoutPanel bar)
    {
        while (bar.Controls.Count > 0)
            bar.Controls[0].Dispose();
    }
    static void Button(FlowLayoutPanel bar, string text, Action action)
    {
        var b = new Button { Text = text, AutoSize = true, Height = 29, TabStop = true };
        b.Click += (_, _) => action();
        bar.Controls.Add(b);
    }
    public Desktop()
    {
        Text = "Muster";
        MinimumSize = new Size(740, 480);
        Size = new Size(1050, 700);
        Padding = new Padding(16);
        var body = new Panel { Dock = DockStyle.Fill };
        body.Controls.Add(output);
        body.Controls.Add(list);
        actions.Dock = DockStyle.Bottom;
        Controls.Add(body);
        Controls.Add(input);
        Controls.Add(actions);
        Controls.Add(status);
        Controls.Add(title);
        Controls.Add(nav);
        Button(nav, "Back", () => Send(new() { Navigate = new() { Back = true } }));
        Button(nav, "Network", () => Go(Page.Home));
        Button(nav, "Agents", () => Send(new() { OpenAgents = new() { DeviceId = "*" } }));
        Button(nav, "Chats", () => Send(new() { OpenDesktopChats = new() }));
        Button(nav, "Terminals",
               () => Send(new() { OpenTerminals = new() { DeviceId = current?.SelectedDevice ?? "" } }));
        Button(nav, "Browser", () => Send(new() { OpenBrowser = new() { Device = current?.SelectedDevice ?? "" } }));
        Button(nav, "Multica", () => Go(Page.Multica));
        Button(nav, "Settings", () => Go(Page.Settings));
        Button(nav, "Refresh", () => Send(new() { Refresh = new() }));
        list.DoubleClick += (_, _) => OpenRow();
        list.KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter)
            {
                OpenRow();
                e.Handled = true;
            }
        };
        core = new(v => Interlocked.Exchange(ref pending, v), (id, e) => effects.Enqueue((id, e)));
        timer.Tick += (_, _) =>
        {
            if (!ready)
                return;
            if (Interlocked.Exchange(ref pending, null) is {} v)
                Render(v);
            while (effects.TryDequeue(out var item))
                EffectReceived(item.Item1, item.Item2);
        };
        Shown += async (_, _) => await Connect();
        Activated += (_, _) => core.Lifecycle(0);
        Deactivate += (_, _) => core.Lifecycle(1);
        FormClosing += (_, e) =>
        {
            if (opening)
            {
                e.Cancel = true;
                return;
            }
            timer.Stop();
            core.Dispose();
        };
    }
    async Task Connect()
    {
        if (opening)
            return;
        opening = true;
        nav.Enabled = false;
        ClearBar(actions);
        title.Text = "Connecting to Muster";
        try
        {
            await Task.Run(core.Open);
            ready = true;
            nav.Enabled = true;
            timer.Start();
            core.Lifecycle(0);
            Send(new() { Refresh = new() });
        }
        catch (Exception e)
        {
            title.Text = "Muster could not connect";
            output.Text = e.Message + "\r\n\r\n" +
                          Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                                       "Muster", "musterd.log");
            Button(actions, "Retry", async () => await Connect());
        }
        finally
        {
            opening = false;
        }
    }
    void OpenRow()
    {
        if (list.SelectedItem is Row r)
            Send(r.Action);
    }
    void Go(Page page) => Send(new() { Navigate = new() { Screen = page } });
    void Send(Intent i)
    {
        core.Send(i);
    }
    async void EffectReceived(ulong id, Effect e)
    {
        var result = new EffectResult();
        if (e.Notify != null)
        {
            status.Text = e.Notify.Title + "  " + e.Notify.Body;
            result.Ok = true;
        }
        else if (e.LocalAuth != null && OperatingSystem.IsWindowsVersionAtLeast(10, 0, 22000))
        {
            try
            {
                var consent = await global::Windows.Security.Credentials.UI.UserConsentVerifierInterop
                                  .RequestVerificationForWindowAsync(Handle, e.LocalAuth.Reason);
                result.Ok = consent == global::Windows.Security.Credentials.UI.UserConsentVerificationResult.Verified;
                if (!result.Ok)
                    result.Error = consent.ToString();
            }
            catch (Exception ex)
            {
                result.Error = ex.Message;
            }
        }
        else if (e.OpenUrl != null && Uri.TryCreate(e.OpenUrl.Url, UriKind.Absolute, out var url) &&
                 (url.Scheme == "https" || url.Scheme == "http"))
        {
            if (MessageBox.Show(this, url.ToString(), "Open link", MessageBoxButtons.OKCancel) == DialogResult.OK)
            {
                try
                {
                    Process.Start(new ProcessStartInfo(url.ToString()) { UseShellExecute = true });
                    result.Ok = true;
                }
                catch (Exception ex)
                {
                    result.Error = ex.Message;
                }
            }
        }
        else
            result.Error = $"{e.EfCase} is unavailable on this Windows client.";
        core.Answer(id, result);
    }
    void Render(ViewModel v)
    {
        current = v;
        var page = v.Screen;
        title.Text=page is Page.Home or Page.Onboarding ? v.Network?.Name is {Length:>0} n ? n : "Muster network" : page.ToString();
        status.Text = v.Toasts.LastOrDefault()?.Text ?? v.Join?.Message ?? "";
        var rows = new List<Row>();
        var lines = new List<string>();
        void Row(string name, Intent intent) => rows.Add(new(name, intent));
        void Act(string label, Func<Intent> intent) => Button(actions, label, () => Send(intent()));
        ClearBar(actions);
        var inputKey = $"{page}/{v.SelectedDevice}/{v.Chat?.Draft?.Key}/{v.Terminal?.Session}";
        if (inputContext != inputKey)
        {
            inputContext = inputKey;
            input.Text = page == Page.Chat ? v.Chat?.Draft?.Text ?? "" : "";
        }
        input.Visible = page is Page.Onboarding or Page.Home or Page.Join or Page.Chat or Page.Terminal or Page.Browser;
        input.PlaceholderText = "";
        switch (page)
        {
        case Page.Home:
        case Page.Onboarding:
        case Page.Join:
        case Page.Device:
        case Page.Settings:
        case Page.Invite:
            if (v.Self != null)
                lines.Add(
                    $"{v.Self.Name}    {v.Self.Fingerprint}\r\n{v.Self.Platform}    {v.Self.Version}    {v.Self.KeyLabel}");
            if (v.Network != null)
            {
                lines.Add(v.Network.VnetCidr);
                foreach (var m in v.Network.Members)
                {
                    Row($"{m.Name}    {m.Vip}", new() { SelectDevice = new() { DeviceId = m.DeviceId } });
                    if (m.DeviceId == v.SelectedDevice)
                        lines.Add(
                            $"{m.Name}\r\n{m.Fingerprint}\r\n{m.ConnectionStatus}\r\n{string.Join(", ",m.Scopes)}");
                }
            }
            if (v.Network?.HasNetwork != true)
            {
                input.PlaceholderText = "Network name or invitation";
                Act("Create network", () => new() { CreateNetwork = new() { Name = input.Text.Trim() } });
                Act("Join with invite", () => new() { JoinNetwork = new() { InviteText = input.Text.Trim(),
                                                                            DeviceName = Environment.MachineName,
                                                                            DeviceKind = "desktop" } });
            }
            if (v.CanInvite)
            {
                Act("Create invite", () => new() { ShowInvite = new() { TtlSecs = 300 } });
                foreach (var p in v.Pending)
                {
                    Button(actions, "Review " + p.Name,
                           () =>
                           {
                               using var dialog =
                                   new AccessDialog("Approve device", p.Name + "\r\n" + p.Fingerprint, v.JoinAccess);
                               if (dialog.ShowDialog(this) == DialogResult.OK)
                                   Send(new() { DecideJoin = new() { DeviceId = p.DeviceId, Approve = true,
                                                                     Scopes = { dialog.Scopes } } });
                           });
                    Act("Decline " + p.Name, () => new() { DecideJoin = new() { DeviceId = p.DeviceId } });
                }
            }
            if (page == Page.Device && v.DeviceActions?.CanEditAccess == true &&
                v.Network?.Members.FirstOrDefault(m => m.DeviceId == v.SelectedDevice) is {} member)
            {
                Button(actions, "Edit access",
                       () =>
                       {
                           using var dialog = new AccessDialog("Save access", member.Name + "\r\n" + member.Fingerprint,
                                                               v.DeviceAccess);
                           if (dialog.ShowDialog(this) == DialogResult.OK)
                               Send(new() { SetScopes = new() { DeviceId = member.DeviceId, Role = member.Role,
                                                                Scopes = { dialog.Scopes } } });
                       });
            }
            if (page == Page.Invite && v.Invite != null)
                lines.Add(v.Invite.Text);
            if (v.SelectedDevice.Length > 0)
            {
                Act("Agents on device", () => new() { OpenAgents = new() { DeviceId = v.SelectedDevice } });
                Act("Terminals on device", () => new() { OpenTerminals = new() { DeviceId = v.SelectedDevice } });
            }
            if (page == Page.Settings && v.Policy is {} policy)
            {
                lines.Add("Agent access: " + (policy.OfferAgent ? "On" : "Off") +
                          "\r\nTerminal access: " + (policy.OfferTerminal ? "On" : "Off"));
                Act((policy.OfferAgent ? "Disable" : "Enable") + " agent sharing",
                    () =>
                    {
                        var p = policy.Clone();
                        p.OfferAgent = !p.OfferAgent;
                        return new() { SetPolicy = new() { Policy = p } };
                    });
                Act((policy.OfferTerminal ? "Disable" : "Enable") + " terminal sharing",
                    () =>
                    {
                        var p = policy.Clone();
                        p.OfferTerminal = !p.OfferTerminal;
                        return new() { SetPolicy = new() { Policy = p } };
                    });
            }
            break;
        case Page.Agents:
            if (v.Agents != null)
            {
                foreach (var a in v.Agents.Sessions)
                    Row($"{a.Title}    {a.DeviceName}",
                        new() { OpenAgent = new() { Session = a.Id, DeviceId = a.DeviceId } });
                lines.AddRange(v.Agents.Errors);
            }
            break;
        case Page.DesktopChats:
            if (v.DesktopChats != null)
            {
                var c = v.DesktopChats;
                foreach (var choice in c.Choices)
                    Act(choice.Label, () => new() { OpenDesktopChats = new() { Provider = choice.Id } });
                if (c.Catalog != null)
                    foreach (var chat in c.Catalog.Chats)
                        Row(chat.Title, new() { DesktopChatOpen = new() { Chat = chat.Id, Desktop = true } });
                lines.Add(c.Error);
            }
            break;
        case Page.Chat:
            if (v.Chat is {} chatView)
            {
                title.Text = chatView.Session?.Title ?? "Chat";
                foreach (var b in chatView.Timeline)
                    lines.AddRange(Block(b));
                lines.Add(chatView.LoadError);
                input.PlaceholderText = chatView.ComposerPlaceholder;
                if (lastSend is {} sent && sent.Id == chatView.SentDraftId)
                {
                    if (sent.Key == chatView.Draft?.Key && input.Text == sent.Text)
                        input.Clear();
                    lastSend = null;
                }
                if (chatView.Draft != null && !chatView.Sending && chatView.ComposerDisabledReason.Length == 0)
                    Button(
                        actions, "Send",
                        () =>
                        {
                            if (string.IsNullOrWhiteSpace(input.Text))
                                return;
                            var draft = chatView.Draft.Clone();
                            draft.Text = input.Text;
                            if (lastSend is not {} previous || previous.Key != draft.Key || previous.Text != draft.Text)
                                lastSend = (draft.Key, draft.Text, Guid.NewGuid().ToString());
                            Send(new() { ComposerAction = new() {
                                Draft = draft,
                                Action = new() { AgentSend = new() { Text = draft.Text, DraftId = lastSend.Value.Id } }
                            } });
                        });
                foreach (var i in chatView.Interactions)
                {
                    lines.Add(i.Prompt);
                    foreach (var option in i.Options)
                        Act(option, () => new() { AgentRespond = new() { Interaction = i.Id, Option = option } });
                }
                if (chatView.CanInterrupt)
                    Act("Interrupt", () => new() { AgentInterrupt = new() });
                if (chatView.HasEarlier)
                    Act("Earlier messages", () => new() { AgentLoadEarlier = new() });
                if (chatView.CanBrowseFiles)
                {
                    Act("Files", () => new() { OpenFiles = new() });
                    Act("Git history", () => new() { OpenGit = new() });
                }
            }
            break;
        case Page.Terminals:
            if (v.Terminals is {} t)
            {
                foreach (var s in t.Sessions)
                    Row($"{s.Title}    {s.DeviceName}",
                        new() { OpenTerminal = new() { Session = s.Id, DeviceId = s.DeviceId } });
                if (t.CanCreate)
                    foreach (var src in t.Sources)
                        Act("New " + src.Label, () => new() { TerminalCreate = new() { Source = src.Source } });
                lines.AddRange(t.Errors);
            }
            break;
        case Page.Terminal:
            if (v.Terminal is {} term)
            {
                lines.AddRange(term.Lines.Select(l => l.Text));
                input.PlaceholderText = "Terminal input";
                if (!term.Ended)
                {
                    Button(actions, "Send line",
                           () =>
                           {
                               Send(new() { TerminalText = new() { Text = input.Text } });
                               Send(new() { TerminalKey = new() { Key = "enter" } });
                               input.Clear();
                           });
                    Act("Ctrl+C", () => new() { TerminalKey = new() { Key = "ctrl-c" } });
                }
                Act("Close terminal", () => new() { CloseTerminal = new() });
            }
            break;
        case Page.Files:
            if (v.Files is {} files)
            {
                title.Text = files.Path;
                foreach (var f in files.Entries)
                {
                    var path = (files.Path.TrimEnd('/') + "/" + f.Name).TrimStart('/');
                    Row(f.Name, f.Dir ? new() { OpenFiles = new() { Path = path } }
                                      : new() { OpenFile = new() { Path = path } });
                }
            }
            break;
        case Page.File:
            if (v.File != null)
            {
                title.Text = v.File.Path;
                lines.Add(v.File.Error);
                lines.Add(v.File.Content.ToStringUtf8());
            }
            break;
        case Page.Git:
            if (v.Git is {} git)
            {
                foreach (var commit in git.Commits)
                    Row(commit.Subject, new() { OpenGit = new() { DiffCommit = commit.Hash } });
                lines.Add(git.Diff);
            }
            break;
        case Page.Browser:
            if (v.Browser is {} browser)
            {
                input.PlaceholderText = "Browser operation (JSON)";
                lines.Add(browser.Result);
                Act("Run browser operation", () => new() { BrowserRun = new() { Json = input.Text } });
            }
            break;
        case Page.Multica:
            if (v.MulticaWorkspace is {} space)
            {
                foreach (var d in space.Devices)
                    Row(d.Name, new() { MulticaSelect = new() { DeviceId = d.DeviceId } });
                if (v.Multica != null)
                    lines.Add(v.Multica.Message);
                if (v.MulticaPage is {} mp)
                {
                    lines.Add(mp.Error);
                    foreach (var ws in mp.Workspaces)
                        Row(ws.Name, new() { MulticaBrowse = new() { WorkspaceId = ws.Id } });
                    foreach (var issue in mp.Issues)
                        Row(issue.Identifier + "  " + issue.Title,
                            new() { MulticaBrowse = new() { WorkspaceId = mp.WorkspaceId, IssueId = issue.Id } });
                    foreach (var agent in mp.Agents)
                        Row(agent.Name,
                            new() { MulticaBrowse = new() { WorkspaceId = mp.WorkspaceId, AgentId = agent.Id } });
                    if (mp.Issue != null)
                        lines.Add(mp.Issue.Description);
                    if (mp.Agent != null)
                        lines.Add(mp.Agent.Description);
                    foreach (var comment in mp.Comments)
                        lines.Add(comment.Content);
                    foreach (var run in mp.Runs)
                        lines.Add(run.Status + "  " + run.FailureReason);
                    if (mp.Offset != 0)
                        Act("Previous page", () => new() { MulticaBrowse = new() { WorkspaceId = mp.WorkspaceId,
                                                                                   Offset = mp.PreviousOffset } });
                    if (mp.HasMore)
                        Act("Next page", () => new() { MulticaBrowse = new() { WorkspaceId = mp.WorkspaceId,
                                                                               Offset = mp.NextOffset } });
                }
            }
            break;
        }
        if (!rows.SequenceEqual(list.Items.Cast<Row>()))
        {
            var old = list.SelectedItem as Row;
            list.BeginUpdate();
            list.Items.Clear();
            list.Items.AddRange(rows.ToArray());
            if (old != null)
                list.SelectedItem = old;
            list.EndUpdate();
        }
        list.Visible = rows.Count > 0;
        var text = string.Join("\r\n", lines.Where(x => !string.IsNullOrEmpty(x)));
        if (output.Text != text)
        {
            int pos = output.SelectionStart;
            output.Text = text;
            output.Select(Math.Min(pos, text.Length), 0);
        }
    }
    static IEnumerable<string> Block(ChatBlock b)
    {
        if (b.Title.Length > 0)
            yield return b.Title;
        if (b.Message is {} m)
            yield return $"{m.Role}  {m.Text}";
        foreach (var child in b.Children)
            foreach (string line in Block(child))
                yield return line;
    }
}

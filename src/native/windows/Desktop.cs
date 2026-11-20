using Google.Protobuf;
using Muster.App;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;
using Page = Muster.App.Screen;

namespace Muster.Windows;

internal sealed class Desktop : Form
{
    sealed record Row(string Label, Intent Action, string CatalogKey = "")
    {
        public override string ToString() => Label;
    }
    readonly FlowLayoutPanel nav = Bar(), actions = Bar();
    readonly Label title = new() { Dock = DockStyle.Top, AutoEllipsis = true, TextAlign = ContentAlignment.MiddleLeft,
                                   Font = new Font("Segoe UI", 17) };
    readonly Label status = new() { Dock = DockStyle.Bottom, Height = 30, AutoEllipsis = true };
    readonly ListBox list = new() { Dock = DockStyle.Left, Width = 310, IntegralHeight = false, HorizontalScrollbar = true };
    readonly TextBox output = new() { Dock = DockStyle.Fill, Multiline = true, ScrollBars = ScrollBars.Both,
                                      ReadOnly = true, WordWrap = false };
    readonly Label chatWelcome = new() { Dock = DockStyle.Fill, Visible = false, TextAlign = ContentAlignment.MiddleCenter,
        Text = "Select a conversation or start a new chat." };
    readonly DesktopChatSidebar chatSidebar = new() { Dock = DockStyle.Left, Visible = false };
    readonly TranscriptSteps transcript = new();
    readonly ConversationTerminal conversationTerminal = new() { Dock = DockStyle.Fill, Visible = false };
    RunHistoryDialog? runHistory;
    (string Device, string Session)? terminalTab;
    readonly TerminalGrid terminal = new() { Dock = DockStyle.Fill, Visible = false };
    readonly SourcePreview preview = new() { Dock = DockStyle.Fill, Visible = false };
    readonly ChatInput input =
        new() { Dock = DockStyle.Bottom, Height = 58, Multiline = true, AcceptsReturn = true, ScrollBars = ScrollBars.Vertical };
    readonly System.Windows.Forms.Timer timer = new() { Interval = 120 };
    readonly ConcurrentQueue<(ulong, Effect)> effects = new();
    readonly Core core;
    readonly List<(Button Button, bool NeedsText)> composerButtons = new();
    void UpdateComposerButtons()
    {
        foreach (var (button, needsText) in composerButtons)
            button.Enabled = needsText ? input.CanSubmitPayload : input.TextLength == 0;
    }
    ViewModel? pending, current;
    bool opening, ready;
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
        b.Tag = action;
        b.Click += (_, _) => ((Action)b.Tag!)();
        bar.Controls.Add(b);
    }
    readonly bool deviceAuthAvailable;
    public Desktop(bool attachOnly) : this(attachOnly, null) { }
    internal Desktop(bool attachOnly, string? fixtureSocket)
    {
        deviceAuthAvailable = fixtureSocket == null;
        Text = !deviceAuthAvailable ? "Muster (isolated development)" : attachOnly ? "Muster (development)" : "Muster";
        MinimumSize = new Size(740, 480);
        Size = new Size(1050, 700);
        Padding = new Padding(16);
        void SizeTitle() => title.Height = title.Font.Height + LogicalToDeviceUnits(8);
        title.FontChanged += (_, _) => SizeTitle();
        title.DpiChangedAfterParent += (_, _) => SizeTitle();
        SizeTitle();
        var body = new Panel { Dock = DockStyle.Fill };
        body.Controls.Add(conversationTerminal);
        terminal.ClipboardError += message => status.Text = message;
        body.Controls.Add(terminal);
        body.Controls.Add(preview);
        body.Controls.Add(output);
        body.Controls.Add(chatWelcome);
        body.Controls.Add(list);
        body.Controls.Add(chatSidebar);
        body.Resize += (_, _) => chatSidebar.Width = Math.Min(LogicalToDeviceUnits(340), body.ClientSize.Width / 3);
        actions.Dock = DockStyle.Bottom;
        Controls.Add(body);
        Controls.Add(input);
        Controls.Add(actions);
        Controls.Add(status);
        Controls.Add(title);
        Controls.Add(nav);
        core = new(v => Interlocked.Exchange(ref pending, v), (id, e) => effects.Enqueue((id, e)), attachOnly, fixtureSocket);
        chatSidebar.Intent += Send;
        terminal.Intent += core.Send;
        conversationTerminal.Intent += core.Send;
        conversationTerminal.ReturnToChat += () =>
        {
            if (current == null || terminalTab == null) return;
            terminalTab = null;
            core.Send(new() { SessionTerminalAction = new() { Action = SessionTerminalAction.Types.Action.Detach } });
            Render(current);
        };
        Button(nav, "Back", () => Send(new() { Navigate = new() { Back = true } }));
        Button(nav, "Network", () => Go(Page.Home));
        Button(nav, "Agents", () => Send(new() { OpenAgents = new() { DeviceId = "*" } }));
        Button(nav, "Chats", () => Send(new() { OpenDesktopChats = new() }));
        Button(nav, "Terminals", () =>
        {
            string device = current?.Screen switch
            {
                Page.Agents => current.Agents?.DeviceId ?? "",
                Page.Device => current.SelectedDevice,
                _ => ""
            };
            Send(new() { OpenTerminals = new() { DeviceId = device } });
        });
        Button(nav, "Browser", () => Send(new() { OpenBrowser = new() { Device = current?.SelectedDevice ?? "" } }));
        if (deviceAuthAvailable)
            Button(nav, "Presence key", () => ShowPresenceReport(() => Presence.ReadReport(core.Socket)));
        Button(nav, "Settings", () => Go(Page.Settings));
        Button(nav, "Refresh", () => Send(new() { Refresh = new() }));
        input.ReturnPressed += () => Submit();
        input.AllowDrop = true;
        input.DragEnter += (_, e) =>
        {
            if (input.CanImport && e.Data != null && ChatInput.CanImportData(e.Data))
                e.Effect = e.AllowedEffect & DragDropEffects.Copy;
        };
        input.DragDrop += (_, e) =>
        {
            if (input.CanImport && e.Data != null)
                Import(() => input.ImportData(e.Data));
        };
        input.KeyDown += (_, e) =>
        {
            if (input.CanImport && e.KeyData == (Keys.Control | Keys.V))
                Import(() => {
                    if (Clipboard.GetDataObject() is {} data && ChatInput.CanImportData(data))
                    {
                        e.SuppressKeyPress = true;
                        input.ImportData(data);
                    }
                });
        };
        input.TextChanged += (_, _) => UpdateComposerButtons();
        var rowMenu = new ContextMenuStrip();
        list.ContextMenuStrip = rowMenu;
        FormClosed += (_, _) => rowMenu.Dispose();
        list.MouseDown += (_, e) =>
        {
            if (e.Button == MouseButtons.Right)
                list.SelectedIndex = list.IndexFromPoint(e.Location);
        };
        rowMenu.Opening += (_, e) =>
        {
            while (rowMenu.Items.Count > 0) rowMenu.Items[0].Dispose();
            if (current?.Screen == Page.Agents && list.SelectedItem is Row { CatalogKey.Length: > 0 } selected)
            {
                var row = current.Agents.Sections.SelectMany(section => section.Rows)
                    .FirstOrDefault(r => r.Key == selected.CatalogKey);
                if (row != null) AgentRowMenu.Populate(rowMenu, row, Send);
            }
            e.Cancel = rowMenu.Items.Count == 0;
        };
        list.DoubleClick += (_, _) => OpenRow();
        list.KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter)
            {
                OpenRow();
                e.Handled = true;
            }
        };
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
    async Task Connect(bool start = false)
    {
        if (opening)
            return;
        opening = true;
        nav.Enabled = false;
        ClearBar(actions);
        title.Text = "Connecting to Muster";
        try
        {
            if (!start && await Task.Run(core.NeedsStart))
            {
                title.Text = "Muster background service is stopped";
                output.Text = "Start musterd to connect to your network.";
                Button(actions, "Start musterd", async () => await Connect(true));
                return;
            }
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
    bool presenceReportOpen;
    async void ShowPresenceReport(Func<Task<string>> query)
    {
        if (presenceReportOpen) return;
        presenceReportOpen = true;
        try
        {
            var report = await query();
            if (!IsDisposed) MessageBox.Show(this, report, "Muster presence key");
        }
        catch (Exception ex)
        {
            if (!IsDisposed) MessageBox.Show(this, ex.Message, "Presence key request failed");
        }
        finally { presenceReportOpen = false; }
    }

    void Go(Page page) => Send(new() { Navigate = new() { Screen = page } });
    void Send(Intent intent) => core.Send(input.WithDraft(intent));
    void Import(Action import)
    {
        try { import(); }
        catch (Exception ex) { MessageBox.Show(this, ex.Message, "Chat attachment"); }
        if (current != null) Render(current);
    }

    void ChooseFiles()
    {
        string? origin = input.DraftKey;
        using var picker = new OpenFileDialog { Title = "Add attachments", Multiselect = true };
        if (picker.ShowDialog(this) == DialogResult.OK && input.CanImport && input.DraftKey == origin)
            Import(() => { foreach (string path in picker.FileNames) input.AddFile(path); });
    }

    async Task ShowPreview(Muster.V1.AgentAttachment attachment)
    {
        try
        {
            using var image = await AttachmentPreview.Decode(attachment.Data, 1600);
            if (IsDisposed || !input.Attachments.Any(item => ReferenceEquals(item, attachment))) return;
            using var dialog = new Form { Text = attachment.Name, ClientSize = new Size(660, 450),
                StartPosition = FormStartPosition.CenterParent, MinimizeBox = false };
            dialog.Controls.Add(new PictureBox { Dock = DockStyle.Fill, SizeMode = PictureBoxSizeMode.Zoom, Image = image });
            dialog.ShowDialog(this);
        }
        catch (Exception ex) { if (!IsDisposed) MessageBox.Show(this, ex.Message, "Image preview"); }
    }

    void PasteImage()
    {
        Import(() =>
        {
            using var image = Clipboard.GetImage() ?? throw new InvalidOperationException("The clipboard contains no image.");
            input.AddImage(image);
        });
    }

    void Submit(string releaseToken = "")
    {
        if (input.Submission(releaseToken) is {} intent)
            Send(intent);
    }
    async void EffectReceived(ulong id, Effect e)
    {
        var result = new EffectResult();
        if (e.Notify != null)
        {
            status.Text = e.Notify.Title + "  " + e.Notify.Body;
            result.Ok = true;
        }
        else if (deviceAuthAvailable && e.Authenticate != null)
        {
            try
            {
                var hwnd = Handle;
                result.Data = ByteString.CopyFrom(await Task.Run(() =>
                    Presence.Sign(hwnd, e.Authenticate.Reason, e.Authenticate.Message.ToByteArray())));
                result.Ok = true;
            }
            catch (Exception ex) { result.Error = ex.Message; }
        }
        else if (deviceAuthAvailable && e.LocalAuth != null && OperatingSystem.IsWindowsVersionAtLeast(10, 0, 22000))
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
    static string Reported(string? value) => string.IsNullOrWhiteSpace(value) ? "not reported" : value;

    void Render(ViewModel v)
    {
        current = v;
        var page = v.Screen;
        runHistory?.Display(page == Page.Chat ? v.Chat : null);
        title.Text=page is Page.Home or Page.Onboarding ? v.Network?.Name is {Length:>0} n ? n : "Muster network" : page.ToString();
        status.Text = v.Toasts.LastOrDefault()?.Text ?? v.Join?.Message ?? "";
        var rows = new List<Row>();
        var lines = new List<string>();
        void Row(string name, Intent intent, string catalogKey = "") => rows.Add(new(name, intent, catalogKey));
        composerButtons.Clear();
        int actionIndex = 0;
        void AddAction(string label, Action invoke)
        {
            if (actionIndex < actions.Controls.Count && actions.Controls[actionIndex] is Button button)
            {
                if (button.Text != label)
                    button.Text = label;
                button.Tag = invoke;
            }
            else
                Button(actions, label, invoke);
            actions.Controls[actionIndex].Enabled = true;
            actionIndex++;
        }
        void DeviceFilter(string selected, Action<string> choose)
        {
            if (v.ServiceDevices.Count == 0) return;
            int index = actionIndex;
            string name = DeviceMenu.Label(v.ServiceDevices, selected);
            AddAction("Device: " + name, () =>
            {
                var anchor = actions.Controls[index];
                var menu = DeviceMenu.Create(v.ServiceDevices, selected, choose);
                menu.Closed += (_, _) => BeginInvoke(new Action(menu.Dispose));
                menu.Show(anchor, new Point(0, anchor.Height));
            });
        }
        void ComposerButton(string label, bool needsText, Action invoke)
        {
            AddAction(label, invoke);
            composerButtons.Add(((Button)actions.Controls[actionIndex - 1], needsText));
        }
        void Act(string label, Func<Intent> intent) => AddAction(label, () => Send(intent()));
        var inputKey = $"{page}/{v.SelectedDevice}/{v.Chat?.Draft?.Key}/{v.Terminal?.Session}";
        input.ShowDraft(page == Page.Chat ? v.Chat : null);
        if (inputContext != inputKey)
        {
            inputContext = inputKey;
            if (page != Page.Chat) input.Clear();
        }
        (string Device, string Session)? chatKey = page == Page.Chat && v.Chat?.Session is {} openChatSession
            ? (v.Chat.DeviceId, openChatSession.Id) : null;
        transcript.Display(page == Page.Chat ? v.Chat : null);
        if (terminalTab != chatKey) terminalTab = null;
        bool inWorkspaceTerminal = terminalTab != null && v.Chat?.Terminal != null;
        input.Visible = !inWorkspaceTerminal && (page is Page.Onboarding or Page.Home or Page.Join or Page.Chat or Page.Browser or Page.Agents);
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
            {
                var platform = page == Page.Settings ? v.Self.Platform : $"{v.Self.Platform}    {v.Self.Version}";
                lines.Add($"{v.Self.Name}    {v.Self.Fingerprint}\r\n{platform}    {v.Self.KeyLabel}");
            }
            if (page == Page.Settings)
            {
                if (deviceAuthAvailable)
                    AddAction("Register presence candidate", () => ShowPresenceReport(() => Presence.Register(Handle, core.Socket)));
                var desktopVersion = typeof(Desktop).Assembly.GetName().Version?.ToString(3);
                lines.Add($"Desktop version    {Reported(desktopVersion)}");
                lines.Add($"Local node version    {Reported(v.Self?.Version)}");
                lines.Add($"Local node commit    {Reported(v.Self?.Commit)}");
            }
            if (v.Network != null)
            {
                lines.Add(v.Network.VnetCidr);
                foreach (var m in v.Network.Members)
                {
                    Row($"{m.Name}    {m.Vip}", new() { SelectDevice = new() { DeviceId = m.DeviceId } });
                    if (m.DeviceId == v.SelectedDevice)
                    {
                        lines.Add(
                            $"{m.Name}\r\n{m.Fingerprint}\r\n{m.ConnectionStatus}\r\n{string.Join(", ",m.Scopes)}");
                        if (page == Page.Device)
                        {
                            lines.Add($"Device version    {Reported(m.Version)}");
                            lines.Add($"Device commit    {Reported(m.Commit)}");
                        }
                    }
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
                    AddAction("Review " + p.Name,
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
                AddAction("Edit access",
                          () =>
                          {
                              using var dialog = new AccessDialog(
                                  "Save access", member.Name + "\r\n" + member.Fingerprint, v.DeviceAccess);
                              if (dialog.ShowDialog(this) == DialogResult.OK)
                                  Send(new() { SetScopes = new() { DeviceId = member.DeviceId, Role = member.Role,
                                                                   Scopes = { dialog.Scopes } } });
                          });
            }
            if (page == Page.Device && v.DeviceActions is {} deviceActions &&
                v.Network?.Members.FirstOrDefault(m => m.DeviceId == v.SelectedDevice) is {} selected)
            {
                if (deviceActions.CanRename)
                    AddAction("Rename device", () =>
                    {
                        var name = Microsoft.VisualBasic.Interaction.InputBox("Device name", "Rename device", selected.Name);
                        if (name.Trim().Length > 0 && name != selected.Name)
                            Send(new() { RenameDevice = new() { DeviceId = selected.DeviceId, Name = name.Trim() } });
                    });
                if (deviceActions.CanRemove)
                    AddAction("Remove device", () =>
                    {
                        if (MessageBox.Show(this, $"Remove {selected.Name} ({selected.Fingerprint}) from the network?",
                                            "Remove device", MessageBoxButtons.YesNo, MessageBoxIcon.Warning,
                                            MessageBoxDefaultButton.Button2) == DialogResult.Yes)
                            Send(new() { RemoveDevice = new() { DeviceId = selected.DeviceId } });
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
            if (v.Agents is {} agents)
            {
                Intent Catalog(AgentCatalogAction.Types.Kind kind, string value = "") =>
                    new() { AgentCatalogAction = new() { Kind = kind, Value = value } };
                void CreateSession(bool desktop)
                {
                    using var dialog = new AgentSessionDialog(agents, desktop);
                    if (dialog.ShowDialog(this) == DialogResult.OK) Send(dialog.Request());
                }
                if (agents.CreateHosts.Count > 0)
                {
                    AddAction(agents.Creating ? "Creating CLI session…" : "New CLI session", () => CreateSession(false));
                    actions.Controls[actionIndex - 1].Enabled = !agents.Creating;
                }
                if (agents.NativeCreateHosts.Count > 0)
                {
                    AddAction(agents.NativeCreating ? "Creating desktop session…" : "New desktop session", () => CreateSession(true));
                    actions.Controls[actionIndex - 1].Enabled = !agents.NativeCreating;
                }
                DeviceFilter(agents.DeviceId, id => Send(new() { OpenAgents = new() { DeviceId = id } }));
                input.PlaceholderText = "Search sessions";
                Act("Search", () => Catalog(AgentCatalogAction.Types.Kind.Search, input.Text));
                if (agents.Query.Length > 0)
                    AddAction("Clear search", () => { input.Clear(); Send(Catalog(AgentCatalogAction.Types.Kind.Search)); });
                foreach (var section in agents.Sections)
                {
                    Row($"{(section.Expanded ? "−" : "+")} {section.Title} ({section.Count})",
                        Catalog(AgentCatalogAction.Types.Kind.ToggleSection, section.Key));
                    foreach (var row in section.Rows)
                    {
                        var session = row.Session;
                        if (session == null) continue;
                        Row($"    {session.Title}    {row.SourceLabel}    {row.LaunchLabel}",
                            new() { OpenAgent = new() { Session = session.Id, DeviceId = session.DeviceId } }, row.Key);
                        if (row.Expandable)
                            Row(row.Expanded ? "      Collapse replies" : "      Expand replies",
                                Catalog(AgentCatalogAction.Types.Kind.ToggleFamily, row.Key));
                    }
                    if (section.Expanded && section.HasMore && !section.LoadingMore)
                        Row("    Load more", Catalog(AgentCatalogAction.Types.Kind.LoadMore, section.Key));
                }
                if (agents.Sections.Count == 0)
                    foreach (var session in agents.Sessions)
                        Row($"{session.Title}    {session.DeviceName}",
                            new() { OpenAgent = new() { Session = session.Id, DeviceId = session.DeviceId } });
                if (agents.SearchHasMore && !agents.Searching)
                    Act("More results", () => Catalog(AgentCatalogAction.Types.Kind.SearchMore));
                lines.AddRange(agents.SearchNotices);
                lines.AddRange(agents.Errors);
                if (agents.Loading || agents.Searching) lines.Add("Loading sessions…");
            }
            break;
        case Page.DesktopChats:
            title.Text = "Chats";
            break;
        case Page.Chat:
            if (v.Chat is {} chatView)
            {
                title.Text = chatView.Session?.Title ?? "Chat";
                if (inWorkspaceTerminal) break;
                if (chatView.Terminal is { Available: true } && chatKey != null)
                    AddAction("Workspace terminal", () =>
                    {
                        terminalTab = chatKey;
                        Send(new() { SessionTerminalAction = new() { Action = SessionTerminalAction.Types.Action.Start } });
                        Render(v);
                    });
                lines.Add(chatView.CodeError);
                if (chatKey != null && chatView.CodeRuns.Count > 0)
                    AddAction("Recent runs", () =>
                    {
                        using var dialog = new RunHistoryDialog(chatView, Send);
                        runHistory = dialog;
                        try { dialog.ShowDialog(this); }
                        finally { runHistory = null; }
                    });
                if (chatView.Session is { ParentSessionId.Length: > 0 } child)
                    Act("Open parent conversation", () => new() { OpenAgent = new() { Session = child.ParentSessionId, DeviceId = chatView.DeviceId } });
                lines.AddRange(transcript.Lines());
                if (transcript.HasStepsMenu)
                {
                    int menuIndex = actionIndex;
                    AddAction("Work steps", () =>
                    {
                        var menu = transcript.Menu(core.Send, () => { if (current != null) Render(current); });
                        menu.Closed += (_, _) => BeginInvoke(new Action(menu.Dispose));
                        var anchor = actions.Controls[menuIndex];
                        menu.Show(anchor, new Point(0, anchor.Height));
                    });
                }
                var changedTurns = WorktreeNavigation.ChangedTurns(chatView.Timeline).ToArray();
                if (changedTurns.Length > 0)
                {
                    int menuIndex = actionIndex;
                    AddAction("Changed files", () =>
                    {
                        var menu = new ContextMenuStrip();
                        WorktreeNavigation.AddChanges(menu, changedTurns, Send);
                        menu.Closed += (_, _) => BeginInvoke(new Action(menu.Dispose));
                        var anchor = actions.Controls[menuIndex];
                        menu.Show(anchor, new Point(0, anchor.Height));
                    });
                }
                lines.Add(chatView.LoadError);
                lines.Add(chatView.ResponseError);
                lines.Add(chatView.ComposerDisabledReason);
                input.PlaceholderText = chatView.ComposerPlaceholder;
                bool busy = chatView.Sending || chatView.Completing;
                bool canSubmit = input.CanSubmit;
                if (input.CanImport)
                {
                    AddAction("Add files", ChooseFiles);
                    AddAction("Paste image", PasteImage);
                }
                if (!input.FitsAttachmentLimits)
                    lines.Add("Attachment limits changed. Remove files before sending.");
                foreach (var attachment in input.Attachments)
                {
                    lines.Add($"{attachment.Name} ({attachment.Data.Length:N0} bytes)");
                    if (attachment.MediaType.StartsWith("image/", StringComparison.OrdinalIgnoreCase))
                        AddAction($"Preview {attachment.Name}", async () => await ShowPreview(attachment));
                    if (!busy)
                        AddAction($"Remove {attachment.Name}", () =>
                        {
                            input.RemoveAttachment(attachment);
                            if (current != null) Render(current);
                        });
                }
                if (canSubmit)
                    ComposerButton("Send", true, () => Submit());
                if (chatView.Unconfirmed is {} refusal)
                {
                    lines.Add("Delivery of the previous message is unconfirmed.");
                    lines.Add(refusal.Message);
                    if (canSubmit)
                        ComposerButton("Retry current draft", true, () => Submit(refusal.Token));
                    if (!busy)
                        Act("Dismiss delivery notice", () => new() { ResolveAgentSend = new() { Token = refusal.Token, Dismiss = true } });
                }
                if (chatView.CanComplete && !busy)
                    ComposerButton("Read suggestion", false, () => Send(new() { ComposerComplete = new() }));
                if (chatView.ComposerSuggestion.Length > 0)
                {
                    lines.Add(chatView.ComposerSuggestion);
                    if (!busy)
                        ComposerButton("Use suggestion", false, () => input.Text = chatView.ComposerSuggestion);
                }
                lines.Add(chatView.CompletionError);
                foreach (var i in chatView.Interactions)
                {
                    lines.Add(i.Prompt);
                    if (chatView.ResponseUnknown || chatView.RespondingInteraction.Length > 0)
                        continue;
                    if (i.Questions.Count > 0)
                        AddAction("Answer questions",
                                  () =>
                                  {
                                      using var dialog = new Questions(i);
                                      if (dialog.ShowDialog(this) == DialogResult.OK)
                                          Send(new() { AgentRespond = new() { Interaction = i.Id,
                                                                              Answers = { dialog.Answers } } });
                                  });
                    else
                        foreach (var option in i.Options)
                            Act(option, () => new() { AgentRespond = new() { Interaction = i.Id, Option = option } });
                }
                if (chatView.CanCreateSideChat)
                    Act("Start side chat", () => new() { SideChatAction = new() });
                if (chatView.CanEndSideChat)
                    Act("End side chat", () => new() { SideChatAction = new() { End = true } });
                if (chatView.CanChooseModel && !chatView.ChatOptionsBusy)
                {
                    Act("Model and effort", () => new() { ChatOptionsLoad = new() });
                    if (chatView.ChatOptions is {} options)
                    {
                        foreach (var model in options.Models)
                            if (model.Selected) lines.Add($"Model: {model.Name}");
                            else Act(model.Name, () => new() { ChatSetOptions = new() { Model = model.Name } });
                        foreach (var effort in options.Efforts)
                            if (!effort.Selected) Act(effort.Name, () => new() { ChatSetOptions = new() { Effort = effort.Name } });
                        if (options.Effort.Length > 0) lines.Add($"Effort: {options.Effort}");
                        lines.Add(options.Note);
                    }
                }
                lines.Add(chatView.ChatOptionsError);
                if (chatView.HasNewer)
                    Act("Newer messages", () => new() { AgentLoadEarlier = new() { Newer = true } });
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
                DeviceFilter(t.DeviceId, id => Send(new() { OpenTerminals = new() { DeviceId = id } }));
                if (t.DeviceId.Length > 0 && t.DeviceId != "*")
                    Act("Device details", () => new() { SelectDevice = new() { DeviceId = t.DeviceId } });
                if (t.Sessions.Count == 0)
                    lines.Add(t.Loading ? "Loading terminals…" : "No terminals");
                foreach (var s in t.Sessions)
                    Row($"{(s.Title.Length == 0 ? s.Id : s.Title)}    {s.DeviceName} · {s.Source} · {s.Cols}×{s.Rows}",
                        new() { OpenTerminal = new() { Session = s.Id, DeviceId = s.DeviceId } });
                if (t.CanCreate)
                    foreach (var src in t.Sources)
                    {
                        Act("New " + src.Label, () => new() { TerminalCreate = new() { Source = src.Source } });
                        actions.Controls[actionIndex - 1].Enabled = !v.Busy;
                    }
                lines.AddRange(t.Errors);
            }
            break;
        case Page.Terminal:
            if (v.Terminal is {} term)
            {
                title.Text = term.Session.Length == 0 ? "Terminal" : $"{term.Source} · {term.Session}";
                if (status.Text.Length == 0)
                    status.Text = term.Ended && term.Status.Length > 0 ? "Ended · " + term.Status
                                  : term.Ended ? "Ended" : term.Status;
                if (term.CanPaste && !term.Ended)
                    AddAction("Paste (Ctrl+Shift+V)", () => { terminal.PasteClipboard(); terminal.Focus(); });
                Act("Close terminal", () => new() { CloseTerminal = new() });
            }
            break;
        case Page.Files:
            if (v.Files is {} files)
            {
                title.Text = files.Path.Length == 0 ? "Files" : files.Path;
                if (files.Path.Length != 0)
                    Row("↑  Parent directory", WorktreeNavigation.Parent(files.Path));
                AddAction("Open file link…", () =>
                {
                    string link = Microsoft.VisualBasic.Interaction.InputBox("File link in this session", "Open file link");
                    if (!string.IsNullOrWhiteSpace(link)) Send(new() { OpenFileLink = new() { Url = link.Trim() } });
                });
                if (files.Loading) lines.Add("Loading directory…");
                foreach (var entry in files.Entries)
                {
                    string label = entry.Dir ? entry.Name + "/" : $"{entry.Name}    {entry.Size:N0} B";
                    Row(label, WorktreeNavigation.Entry(files.Path, entry));
                }
            }
            break;
        case Page.File:
            if (v.File is {} file)
            {
                title.Text = file.Path;
                preview.ShowFile(file);
                if (file.IsDiff && file.CurrentPath.Length > 0)
                    Act("Current file", () => new() { OpenFile = new() { Path = file.CurrentPath } });
            }
            break;
        case Page.Git:
            if (v.Git is {} git)
            {
                if (git.DiffCommit.Length != 0)
                {
                    title.Text = "Git diff";
                    preview.ShowCommit(git);
                    Act("Git log", () => new() { OpenGit = new() });
                }
                else
                {
                    title.Text = "Git log";
                    if (git.Loading) lines.Add("Loading commits…");
                    foreach (var commit in git.Commits)
                        Row($"{commit.Subject}    {commit.Hash[..Math.Min(8, commit.Hash.Length)]} · {commit.Author} {WorktreeNavigation.Time(commit.TsUnix)}",
                            new() { OpenGit = new() { DiffCommit = commit.Hash } });
                }
            }
            break;
        case Page.Browser:
            if (v.Browser is {} browser)
            {
                input.PlaceholderText = "Browser operation (JSON)";
                if (browser.Status is {} bs)
                {
                    lines.Add(bs.Message);
                    if (browser.Local && bs.ExtensionPath.Length > 0)
                        lines.Add(bs.ExtensionPath);
                }
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
        UpdateComposerButtons();
        while (actions.Controls.Count > actionIndex)
            actions.Controls[actions.Controls.Count - 1].Dispose();
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
        bool showChatSidebar = page == Page.DesktopChats || (page == Page.Chat && v.Workspace == Page.DesktopChats);
        chatWelcome.Visible = page == Page.DesktopChats && v.DesktopChats is { DeviceId.Length: > 0 };
        chatSidebar.Visible = showChatSidebar;
        chatSidebar.Display(showChatSidebar ? v.DesktopChats : null, chatKey?.Device ?? "", chatKey?.Session ?? "");
        bool showingSource = page == Page.File && v.File != null || page == Page.Git && v.Git is { DiffCommit.Length: > 0 };
        preview.Visible = showingSource;
        bool showingTerminal = page == Page.Terminal && v.Terminal != null;
        conversationTerminal.Visible = inWorkspaceTerminal;
        conversationTerminal.Display(inWorkspaceTerminal ? v.Chat : null, inWorkspaceTerminal ? v.Terminal : null);
        output.Visible = page != Page.DesktopChats && !showingSource && !showingTerminal && !inWorkspaceTerminal;
        terminal.Visible = showingTerminal;
        list.Visible = rows.Count > 0;
        terminal.Display(showingTerminal ? v.Terminal : null);
        var text = string.Join("\r\n", lines.Where(x => !string.IsNullOrEmpty(x)));
        if (output.Text != text)
        {
            int pos = output.SelectionStart;
            output.Text = text;
            output.Select(Math.Min(pos, text.Length), 0);
        }
    }
}

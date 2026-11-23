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
    readonly FlowLayoutPanel utilities = Bar();
    readonly Button back, settings, agentsTab, chatsTab, terminalsTab;
    readonly TableLayoutPanel toolbar = new() { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 2, RowCount = 1,
        Padding = new Padding(8, 2, 8, 2) };
    readonly Label title = new() { Dock = DockStyle.Top, AutoEllipsis = true, TextAlign = ContentAlignment.MiddleLeft,
                                   Font = new Font("Segoe UI", 17) };
    readonly Label status = new() { Dock = DockStyle.Bottom, Height = 30, AutoEllipsis = true };
    readonly ListBox list = new() { Dock = DockStyle.Left, Width = 310, IntegralHeight = false, HorizontalScrollbar = true };
    readonly TextBox output = new() { Dock = DockStyle.Fill, Multiline = true, ScrollBars = ScrollBars.Both,
                                      ReadOnly = true, WordWrap = false };
    readonly WorkspaceWelcome chatWelcome = new() { Visible = false };
    readonly DesktopChatSidebar chatSidebar = new() { Dock = DockStyle.Left, Visible = false };
    readonly AgentSidebar agentSidebar = new() { Dock = DockStyle.Left, Visible = false };
    List<BlockOffset> plainOffsets = [];
    (string Device, string Session)? readingScope;
    Form? desktopViewer;
    string? viewerDevice;
    bool viewerLeaseSeen;
    string? priorViewerToast;
    readonly JumpToLatest jump = new();
    readonly TranscriptSteps transcript = new();
    readonly HistoryTranscript history = new() { Dock = DockStyle.Fill, Visible = false };
    readonly ConversationTerminal conversationTerminal = new() { Dock = DockStyle.Fill, Visible = false };
    readonly SessionTrace trace = new() { Dock = DockStyle.Fill, Visible = false };
    readonly RelatedSessions related = new() { Dock = DockStyle.Fill, Visible = false };
    readonly SessionTabs sessionTabs = new() { Visible = false };
    (string Device, string Session)? tabScope;
    SessionTab selectedTab;
    RunHistoryDialog? runHistory;
    ChatOptionsDialog? chatOptions;
    readonly BrowserPanel browserPanel = new() { Dock = DockStyle.Fill, Visible = false };
    readonly InvitePanel invitePanel;
    readonly TerminalGrid terminal = new() { Dock = DockStyle.Fill, Visible = false };
    readonly SourcePreview preview = new() { Dock = DockStyle.Fill, Visible = false };
    readonly QueuedMessages queue = new();
    readonly LockedUseStatus lockedUse = new();
    readonly ComposerSurface composerSurface = new();
    readonly Button sendDraft = new() { Text = "Send", AccessibleName = "Send message", Dock = DockStyle.Right, Width = 78 };
    readonly ChatInput input =
        new() { Dock = DockStyle.Fill, BorderStyle = BorderStyle.None, Multiline = true, AcceptsReturn = true, ScrollBars = ScrollBars.Vertical };
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
    bool opening, ready, appActive, focusNetworkList;
    string inputContext = "";
    static FlowLayoutPanel Bar() => new() { Dock = DockStyle.Top, AutoSize = true, WrapContents = true,
                                            Padding = new Padding(0, 2, 0, 2) };
    static void ClearBar(FlowLayoutPanel bar)
    {
        while (bar.Controls.Count > 0)
            bar.Controls[0].Dispose();
    }
    static Button Button(FlowLayoutPanel bar, string text, Action action)
    {
        var b = new Button { Text = text, AutoSize = true, Height = 29, TabStop = true };
        WorkspaceStyle.StyleButton(b);
        b.Tag = action;
        b.Click += (_, _) => ((Action)b.Tag!)();
        bar.Controls.Add(b);
        return b;
    }
    readonly bool deviceAuthAvailable;
    public Desktop(bool attachOnly) : this(attachOnly, null) { }
    internal Desktop(bool attachOnly, string? fixtureSocket)
    {
        deviceAuthAvailable = fixtureSocket == null;
        Text = !deviceAuthAvailable ? "Muster (isolated development)" : attachOnly ? "Muster (development)" : "Muster";
        MinimumSize = new Size(740, 480);
        Size = new Size(1050, 700);
        BackColor = WorkspaceStyle.Canvas;
        title.ForeColor = WorkspaceStyle.Ink;
        status.ForeColor = WorkspaceStyle.Muted;
        status.Visible = false;
        status.TextChanged += (_, _) => status.Visible = status.Text.Length > 0;
        output.BorderStyle = BorderStyle.None;
        output.BackColor = WorkspaceStyle.Canvas;
        var workspace = new Panel { Name = "Workspace", Dock = DockStyle.Fill };
        var detail = new Panel { Name = "WorkspaceDetail", Dock = DockStyle.Fill, Padding = new Padding(14, 8, 14, 10) };
        void SizeTitle() => title.Height = title.Font.Height + LogicalToDeviceUnits(8);
        title.FontChanged += (_, _) => SizeTitle();
        title.DpiChangedAfterParent += (_, _) => SizeTitle();
        SizeTitle();
        var body = new Panel { Dock = DockStyle.Fill };
        invitePanel = new InvitePanel(Clipboard.SetText, () => Send(new() { ShowInvite = new() }));
        body.Controls.Add(browserPanel);
        body.Controls.Add(invitePanel);
        body.Controls.Add(related);
        body.Controls.Add(trace);
        body.Controls.Add(conversationTerminal);
        terminal.ClipboardError += message => status.Text = message;
        body.Controls.Add(terminal);
        body.Controls.Add(preview);
        body.Controls.Add(output);
        body.Controls.Add(history);
        history.ActivateLink += destination =>
        {
            if (!history.Visible || readingScope == null || current?.Screen != Page.Chat) return;
            if (Uri.TryCreate(destination, UriKind.Absolute, out var uri) && uri.Scheme is "http" or "https" or "mailto")
            {
                if (MessageBox.Show(this, uri.AbsoluteUri, "Open link", MessageBoxButtons.OKCancel) != DialogResult.OK) return;
                try { Process.Start(new ProcessStartInfo(uri.AbsoluteUri) { UseShellExecute = true }); }
                catch (System.ComponentModel.Win32Exception ex) { MessageBox.Show(this, ex.Message, "Open link"); }
            }
            else Send(new() { OpenFileLink = new() { Url = destination } });
        };
        body.Controls.Add(chatWelcome);
        body.Controls.Add(list);
        chatWelcome.NewChat += () => Send(new() { DesktopChatOpen = new() { Chat = "" } });
        workspace.Controls.Add(detail);
        workspace.Controls.Add(chatSidebar);
        workspace.Controls.Add(agentSidebar);
        workspace.Resize += (_, _) =>
        {
            int width = workspace.ClientSize.Width;
            agentSidebar.Width = chatSidebar.Width = Math.Min(width / 2, Math.Max(LogicalToDeviceUnits(240), width / 4));
        };
        body.Resize += (_, _) =>
        {
            int third = body.ClientSize.Width / 3;
            list.Width = Math.Min(LogicalToDeviceUnits(310), third);
        };
        body.Controls.Add(jump);
        jump.BringToFront();
        void PlaceJump() => jump.Location = new Point(body.ClientSize.Width - jump.Width - LogicalToDeviceUnits(24),
            body.ClientSize.Height - jump.Height - LogicalToDeviceUnits(8));
        body.Resize += (_, _) => PlaceJump();
        jump.SizeChanged += (_, _) => PlaceJump();
        jump.Intent += Send;
        actions.Dock = DockStyle.Bottom;
        detail.Controls.Add(body);
        detail.Controls.Add(lockedUse);
        detail.Controls.Add(queue);
        WorkspaceStyle.StyleButton(sendDraft);
        void SizeSendButton() => sendDraft.Width = sendDraft.GetPreferredSize(Size.Empty).Width + sendDraft.LogicalToDeviceUnits(10);
        sendDraft.HandleCreated += (_, _) => SizeSendButton();
        sendDraft.DpiChangedAfterParent += (_, _) => SizeSendButton();
        sendDraft.Click += (_, _) => Submit();
        composerSurface.Controls.Add(input);
        composerSurface.Controls.Add(sendDraft);
        detail.Controls.Add(composerSurface);
        detail.Controls.Add(actions);
        detail.Controls.Add(status);
        detail.Controls.Add(sessionTabs);
        detail.Controls.Add(title);
        Controls.Add(workspace);
        utilities.WrapContents = false;
        utilities.AutoSizeMode = AutoSizeMode.GrowAndShrink;
        utilities.Anchor = AnchorStyles.Right;
        toolbar.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        toolbar.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        toolbar.Controls.Add(nav, 0, 0);
        toolbar.Controls.Add(utilities, 1, 0);
        toolbar.BackColor = nav.BackColor = utilities.BackColor = WorkspaceStyle.Sidebar;
        Controls.Add(toolbar);
        core = new(v => Interlocked.Exchange(ref pending, v), (id, e) => effects.Enqueue((id, e)), attachOnly, fixtureSocket);
        browserPanel.Intent += Send;
        chatSidebar.Intent += Send;
        agentSidebar.Intent += Send;
        queue.Intent += Send;
        lockedUse.Intent += Send;
        terminal.Intent += core.Send;
        conversationTerminal.Intent += core.Send;
        sessionTabs.Chosen += SelectTab;
        related.Open += Send;
        conversationTerminal.ReturnToChat += () => SelectTab(SessionTab.Conversation);
        back = Button(nav, "Back", () =>
        {
            if (current is { CanGoBack: true } view)
                Send(new() { Navigate = new() { Back = true, BackCount = 1, ExpectedPath = { view.NavigationPath } } });
        });
        back.Visible = false;
        void Network()
        {
            if (current?.Screen == Page.Settings)
                FocusNetworkList();
            else
            {
                focusNetworkList = true;
                Go(Page.Settings);
            }
        }
        agentsTab = Button(nav, "Agents", () => Send(new() { OpenAgents = new() { DeviceId = "*" } }));
        chatsTab = Button(nav, "Chats", () => Send(new() { OpenDesktopChats = new() }));
        terminalsTab = Button(nav, "Terminals", () =>
        {
            string device = current?.Screen switch
            {
                Page.Agents => current.Agents?.DeviceId ?? "",
                Page.Device when current.SelectedDeviceHasServices => current.SelectedDevice,
                _ => ""
            };
            Send(new() { OpenTerminals = new() { DeviceId = device } });
        });
        var tools = new Button { Text = "Tools ▾", AccessibleName = "Tools", AutoSize = true };
        WorkspaceStyle.StyleButton(tools);
        utilities.Controls.Add(tools);
        tools.Click += (_, _) =>
        {
            var menu = new ContextMenuStrip();
            menu.Items.Add("Network", null, (_, _) => Network());
            menu.Items.Add("Browser", null, (_, _) => Send(new() { OpenBrowser = new() {
                Device = current?.SelectedDeviceHasServices == true ? current.SelectedDevice : "" } }));
            if (deviceAuthAvailable)
                menu.Items.Add("Presence key", null, (_, _) => ShowPresenceReport(() => Presence.ReadReport(core.Socket)));
            menu.ShowBelow(tools);
        };
        settings = Button(utilities, "Settings", () => Go(Page.Settings));
        var tips = new ToolTip();
        tips.SetToolTip(back, "Back (Alt+Left)");
        tips.SetToolTip(settings, "Settings (Ctrl+,)");
        Disposed += (_, _) => tips.Dispose();
        Button(utilities, "Refresh", () => Send(new() { Refresh = new() }));
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
        list.MouseClick += (_, e) =>
        {
            int index = list.IndexFromPoint(e.Location);
            if (e.Button == MouseButtons.Left && index >= 0 && list.Items[index] is Row row && row.Action.SelectDevice != null)
                Send(row.Action);
        };
        list.DoubleClick += (_, _) =>
        {
            if (list.SelectedItem is Row row && row.Action.SelectDevice == null)
                OpenRow();
        };
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
            jump.UpdateAvailability();
            while (effects.TryDequeue(out var item))
                EffectReceived(item.Item1, item.Item2);
        };
        Shown += async (_, _) => await Connect();
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
    public void InstallPowerMenu()
    {
        var menu = new PowerMenu(PowerMenu.PreferenceKey) { Dock = DockStyle.None, GripStyle = ToolStripGripStyle.Hidden,
            BackColor = WorkspaceStyle.Sidebar, Margin = new Padding(2), Anchor = AnchorStyles.Left };
        MainMenuStrip = menu;
        utilities.Controls.Add(menu);
        utilities.Controls.SetChildIndex(menu, 0);
    }

    protected override bool ProcessCmdKey(ref Message message, Keys keyData)
    {
        if (ready && current != null && Control.FromChildHandle(message.HWnd) is not TerminalGrid)
        {
            if (keyData == (Keys.Control | Keys.Oemcomma))
            {
                settings.PerformClick();
                return true;
            }
            if (keyData == (Keys.Alt | Keys.Left) && back.Visible)
            {
                back.PerformClick();
                return true;
            }
        }
        return base.ProcessCmdKey(ref message, keyData);
    }

    protected override void WndProc(ref Message message)
    {
        const int WM_ACTIVATEAPP = 0x001C;
        if (message.Msg == WM_ACTIVATEAPP)
        {
            appActive = message.WParam != IntPtr.Zero;
            if (ready) core.Lifecycle(appActive ? 0u : 1u);
        }
        base.WndProc(ref message);
    }

    async Task Connect(bool start = false)
    {
        if (opening)
            return;
        opening = true;
        toolbar.Enabled = false;
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
            toolbar.Enabled = true;
            timer.Start();
            core.Lifecycle(appActive ? 0u : 1u);
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
    void SelectTab(SessionTab tab)
    {
        if (current?.Chat?.Session == null || tabScope == null || selectedTab == tab) return;
        if (tab == SessionTab.Terminal && current.Chat.Terminal?.Available != true) return;
        var previous = selectedTab;
        selectedTab = tab;
        if (tab == SessionTab.Terminal || previous == SessionTab.Terminal)
            Send(new() { SessionTerminalAction = new() { Action = tab == SessionTab.Terminal
                ? SessionTerminalAction.Types.Action.Start : SessionTerminalAction.Types.Action.Detach } });
        Render(current);
    }
    void FocusNetworkList()
    {
        if (!list.Visible || list.Items.Count == 0) return;
        if (list.SelectedIndex < 0) list.SelectedIndex = 0;
        list.Focus();
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

    void ConfirmEndSideChat(ChatView chat)
    {
        var request = input.WithDraft(new() { SideChatAction = new() { End = true } });
        var key = request.ComposerAction?.Draft?.Key;
        if (string.IsNullOrEmpty(key) || key != chat.Draft?.Key) return;
        var answer = MessageBox.Show(this, "End this side chat? Its history will remain available to read.",
            "End side chat", MessageBoxButtons.OKCancel, MessageBoxIcon.Question, MessageBoxDefaultButton.Button2);
        if (answer == DialogResult.OK && current?.Screen == Page.Chat &&
            current.Chat is { CanEndSideChat: true } active && active.DeviceId == chat.DeviceId &&
            active.Session?.Id == chat.Session?.Id && active.Draft?.Key == key)
            core.Send(request);
    }

    void ViewDesktop(string device, string name)
    {
        if (desktopViewer != null) { desktopViewer.Activate(); return; }
        var form = new Form { Text = $"Desktop · {name}", StartPosition = FormStartPosition.CenterParent,
            ClientSize = new Size(960, 540), MinimumSize = new Size(480, 300) };
        var panel = new Panel { Dock = DockStyle.Fill, BackColor = Color.Black, AccessibleName = "Remote desktop" };
        form.Controls.Add(panel);
        desktopViewer = form;
        viewerDevice = device;
        viewerLeaseSeen = false;
        priorViewerToast = current?.Toasts.LastOrDefault()?.Id;
        form.FormClosed += (_, _) => {
            desktopViewer = null;
            viewerDevice = null;
            if (!IsDisposed) Send(new() { EndDesktopSession = device });
        };
        form.Show(this);
        Send(new() { DesktopSession = new() { Device = device, ViewInWindow = (ulong)panel.Handle.ToInt64() } });
    }

    void Go(Page page) => Send(new() { Navigate = new() { Screen = page } });
    void CopySource(string text, string kind)
    {
        if (text.Contains('\0'))
        {
            MessageBox.Show(this, "This text contains a null character and cannot be copied as clipboard text.", "Copy " + kind);
            return;
        }
        try { Clipboard.SetText(text); status.Text = kind == "diff" ? "Diff copied" : "Code copied"; }
        catch (System.Runtime.InteropServices.ExternalException ex)
        {
            MessageBox.Show(this, ex.Message, "Copy " + kind, MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
    }

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
        else if (e.ScanQr != null)
        {
            using var dialog = new PasteInviteDialog();
            result.Ok = dialog.ShowDialog(this) == DialogResult.OK;
            if (result.Ok) result.Text = dialog.Invitation;
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

    internal void Render(ViewModel v)
    {
        var previousScope = readingScope;
        TextBoxBase previousReader = history.Visible ? history : output;
        var previousViewport = previousScope != null && previousReader.Visible
            ? new TranscriptViewport(previousReader, history.Visible ? history.BlockOffsets : plainOffsets) : null;
        current = v;
        back.Visible = v.CanGoBack;
        var page = v.Screen;
        if (desktopViewer is {} viewer)
        {
            bool selected = page == Page.Device && v.SelectedDevice == viewerDevice;
            if (selected && v.DeviceActions?.DesktopSessionOpen == true)
            {
                viewerLeaseSeen = true;
                viewer.Text = v.DeviceActions.DesktopSessionText;
            }
            else if (!selected || viewerLeaseSeen || (v.Toasts.LastOrDefault() is { Error: true } toast && toast.Id != priorViewerToast)) viewer.Close();
        }
        foreach (var (button, route) in new[] { (agentsTab, Page.Agents), (chatsTab, Page.DesktopChats), (terminalsTab, Page.Terminals) })
        {
            bool selected = v.Workspace == route;
            button.BackColor = selected ? WorkspaceStyle.Selected : WorkspaceStyle.Canvas;
            button.ForeColor = selected ? WorkspaceStyle.SelectedText : WorkspaceStyle.Ink;
            button.AccessibleDescription = selected ? "Current workspace" : null;
        }
        title.Visible = page is not (Page.Agents or Page.DesktopChats);
        output.WordWrap = page == Page.Chat;
        runHistory?.Display(page == Page.Chat ? v.Chat : null);
        chatOptions?.Display(page == Page.Chat ? v.Chat : null);
        title.Text=page is Page.Home or Page.Onboarding ? v.Network?.Name is {Length:>0} n ? n : "Muster network" : page.ToString();
        status.Text = v.Toasts.LastOrDefault()?.Text ?? v.Join?.Message ?? "";
        var rows = new List<Row>();
        var lines = new List<string>();
        var images = new List<TranscriptImage>();
        var blocks = new List<TranscriptBlock>();
        var links = new List<TranscriptLink>();
        var speakers = new HashSet<int>();
        var styles = new List<TranscriptStyle>();
        void Row(string name, Intent intent) => rows.Add(new(name, intent));
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
                menu.ShowBelow(anchor);
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
        if (tabScope != chatKey)
        {
            tabScope = chatKey;
            selectedTab = SessionTab.Conversation;
        }
        var conversation = page == Page.Chat && selectedTab == SessionTab.Conversation ? v.Chat : null;
        output.AccessibleName = conversation != null ? "Conversation history" : $"{page} details";
        queue.Display(conversation);
        lockedUse.Display(conversation);
        bool inRelated = selectedTab is SessionTab.Subagents or SessionTab.Sessions;
        bool inTrace = selectedTab == SessionTab.Trace;
        bool inWorkspaceTerminal = selectedTab == SessionTab.Terminal;
        sessionTabs.Visible = chatKey != null;
        if (chatKey != null) sessionTabs.Display(v.Chat!, selectedTab);
        sendDraft.Visible = false;
        composerSurface.Visible = selectedTab == SessionTab.Conversation && (page is Page.Onboarding or Page.Home or Page.Chat);
        input.PlaceholderText = "";
        switch (page)
        {
        case Page.Device:
        {
            var selected = v.Network?.Members.FirstOrDefault(member => member.DeviceId == v.SelectedDevice);
            if (selected == null)
            {
                lines.Add("No such device");
                break;
            }
            title.Text = selected.Name;
            lines.Add($"Name    {Reported(selected.Name)}");
            lines.Add($"Kind    {Reported(selected.Kind)}");
            lines.Add($"Role    {Reported(selected.Role)}");
            lines.Add($"Virtual IP    {Reported(selected.Vip)}");
            lines.Add($"Fingerprint    {Reported(selected.Fingerprint)}");
            if (selected.Link.Length > 0) lines.Add($"Link    {selected.Link}");
            if (selected.ConnectionStatus.Length > 0) lines.Add($"Connection    {selected.ConnectionStatus}");
            lines.Add($"Device version    {Reported(selected.Version)}");
            lines.Add($"Device commit    {Reported(selected.Commit)}");
            if (v.SelectedDeviceHasServices)
            {
                Act("Chrome on device", () => new() { OpenBrowser = new() { Device = selected.DeviceId } });
                Act("Agents on device", () => new() { OpenAgents = new() { DeviceId = selected.DeviceId } });
                Act("Terminals on device", () => new() { OpenTerminals = new() { DeviceId = selected.DeviceId } });
            }
            if (v.DeviceActions is {} deviceActions)
            {
                if (deviceActions.CanViewDesktop)
                    AddAction("View desktop", () => ViewDesktop(selected.DeviceId, selected.Name));
                if (deviceActions.DesktopSessionOpen)
                {
                    lines.Add(deviceActions.DesktopSessionText);
                    AddAction("Close desktop", () => desktopViewer?.Close());
                }
                if (deviceActions.ShowAccess && v.DeviceAccess is {} access)
                {
                    lines.Add("Access to other devices");
                    foreach (var section in access.Sections)
                        lines.Add($"{section.Title}    {section.Summary}");
                    if (deviceActions.CanEditAccess)
                        AddAction("Edit access", () =>
                        {
                            using var dialog = new AccessDialog("Save access", selected.Name + "\r\n" + selected.Fingerprint, access);
                            if (dialog.ShowDialog(this) == DialogResult.OK)
                                Send(new() { SetScopes = new() { DeviceId = selected.DeviceId, Role = selected.Role,
                                                                 Scopes = { dialog.Scopes } } });
                        });
                }
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
            break;
        }
        case Page.Invite:
            title.Text = "Invite a device";
            break;
        case Page.Join:
            title.Text = "Joining network";
            lines.Add(v.Join?.Phase switch {
                "failed" => "Could not join network", "waiting_approval" => "Waiting for approval",
                "joined" => "Joined", "connecting" => "Connecting…", _ => "Joining network"
            });
            lines.Add(v.Join?.Message ?? "");
            if (v.Join?.Phase == "failed")
                Act("Try again", () => new() { JoinNetwork = new() });
            break;
        case Page.Home:
        case Page.Onboarding:
        case Page.Settings:
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
            if (page is Page.Settings or Page.Onboarding)
            {
                var self = v.Self;
                var state = self?.State;
                lines.Add($"Local relay    {(self == null ? "not reported" : self.Relay.Length == 0 ? "none configured" : self.Relay)}");
                lines.Add($"Relay registration    {Reported(state?.Relay)}");
                if (page == Page.Settings)
                {
                    lines.Add($"Assigned virtual IP    {Reported(state?.Vip)}");
                    lines.Add($"Local vnet    {state?.VnetUp switch { true => "up", false => "down", null => "not reported" }}");
                    if (state?.CanRetryVnet == true)
                        Act("Retry local vnet", () => new() { DesktopSetupAction = new() { Action = DesktopSetupAction.Types.Action.RetryVnet } });
                }
                if (self != null)
                    AddAction("Change local relay", () =>
                    {
                        using var editor = new RelayDialog(self.Relay);
                        if (editor.ShowDialog(this) == DialogResult.OK)
                            Send(new() { SetRelay = new() { Relay = editor.Endpoint } });
                    });
            }
            if (v.Network != null)
            {
                lines.Add(v.Network.VnetCidr);
                foreach (var m in v.Network.Members)
                {
                    var parts = new[] { m.Name, m.IsSelf ? "this device" : "", m.Role, m.Vip, m.Link };
                    Row(string.Join(" · ", parts.Where(part => part.Length > 0)),
                        new() { SelectDevice = new() { DeviceId = m.DeviceId } });
                }
            }
            if (v.Network?.HasNetwork != true)
            {
                input.PlaceholderText = "Network name or invitation";
                Act("Create network", () => new() { CreateNetwork = new() { Name = input.Text.Trim() } });
                Act("Join with invite", () => new() { JoinNetwork = new() { InviteText = input.Text.Trim(),
                                                                            DeviceName = v.Self?.Name ?? "",
                                                                            DeviceKind = v.Self?.Kind ?? "" } });
            }
            if (v.CanInvite)
            {
                Act("Create invite", () => new() { ShowInvite = new() });
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
            if (v.SelectedDevice.Length > 0 && v.SelectedDeviceHasServices)
            {
                Act("Agents on device", () => new() { OpenAgents = new() { DeviceId = v.SelectedDevice } });
                Act("Terminals on device", () => new() { OpenTerminals = new() { DeviceId = v.SelectedDevice } });
            }
            if (page == Page.Settings && v.Policy is {} policy)
            {
                if (v.Network is {} network && network.AccessMode.Length > 0)
                {
                    lines.Add("Network access mode: " + network.AccessMode);
                    if (v.CanInvite)
                    {
                        var next = network.AccessMode == "basic" ? "advanced" : "basic";
                        Act("Use " + next + " network access",
                            () => new() { SetNetworkAccessMode = new() { Mode = next } });
                    }
                    void EditService(string name, string wholeScope, string placeholder,
                                     Func<Muster.V1.ServiceGrants, Muster.V1.GrantSet?> read,
                                     Action<Muster.V1.ServiceGrants, Muster.V1.GrantSet?> write)
                    {
                        var original = read(policy.ServiceGrants ?? new());
                        string summary = "Existing policy";
                        if (original?.AllDevices == true) summary = "All paired devices";
                        else if (original != null)
                        {
                            var assignments = original.Rules
                                .Where(rule => placeholder.Length > 0 || rule.Scope == wholeScope)
                                .Select(rule => (
                                    Selector: rule.Scope == wholeScope ? "All" : rule.Scope,
                                    Devices: network.Members.Where(member => rule.DeviceIds.Contains(member.DeviceId))
                                        .Select(member => member.Name).ToArray()))
                                .Where(rule => rule.Devices.Length > 0)
                                .Select(rule => rule.Selector + " → " + string.Join(", ", rule.Devices));
                            summary = string.Join("; ", assignments);
                            if (summary.Length == 0) summary = "No assigned devices";
                        }
                        lines.Add(name + " access rules: " + summary);
                        AddAction(name + " access rules", () =>
                        {
                            using var dialog = new ServiceGrantsDialog(name, wholeScope, placeholder,
                                network.Members.Where(member => !member.IsSelf), original);
                            if (dialog.ShowDialog(this) != DialogResult.OK || current?.Screen != Page.Settings ||
                                current.Policy is not {} latest) return;
                            if (!Equals(original, read(latest.ServiceGrants ?? new())))
                            {
                                MessageBox.Show(this, "Rules changed. Reopen the editor to review the current rules.", name);
                                return;
                            }
                            var updated = latest.Clone();
                            updated.ServiceGrants ??= new();
                            write(updated.ServiceGrants, dialog.Grants);
                            Send(new() { SetPolicy = new() { Policy = updated } });
                        });
                    }
                    EditService("Agents", "agents", "agents or agents:codex", rules => rules.Agents,
                        (rules, value) => rules.Agents = value);
                    EditService("Terminals", "terminals", "terminals, terminal.view or terminal.control",
                        rules => rules.Terminals, (rules, value) => rules.Terminals = value);
                    EditService("Network", "net:*", network.AccessMode == "advanced" ? "net:* or net:tcp/443" : "",
                        rules => rules.Network, (rules, value) => rules.Network = value);
                    lines.Add("Network offer: " + (policy.HasOfferNetwork
                        ? (policy.OfferNetwork ? "On" : "Off") : "Existing port policy"));
                    Intent NetworkOffer(bool enabled)
                    {
                        var updated = policy.Clone();
                        updated.OfferNetwork = enabled;
                        return new() { SetPolicy = new() { Policy = updated } };
                    }
                    if (!policy.HasOfferNetwork || !policy.OfferNetwork)
                        Act("Enable network service", () => NetworkOffer(true));
                    if (!policy.HasOfferNetwork || policy.OfferNetwork)
                        Act("Disable network service", () => NetworkOffer(false));
                }
                lines.Add("Agent access: " + (policy.OfferAgent ? "On" : "Off") +
                          "\r\nTerminal access: " + (policy.OfferTerminal ? "On" : "Off"));
                if (v.Network?.AccessMode != "basic")
                {
                    lines.Add("Open ports    " + (policy.OpenPorts.Count == 0 ? "None" : string.Join(", ", policy.OpenPorts)));
                    AddAction("Edit open ports", () =>
                    {
                        using var dialog = new OpenPortsDialog(policy.OpenPorts);
                        if (dialog.ShowDialog(this) != DialogResult.OK || current?.Screen != Page.Settings ||
                            current.Policy is not {} latest) return;
                        if (!policy.OpenPorts.SequenceEqual(latest.OpenPorts))
                        {
                            MessageBox.Show(this, "Open ports changed. Reopen the editor to review the current rules.", "Open ports");
                            return;
                        }
                        var updated = latest.Clone();
                        updated.OpenPorts.Clear();
                        updated.OpenPorts.Add(dialog.Rules);
                        Send(new() { SetPolicy = new() { Policy = updated } });
                    });
                }
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
        case Page.DesktopChats:
            title.Text = "Chats";
            break;
        case Page.Chat:
            if (v.Chat is {} chatView)
            {
                title.Text = chatView.Session?.Title ?? "Chat";
                if (inWorkspaceTerminal || inRelated) break;
                if (inTrace)
                {
                    if (chatView.HasEarlier)
                        Act("Earlier messages", () => new() { AgentLoadEarlier = new() });
                    break;
                }
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
                transcript.AppendTo(lines, images, blocks, links, speakers, styles);
                if (transcript.HasCode)
                {
                    int copyIndex = actionIndex;
                    AddAction("Copy code", () =>
                    {
                        var menu = transcript.CopyCode(text => CopySource(text, "code"));
                        if (menu == null) return;
                        var anchor = actions.Controls[copyIndex];
                        menu.ShowBelow(anchor);
                    });
                }
                if (transcript.HasDiffs)
                {
                    int menuIndex = actionIndex;
                    AddAction("File diffs…", () =>
                    {
                        var menu = transcript.DiffMenu(text => CopySource(text, "diff"),
                            () => { if (current != null) Render(current); });
                        menu.ShowBelow(actions.Controls[menuIndex]);
                    });
                }
                if (transcript.HasStepsMenu)
                {
                    int menuIndex = actionIndex;
                    AddAction("Work steps", () =>
                    {
                        var menu = transcript.Menu(core.Send, () => { if (current != null) Render(current); });
                        var anchor = actions.Controls[menuIndex];
                        menu.ShowBelow(anchor);
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
                        var anchor = actions.Controls[menuIndex];
                        menu.ShowBelow(anchor);
                    });
                }
                lines.Add(chatView.LoadError);
                if (chatView.ResponseError != chatView.Queue?.Error) lines.Add(chatView.ResponseError);
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
                {
                    sendDraft.Visible = true;
                    composerButtons.Add((sendDraft, true));
                }
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
                    AddAction("End side chat", () => ConfirmEndSideChat(chatView));
                if (chatView.CanChooseModel && chatView.Session != null)
                    AddAction("Model and effort", () =>
                    {
                        using var dialog = new ChatOptionsDialog(chatView, Send);
                        chatOptions = dialog;
                        try { dialog.ShowDialog(this); }
                        finally { chatOptions = null; }
                    });
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
            title.Text = "Agent Halo Browser";
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
            var memberId = old?.Action.SelectDevice?.DeviceId ?? v.SelectedDevice;
            var memberRow = rows.Find(row => row.Action.SelectDevice?.DeviceId == memberId);
            if (memberRow != null || old != null)
                list.SelectedItem = memberRow ?? old;
            list.EndUpdate();
        }
        bool showChatSidebar = page == Page.DesktopChats || (page == Page.Chat && v.Workspace == Page.DesktopChats);
        bool showAgents = page == Page.Agents || (v.Workspace == Page.Agents &&
            page is Page.Chat or Page.Files or Page.File or Page.Git);
        agentSidebar.Visible = showAgents;
        agentSidebar.Display(showAgents ? v.Agents : null, v.ServiceDevices, chatKey);
        chatWelcome.Visible = page is Page.Agents or Page.DesktopChats;
        if (chatWelcome.Visible) chatWelcome.Display(page == Page.DesktopChats, v.DesktopChats);
        chatSidebar.Visible = showChatSidebar;
        chatSidebar.Display(showChatSidebar ? v.DesktopChats : null, chatKey?.Device ?? "", chatKey?.Session ?? "");
        bool showingSource = page == Page.File && v.File != null || page == Page.Git && v.Git is { DiffCommit.Length: > 0 };
        preview.Visible = showingSource;
        bool showingTerminal = page == Page.Terminal && v.Terminal != null;
        conversationTerminal.Visible = inWorkspaceTerminal;
        conversationTerminal.Display(inWorkspaceTerminal ? v.Chat : null, inWorkspaceTerminal ? v.Terminal : null);
        readingScope = selectedTab == SessionTab.Conversation ? chatKey : null;
        bool latestPage = v.Chat?.HasNewer == false;
        bool rich = speakers.Count > 0 || images.Count > 0 || links.Count > 0;
        related.Visible = inRelated;
        related.Display(inRelated ? v.Chat : null, selectedTab);
        trace.Visible = inTrace;
        trace.Display(inTrace ? v.Chat : null);
        history.Visible = !inTrace && rich;
        browserPanel.Display(page == Page.Browser ? v.Browser : null);
        invitePanel.Display(page == Page.Invite && v.CanInvite ? v.Invite : null, page == Page.Invite && v.CanInvite);
        output.Visible = page is not (Page.Invite or Page.Browser) && !inRelated && !inTrace && !rich && page is not (Page.DesktopChats or Page.Agents) && !showingSource && !showingTerminal && !inWorkspaceTerminal;
        terminal.Visible = showingTerminal;
        list.Visible = rows.Count > 0;
        if (focusNetworkList && page == Page.Settings)
        {
            focusNetworkList = false;
            FocusNetworkList();
        }
        terminal.Display(showingTerminal ? v.Terminal : null);
        history.Display(readingScope, lines, images, blocks, latestPage, links, speakers, styles);
        var text = !rich ? string.Join("\r\n", lines.Where(x => !string.IsNullOrEmpty(x))) : "";
        bool textChanged = output.Text != text;
        if (textChanged)
        {
            int pos = output.SelectionStart;
            output.Text = text;
            output.Select(Math.Min(pos, text.Length), 0);
        }
        TextBoxBase reader = rich ? history : output;
        plainOffsets = readingScope != null && !rich ? TranscriptViewport.PlainOffsets(blocks, lines) : [];
        if (readingScope != null)
        {
            if (previousScope != readingScope)
            {
                reader.Select(0, 0);
                if (latestPage) TranscriptViewport.End(reader);
            }
            else if (reader != previousReader || (reader == output && textChanged))
            {
                previousViewport?.Restore(reader, rich ? history.BlockOffsets : plainOffsets, latestPage);
            }
        }
        jump.Display(readingScope != null ? v.Chat : null, reader);
    }
}

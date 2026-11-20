using Muster.App;
using SessionAction = Muster.App.SessionTerminalAction.Types.Action;

namespace Muster.Windows;

internal sealed class ConversationTerminal : UserControl
{
    readonly FlowLayoutPanel toolbar = new() { Dock = DockStyle.Top, AutoSize = true, WrapContents = true };
    readonly Label directory = new() { Dock = DockStyle.Top, AutoEllipsis = true, Height = 30 };
    readonly Label status = new() { Dock = DockStyle.Bottom, AutoEllipsis = true, Height = 30 };
    readonly TerminalGrid grid = new() { Dock = DockStyle.Fill };
    readonly Button connect, disconnect, paste, options;
    ChatView? chat;
    TerminalView? terminal;

    public event Action<Intent>? Intent;
    public event Action? ReturnToChat;

    public ConversationTerminal()
    {
        Name = "ConversationTerminal";
        Controls.Add(grid);
        Controls.Add(status);
        Controls.Add(directory);
        Controls.Add(toolbar);
        grid.Intent += intent => Intent?.Invoke(intent);
        grid.ClipboardError += error => status.Text = error;
        AddButton("Conversation", () => ReturnToChat?.Invoke());
        connect = AddButton("Connect", () => Act(SessionAction.Start));
        disconnect = AddButton("Disconnect", () => Act(SessionAction.Detach));
        paste = AddButton("Paste", () => { grid.PasteClipboard(); grid.Focus(); });
        options = AddButton("Terminal actions…", ShowOptions);
    }

    Button AddButton(string label, Action clicked)
    {
        var button = new Button { Text = label, AutoSize = true };
        button.Click += (_, _) => clicked();
        toolbar.Controls.Add(button);
        return button;
    }
    void Act(SessionAction action) => Intent?.Invoke(new() { SessionTerminalAction = new() { Action = action } });

    public void Display(ChatView? conversation, TerminalView? next)
    {
        chat = conversation;
        terminal = next;
        if (chat?.Terminal is not {} workspace) { grid.Display(null); return; }
        directory.Text = workspace.Cwd;
        status.Text = workspace.Error.Length > 0 ? workspace.Error
                    : workspace.Busy ? "Updating terminal…"
                    : next?.Status is { Length: > 0 } state ? state
                    : workspace.Attached ? "Connected" : "Not connected";
        connect.Visible = !workspace.Attached;
        connect.Enabled = workspace.Available && !workspace.Busy;
        disconnect.Visible = workspace.Attached;
        disconnect.Enabled = !workspace.Busy;
        paste.Visible = next is { CanPaste: true, Ended: false } && workspace.Attached;
        paste.Enabled = !workspace.Busy;
        options.Enabled = !workspace.Busy && (workspace.CanRestart || workspace.CanEnd ||
            workspace.Attached && next != null && (next.NativeSources.Count > 0 ||
            next.DesktopAttachCommand.Length > 0 || next.Iterm2AttachCommand.Length > 0));
        grid.Visible = next != null && (workspace.Attached || next.Ended);
        grid.Enabled = !workspace.Busy;
        grid.Display(grid.Visible ? next : null);
    }

    internal ContextMenuStrip CreateOptions()
    {
        var menu = new ContextMenuStrip();
        if (chat?.Terminal is not {} workspace) return menu;
        var device = chat.DeviceId;
        var agent = chat.Session.Id;
        var handle = terminal?.Session;
        bool Current() => chat?.DeviceId == device && chat.Session.Id == agent &&
            chat.Terminal is { Busy: false } && terminal?.Session == handle;
        void Choice(string label, Func<bool> allowed, Action action)
        {
            var item = menu.Items.Add(label);
            item.Enabled = Current() && allowed();
            item.Click += (_, _) => { if (Current() && allowed()) action(); };
        }

        if (workspace.Attached && terminal is { Ended: false } opened)
        {
            foreach (var source in opened.NativeSources)
                Choice("Open in " + source.Label,
                    () => terminal is { Ended: false } t && t.NativeSources.Any(s => s.Source == source.Source),
                    () => Intent?.Invoke(new() { TerminalCreate = new() { Source = source.Source, AttachManaged = true } }));
            void Copy(string label, Func<TerminalView, string> command)
            {
                if (command(opened).Length == 0) return;
                Choice(label, () => terminal is { Ended: false } t && command(t).Length > 0, () =>
                {
                    try { Clipboard.SetText(command(terminal!)); }
                    catch (System.Runtime.InteropServices.ExternalException ex) { status.Text = ex.Message; }
                });
            }
            Copy("Copy desktop connection command", t => t.DesktopAttachCommand);
            Copy("Copy iTerm2 connection command", t => t.Iterm2AttachCommand);
        }
        if (menu.Items.Count > 0) menu.Items.Add(new ToolStripSeparator());
        Choice("Restart terminal", () => chat!.Terminal.CanRestart, () => Act(SessionAction.Restart));
        Choice("End terminal", () => chat!.Terminal.CanEnd, () => Act(SessionAction.End));
        return menu;
    }

    void ShowOptions()
    {
        var menu = CreateOptions();
        menu.ShowBelow(options);
    }
}

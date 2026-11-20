using Muster.App;
using Muster.Windows;
using SessionAction = Muster.App.SessionTerminalAction.Types.Action;

internal static class ConversationTerminalTests
{
    static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    public static void Run(string[] args)
    {
        using var form = new Form { ClientSize = new(820, 400), ShowInTaskbar = false };
        using var pane = new ConversationTerminal { Dock = DockStyle.Fill };
        form.Controls.Add(pane);
        var sent = new List<Intent>();
        pane.Intent += sent.Add;
        bool returned = false;
        pane.ReturnToChat += () => returned = true;
        var chat = new ChatView { DeviceId = "example-host", Session = new() { Id = "example-agent" },
            Terminal = new() { Available = true, Cwd = "/example/work", CanRestart = true } };
        form.Show();
        Button Button(string label) => pane.Controls.OfType<FlowLayoutPanel>().Single().Controls.OfType<Button>().Single(b => b.Text == label);
        bool Label(string text) => pane.Controls.OfType<Label>().Any(l => l.Text == text);
        var grid = pane.Controls.OfType<TerminalGrid>().Single();

        pane.Display(chat, null);
        Check(Label(chat.Terminal.Cwd) && Label("Not connected"), "Workspace cwd and initial state");
        Button("Connect").PerformClick();
        Check(sent.Single().SessionTerminalAction.Action == SessionAction.Start, "Connect should start or resume the bound shell");
        sent.Clear();
        chat.Terminal.Busy = true;
        pane.Display(chat, null);
        Button("Connect").PerformClick();
        Check(sent.Count == 0 && Label("Updating terminal…"), "Starting state accepted a duplicate connect");
        chat.Terminal.Busy = false;
        chat.Terminal.Error = "Fixture host cannot provide a workspace terminal";
        pane.Display(chat, null);
        Check(Label(chat.Terminal.Error) && !grid.Visible, "Host error was not shown");

        chat.Terminal.Error = "";
        chat.Terminal.Attached = true;
        chat.Terminal.CanEnd = true;
        var view = new TerminalView { DeviceId = chat.DeviceId, Session = "example-handle", Source = "tmux",
            Status = "attached", Cols = 50, Rows = 8, CanPaste = true, DesktopAttachCommand = "example desktop command",
            NativeSources = { new Muster.V1.TerminalSource { Source = "iterm2", Label = "iTerm2" } } };
        view.Lines.Add(new Muster.V1.TerminalLine { Index = 0, Text = "Example workspace terminal" });
        pane.Display(chat, view);
        Check(grid.Visible && Label("attached"), "Attached grid and status");
        grid.PasteText("fixture input");
        Check(sent.Single().TerminalPaste.Text == "fixture input", "Terminal input did not use terminal intent");
        Button("Disconnect").PerformClick();
        Check(sent.Last().SessionTerminalAction.Action == SessionAction.Detach, "Disconnect must detach");
        using var menu = pane.CreateOptions();
        ToolStripItem Item(string text) => menu.Items.Cast<ToolStripItem>().Single(i => i.Text == text);
        Item("Open in iTerm2").PerformClick();
        Check(sent.Last().TerminalCreate is { Source: "iterm2", AttachManaged: true }, "Wrong native attachment intent");
        Check(Item("Copy desktop connection command").Enabled, "Projected connection command not offered");
        Item("Restart terminal").PerformClick();
        Check(sent.Last().SessionTerminalAction.Action == SessionAction.Restart, "Restart action");
        Item("End terminal").PerformClick();
        Check(sent.Last().SessionTerminalAction.Action == SessionAction.End, "End action");

        if (args is ["--screenshots", var path])
        {
            using var image = new Bitmap(pane.Width, pane.Height);
            pane.DrawToBitmap(image, pane.ClientRectangle);
            image.Save(Path.Combine(path, "conversation-terminal.png"));
        }
        view.Ended = true;
        chat.Terminal.Attached = false;
        pane.Display(chat, view);
        Check(grid.Visible && !Button("Paste").Visible, "Ended grid should remain readable");
        int count = sent.Count;
        Item("Open in iTerm2").PerformClick();
        Check(sent.Count == count, "Ended session accepted stale native attachment");
        var other = chat.Clone();
        other.DeviceId = "second-host";
        pane.Display(other, view.Clone());
        Item("End terminal").PerformClick();
        Check(sent.Count == count, "Stale menu changed another host");
        Button("Conversation").PerformClick();
        Check(returned, "Conversation return action");
        pane.Display(null, null);
        Console.WriteLine("Passed conversation terminal state, exact lifecycle intents and stale menu checks.");
    }
}

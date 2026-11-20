using Muster.App;

namespace Muster.Windows;

// The core supplies ordered blocks, tool/result pairing, status and lazy detail state.
internal sealed class TranscriptSteps
{
    ChatView? chat;
    object scope = new();
    readonly Dictionary<string, bool> groups = new();
    readonly HashSet<(string Work, string Activity, string Tool)> tools = new();

    public void Display(ChatView? next)
    {
        if (chat?.DeviceId != next?.DeviceId || chat?.Session?.Id != next?.Session?.Id)
        {
            groups.Clear();
            tools.Clear();
            scope = new();
        }
        chat = next;
    }

    static bool HasSteps(ChatBlock b) =>
        b.Kind == "working" && (b.Children.Count > 0 || b.Details.Length > 0)
        || b.Kind == "activity" && b.ActivityRows.Count > 0;
    public bool HasStepsMenu => chat?.Timeline.Any(HasSteps) == true;
    bool IsOpen(ChatBlock b) => groups.GetValueOrDefault(b.Id, b.DefaultExpanded);

    public IEnumerable<string> Lines()
    {
        if (chat == null) yield break;
        if (chat.MessageCount == 0)
        {
            if (chat.Loading) yield return "Loading conversation…";
            else if (chat.LoadError.Length == 0)
                yield return chat.Session?.TranscriptNotice is { Length: > 0 } notice ? notice : "No messages yet";
        }
        foreach (var block in chat.Timeline)
            foreach (string line in Block(block, block.Id))
                yield return line;
    }

    IEnumerable<string> Block(ChatBlock b, string work)
    {
        if (b.Title.Length > 0) yield return b.Title;
        if (b.Message is {} m) yield return $"{m.Role}  {m.Text}";
        if (b.Id == work && HasSteps(b) && !IsOpen(b)) yield break;
        if (b.Details == "loading") yield return "Loading steps…";
        if (b.Details == "failed") yield return b.DetailsError;
        foreach (var row in b.ActivityRows)
        {
            yield return row.Status.Length == 0 ? row.Title : $"{row.Title} · {row.Status}";
            if (!tools.Contains((work, b.Id, row.Id))) continue;
            if (row.ToolName.Length > 0) yield return row.ToolName;
            if (row.Detail.Length > 0) yield return row.Detail;
            if (row.Output.Length > 0) yield return $"Output\r\n{row.Output}";
        }
        foreach (var child in b.Children)
            foreach (string line in Block(child, work))
                yield return line;
    }

    static IEnumerable<(string Activity, ChatActivityRow Row)> Rows(ChatBlock block)
    {
        foreach (var row in block.ActivityRows) yield return (block.Id, row);
        foreach (var child in block.Children)
            foreach (var entry in Rows(child)) yield return entry;
    }

    static string Label(string text)
    {
        int length = Math.Min(text.Length, 60);
        if (length > 0 && length < text.Length && char.IsHighSurrogate(text[length - 1])) length--;
        string line = string.Join(" ", text[..length].Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries));
        return (length < text.Length ? line + "…" : line).Replace("&", "&&");
    }

    public ContextMenuStrip Menu(Action<Intent> send, Action redraw)
    {
        var menu = new ContextMenuStrip { Name = "TranscriptStepsMenu", AccessibleName = "Work steps" };
        var openedIn = scope;
        ChatBlock? Current(string id) => scope == openedIn ? chat?.Timeline.FirstOrDefault(b => b.Id == id && HasSteps(b)) : null;
        string prompt = "";
        int ordinal = 0;
        if (chat == null) return menu;
        foreach (var b in chat.Timeline)
        {
            if (b.Message is { Role: "user" } message) prompt = Label(message.Text);
            if (!HasSteps(b)) continue;
            string label = $"{++ordinal}. {Label(b.Title)}";
            var group = new ToolStripMenuItem(prompt.Length > 0 ? $"{label} — {prompt}" : label);
            menu.Items.Add(group);
            bool expanded = IsOpen(b);
            group.DropDownItems.Add(expanded ? "Hide steps" : "Show steps", null, (_, _) =>
            {
                if (Current(b.Id) is not {} live || IsOpen(live) != expanded) return;
                groups[live.Id] = !expanded;
                if (!expanded && live.Details == "unloaded")
                    send(new() { AgentLoadWork = new() { Block = live.Id } });
                redraw();
            });
            if (expanded && b.Details is "failed" or "unloaded")
                group.DropDownItems.Add(b.Details == "failed" ? "Retry loading steps" : "Load steps", null, (_, _) =>
                {
                    if (Current(b.Id) is {} live && IsOpen(live) && live.Details is "failed" or "unloaded")
                        send(new() { AgentLoadWork = new() { Block = live.Id } });
                });
            if (!expanded) continue;
            foreach (var (activity, row) in Rows(b))
            {
                var key = (b.Id, activity, row.Id);
                bool shown = tools.Contains(key);
                var tool = new ToolStripMenuItem(Label(row.Title)) { Checked = shown };
                tool.Click += (_, _) =>
                {
                    if (Current(b.Id) is not {} live || !IsOpen(live) ||
                        !Rows(live).Any(e => e.Activity == activity && e.Row.Id == row.Id)) return;
                    if (shown) tools.Remove(key); else tools.Add(key);
                    redraw();
                };
                group.DropDownItems.Add(tool);
            }
        }
        return menu;
    }
}

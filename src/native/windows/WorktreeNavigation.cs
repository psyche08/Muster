using Muster.App;
using Muster.V1;

namespace Muster.Windows;

internal static class WorktreeNavigation
{
    public static Intent Parent(string path)
    {
        int slash = path.LastIndexOf('/');
        return new() { OpenFiles = new() { Path = slash < 0 ? "" : path[..slash] } };
    }

    public static Intent Entry(string directory, FileEntry entry)
    {
        string path = directory.Length == 0 ? entry.Name : directory + "/" + entry.Name;
        return entry.Dir ? new() { OpenFiles = new() { Path = path } }
                         : new() { OpenFile = new() { Path = path } };
    }

    public static string Time(ulong seconds) => seconds is > 0 and <= 253402300799
        ? DateTimeOffset.FromUnixTimeSeconds((long)seconds).ToLocalTime().ToString("g") : "";

    public static IEnumerable<AgentMessage> ChangedTurns(IEnumerable<ChatBlock> blocks)
    {
        foreach (var block in blocks)
        {
            if (block.Kind == "summary" && block.Message?.Usage is {} usage &&
                (usage.Changes.Count > 0 || usage.ChangesIncomplete))
                yield return block.Message;
            foreach (var message in ChangedTurns(block.Children)) yield return message;
        }
    }

    public static void AddChanges(ContextMenuStrip menu, IReadOnlyList<AgentMessage> turns, Action<Intent> send)
    {
        foreach (var message in turns.Reverse())
        {
            var items = menu.Items;
            if (turns.Count > 1)
            {
                string stamp = Time(message.TsUnix);
                var group = new ToolStripMenuItem($"{message.Usage.Changes.Count} changed files  {stamp}".TrimEnd());
                menu.Items.Add(group);
                items = group.DropDownItems;
            }
            if (message.Usage.ChangesIncomplete)
                items.Add(new ToolStripMenuItem("Some recorded changes are unavailable") { Enabled = false });
            foreach (var change in message.Usage.Changes)
            {
                string name = change.OpenPath.Length > 0 ? change.OpenPath : change.Path;
                string detail = change.CountsKnown ? $"+{change.Additions} −{change.Deletions}" : change.Kind;
                var item = new ToolStripMenuItem($"{name.Replace("&", "&&")}    {detail}");
                if (change.PreviousPath.Length != 0) item.ToolTipText = "From " + change.PreviousPath;
                item.Click += (_, _) => send(new() { OpenChange = new() { SummaryId = message.Id, Path = change.Path } });
                items.Add(item);
            }
        }
    }
}

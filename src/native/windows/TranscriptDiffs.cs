using Muster.App;

namespace Muster.Windows;

internal sealed partial class TranscriptSteps
{
    readonly HashSet<(string Id, string Path)> expandedDiffs = new();

    static string DiffLabel(ChatDiffFile file) => $"{file.Path}    +{file.Added} −{file.Removed}";

    static IEnumerable<string> DiffLines(ChatDiffFile file)
    {
        foreach (var line in file.Lines)
        {
            string oldLine = line.OldLine == 0 ? "" : line.OldLine.ToString();
            string newLine = line.NewLine == 0 ? "" : line.NewLine.ToString();
            string sign = line.Kind switch { "add" => "+", "delete" => "−", _ => " " };
            yield return $"{oldLine,5} {newLine,5}  {sign} {line.Text}";
        }
        if (file.Truncated) yield return "Preview shortened · Copy includes the full supplied diff";
    }

    IEnumerable<ChatDiffFile> VisibleDiffs() =>
        VisibleBlocks().Where(entry => entry.Block.Kind == "diff").SelectMany(entry => entry.Block.Diffs);

    public bool HasDiffs => VisibleDiffs().Any();

    public ContextMenuStrip DiffMenu(Action<string> copy, Action redraw)
    {
        var menu = new ContextMenuStrip { AccessibleName = "File diffs" };
        var origin = scope;
        foreach (var file in VisibleDiffs())
        {
            var key = (file.Id, file.Path);
            bool expanded = expandedDiffs.Contains(key);
            bool Current() => scope == origin && VisibleDiffs().Any(f =>
                f.Id == file.Id && f.Path == file.Path && f.Equals(file));

            var group = new ToolStripMenuItem(DiffLabel(file).Replace("&", "&&"));
            menu.Items.Add(group);
            group.DropDownItems.Add(expanded ? "Hide diff" : "Show diff", null, (_, _) =>
            {
                if (!Current() || expandedDiffs.Contains(key) != expanded) return;
                if (expanded) expandedDiffs.Remove(key); else expandedDiffs.Add(key);
                redraw();
            });
            var copyItem = group.DropDownItems.Add("Copy diff", null, (_, _) =>
            {
                if (Current() && file.Patch.Length > 0) copy(file.Patch);
            });
            copyItem.Enabled = file.Patch.Length > 0;
        }
        return menu;
    }
}

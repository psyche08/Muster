using Google.Protobuf;
using Muster.App;
using Muster.V1;
using Muster.Windows;

internal static class Program
{
    static void Check(bool valid, string reason)
    {
        if (!valid) throw new InvalidOperationException(reason);
    }

    [STAThread]
    static void Main(string[] args)
    {
        Application.SetHighDpiMode(HighDpiMode.SystemAware);
        Application.EnableVisualStyles();
        string? captures = args switch {
            [] => null,
            ["--screenshots", var path] => Directory.CreateDirectory(path).FullName,
            _ => throw new ArgumentException("Usage: WindowsWorktree [--screenshots DIRECTORY]")
        };
        using var host = new Form { Text = "Session preview", Size = new Size(750, 390),
            StartPosition = FormStartPosition.Manual, Location = new Point(-20000, -20000), ShowInTaskbar = false };
        var preview = new SourcePreview { Dock = DockStyle.Fill };
        host.Controls.Add(preview);
        host.Show();
        var source = preview.Controls.OfType<RichTextBox>().Single();
        var heading = preview.Controls.OfType<Label>().Single();
        var gutter = preview.Controls.OfType<Panel>().Single();

        void Capture(string name)
        {
            if (captures == null) return;
            using var bitmap = new Bitmap(preview.Width, preview.Height);
            preview.DrawToBitmap(bitmap, new Rectangle(Point.Empty, preview.Size));
            bitmap.Save(Path.Combine(captures, name + ".png"), System.Drawing.Imaging.ImageFormat.Png);
        }
        string raw = "First line\nSecond line\t{sample}\n最后一行";
        var file = new FileView { Path = "docs/example.txt", Content = ByteString.CopyFromUtf8(raw), Truncated = true };
        preview.ShowFile(file);
        Check(source.Text == raw && gutter.Visible && heading.Text.Contains("truncated"), "numbered text and truncation");
        Capture("current-file");
        source.Select(0, 5);
        preview.ShowFile(file.Clone());
        Check(source.SelectedText == "First", "keep selection on a repeated view");

        string patch = "--- a/example\n+++ b/example\n@@ -1 +1 @@\n-old\n+new\t{\\field} 简 😀";
        file = new() { IsDiff = true, CurrentPath = "docs/example.txt", Content = ByteString.CopyFromUtf8(patch) };
        preview.ShowFile(file);
        Check(source.Text == patch && !gutter.Visible && heading.Text.Contains("Recorded"), "literal patch text");
        Color ColorAt(string needle)
        {
            source.Select(source.Text.IndexOf(needle, StringComparison.Ordinal), 1);
            return source.SelectionColor;
        }
        if (!SystemInformation.HighContrast)
            Check(ColorAt("+new") != ColorAt("-old") && ColorAt("+++") != ColorAt("+new"), "diff coloring");
        source.Select(0, 0);
        Capture("recorded-diff");
        foreach (var bytes in new byte[][] { [0xff, 0x80], [65, 0, 66] })
        {
            preview.ShowFile(new() { Content = ByteString.CopyFrom(bytes) });
            Check(source.Text.Contains("Binary file") && !gutter.Visible, "binary payload");
        }
        preview.ShowFile(new());
        Check(source.Text == "Empty file.", "empty file");
        preview.ShowFile(new() { Loading = true, Content = ByteString.CopyFromUtf8("previous") });
        Check(source.Text.Contains("Loading") && !gutter.Visible, "loading hides old content");
        preview.ShowFile(new() { Error = "Preview unavailable", Content = ByteString.CopyFromUtf8("previous") });
        Check(source.Text == "Preview unavailable", "error precedes bytes");
        preview.ShowCommit(new() { DiffCommit = "abcd", Diff = patch });
        Check(source.Text == patch && heading.Text.Contains("abcd"), "commit diff");
        preview.ShowCommit(new() { DiffCommit = "abcd" });
        Check(source.Text.Contains("No diff"), "empty commit diff");

        Check(WorktreeNavigation.Parent("docs/sub").OpenFiles.Path == "docs"
              && WorktreeNavigation.Parent("docs").OpenFiles.Path == "", "directory parent");
        Check(WorktreeNavigation.Entry("docs", new FileEntry { Name = "a.txt" }).OpenFile.Path == "docs/a.txt"
              && WorktreeNavigation.Entry("", new FileEntry { Name = "docs", Dir = true }).OpenFiles.Path == "docs",
              "relative entries");

        var summary = new ChatBlock { Kind = "summary", Message = new() { Id = "turn", Usage = new() {
            ChangesIncomplete = true, Changes = { new TurnFileChange { Path = "recorded", OpenPath = "work/a",
                CountsKnown = true, Additions = 3, Deletions = 1 } } } } };
        var turns = WorktreeNavigation.ChangedTurns([new ChatBlock { Children = { summary } }]).ToArray();
        using var menu = new ContextMenuStrip();
        Intent? sent = null;
        WorktreeNavigation.AddChanges(menu, turns, request => sent = request);
        Check(menu.Items.Count == 2 && !menu.Items[0].Enabled && menu.Items[1].Text!.Contains("work/a"), "change list");
        menu.Items[1].PerformClick();
        Check(sent?.OpenChange?.SummaryId == "turn" && sent.OpenChange.Path == "recorded", "original recorded change identity");
        Console.WriteLine($"Passed source, binary, recorded change, directory and commit previews at {host.DeviceDpi} DPI.");
    }
}

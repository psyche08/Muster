using Muster.Windows;
using System.Runtime.InteropServices;

internal static class TranscriptViewportTests
{
    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint Message(nint hwnd, uint message, nint wParam, nint lParam);

    public static void Run()
    {
        using var form = new Form { ClientSize = new(520, 280), StartPosition = FormStartPosition.Manual,
            Location = new(-20000, -20000), ShowInTaskbar = false };
        using var text = new TextBox { Dock = DockStyle.Fill, Multiline = true, ReadOnly = true,
            WordWrap = false, ScrollBars = ScrollBars.Both };
        form.Controls.Add(text);
        form.Show();
        List<BlockOffset> offsets = [];
        void Show(int firstId, int count)
        {
            var previous = new TranscriptViewport(text, offsets);
            var blocks = new List<TranscriptBlock>();
            var lines = new List<string>();
            foreach (int id in Enumerable.Range(firstId, count))
            {
                blocks.Add(new(id.ToString(), lines.Count, 0));
                lines.Add($"Row {id}");
                lines.Add("Repeated 🙂 text");
            }
            text.Text = string.Join(Environment.NewLine, lines);
            offsets = TranscriptViewport.PlainOffsets(blocks, lines);
            previous.Restore(text, offsets, latestPage: true);
        }
        int First() => (int)Message(text.Handle, 0x00CE, 0, 0);
        Show(0, 150);
        if (JumpToLatest.CanScrollDown(text)) throw new Exception("Initial text did not follow latest");
        int selected = text.Text.IndexOf("Row 50", StringComparison.Ordinal) + "Row 50\r\n".Length;
        text.Select(selected, "Repeated 🙂".Length);
        Message(text.Handle, 0x0115, 6, 0);
        Message(text.Handle, 0x00B6, 0, 90);
        int line = First();
        Show(0, 155);
        if (First() != line || text.SelectionStart != selected) throw new Exception("Append moved reader");
        Show(-10, 165);
        if (First() != line + 20 || text.SelectedText != "Repeated 🙂" ||
            text.SelectionStart != text.Text.IndexOf("Row 50", StringComparison.Ordinal) + "Row 50\r\n".Length)
            throw new Exception("Prepend lost the selected row");
        Show(80, 80);
        if (text.SelectionLength != 0) throw new Exception("Removed selection highlighted another block");
        Console.WriteLine($"Passed plain transcript viewport, duplicate text and trimmed selection at {text.DeviceDpi} DPI.");
    }
}

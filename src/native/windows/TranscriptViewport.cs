using System.Runtime.InteropServices;

namespace Muster.Windows;

// Locations come from the projected blocks; offsets are native UTF-16 character positions.
internal readonly record struct TranscriptBlock(string Id, int Line, int Picture);
internal readonly record struct BlockOffset(string Id, int Start);

internal sealed class TranscriptViewport
{
    readonly List<BlockOffset> blocks;
    readonly int top, topY, selection, selectionEnd;
    readonly bool atEnd;

    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint Message(nint hwnd, uint message, nint wParam, nint lParam);
    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint ScrollPosition(nint hwnd, uint message, nint unused, ref Point position);

    public TranscriptViewport(TextBoxBase reader, List<BlockOffset> previous)
    {
        blocks = previous;
        atEnd = !JumpToLatest.CanScrollDown(reader);
        // RichEdit can display a fraction of a line or image. Preserve its offset in pixels.
        top = reader is RichTextBox ? reader.GetCharIndexFromPosition(new Point(2, 1))
            : Math.Max(0, reader.GetFirstCharIndexFromLine((int)Message(reader.Handle, 0x00CE, 0, 0)));
        topY = reader.GetPositionFromCharIndex(top).Y;
        selection = reader.SelectionStart;
        selectionEnd = selection + reader.SelectionLength;
    }

    public static void End(TextBoxBase reader) =>
        Message(reader.Handle, 0x0115, 7, 0); // WM_VSCROLL, SB_BOTTOM

    public void Restore(TextBoxBase reader, List<BlockOffset> next, bool latestPage)
    {
        int Resolve(int index, out bool survived)
        {
            int old = blocks.FindLastIndex(b => b.Start <= index);
            survived = old < 0;
            if (old < 0) return Math.Min(index, reader.TextLength);
            int current = next.FindIndex(b => b.Id == blocks[old].Id);
            if (current < 0)
            {
                // When a bounded history window drops this row, read from the first
                // surviving successor. A discarded selection must not highlight a new row.
                var remaining = blocks.Skip(old + 1).Select(b => b.Id).ToHashSet();
                int successor = next.FindIndex(b => remaining.Contains(b.Id));
                return successor < 0 ? 0 : Math.Min(next[successor].Start, reader.TextLength);
            }
            survived = true;
            int baseOffset = Math.Min(next[current].Start, reader.TextLength);
            int limit = current + 1 < next.Count ? Math.Min(next[current + 1].Start, reader.TextLength) : reader.TextLength;
            return baseOffset + Math.Min(index - blocks[old].Start, limit - baseOffset);
        }

        int start = Resolve(selection, out bool startSurvived);
        int end = Resolve(selectionEnd, out bool endSurvived);
        reader.Select(start, startSurvived && endSurvived ? Math.Max(0, end - start) : 0);
        bool prepended = blocks.Count > 0 && next.FindIndex(b => b.Id == blocks[0].Id) > 0;
        if (atEnd && latestPage && !prepended)
        {
            End(reader);
            return;
        }
        int anchor = Resolve(top, out _);
        if (reader is RichTextBox)
        {
            var position = Point.Empty;
            ScrollPosition(reader.Handle, 0x04DD, 0, ref position); // EM_GETSCROLLPOS
            position.Y = Math.Max(0, position.Y + reader.GetPositionFromCharIndex(anchor).Y - topY);
            ScrollPosition(reader.Handle, 0x04DE, 0, ref position); // EM_SETSCROLLPOS
        }
        else
        {
            int first = (int)Message(reader.Handle, 0x00CE, 0, 0); // EM_GETFIRSTVISIBLELINE
            int desired = reader.GetLineFromCharIndex(anchor);
            Message(reader.Handle, 0x00B6, 0, desired - first); // EM_LINESCROLL
        }
    }

    public static List<BlockOffset> PlainOffsets(List<TranscriptBlock> blocks, List<string> lines)
    {
        var result = new List<BlockOffset>(blocks.Count);
        int length = 0, block = 0;
        for (int line = 0; line <= lines.Count; line++)
        {
            while (block < blocks.Count && blocks[block].Line == line)
                result.Add(new(blocks[block++].Id, length));
            if (line < lines.Count && lines[line].Length > 0)
                length += lines[line].Length + Environment.NewLine.Length;
        }
        // The plain reader joins nonempty lines without a trailing CRLF.
        int last = Math.Max(0, length - Environment.NewLine.Length);
        for (int i = result.Count - 1; i >= 0 && result[i].Start > last; i--)
            result[i] = result[i] with { Start = last };
        return result;
    }
}

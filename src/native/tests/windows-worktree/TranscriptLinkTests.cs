using System.Runtime.InteropServices;
using Google.Protobuf;
using Muster.App;
using Muster.Windows;

internal static class TranscriptLinkTests
{
    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint Message(nint window, uint message, nint first, nint second);

    public static void Run()
    {
        const string source = "😀 first\r\n[guide](docs/a%20b.md)[guide][other] and `[literal](skip.txt)` " +
            "and \\[escaped](skip.md)\n\n[other]: docs/b.txt";
        var block = new ChatBlock { Id = "one", Kind = "message", Message = new() { Role = "assistant", Text = source } };
        var view = new ChatView { DeviceId = "test", Session = new() { Id = "chat" }, MessageCount = 1 };
        view.Timeline.Add(block);
        var steps = new TranscriptSteps();
        steps.Display(view);
        var lines = new List<string>();
        var links = new List<TranscriptLink>();
        var images = new List<TranscriptImage>();
        steps.AppendTo(lines, images, links: links);
        if (!links.Select(l => l.Destination).SequenceEqual(new[] { "docs/a%20b.md", "docs/b.txt" }))
            throw new Exception("Wrong Markdown destinations");
        var code = new ChatBlock { Kind = "message", Message = new() { Role = "assistant" } };
        code.Parts.Add(new MessagePart { Kind = "code", Text = "[example](ignored.txt)" });
        view.Timeline.Add(code);
        view.Timeline.Add(new ChatBlock { Kind = "message", Message = new() { Role = "user", Text = "[prompt](ignored.md)" } });
        var mixed = new List<TranscriptLink>();
        steps.AppendTo([], [], links: mixed);
        if (!mixed.SequenceEqual(links)) throw new Exception("Interpreted plain prompt or code as Markdown");
        using var form = new Form { Size = new Size(650, 360), ShowInTaskbar = false,
            StartPosition = FormStartPosition.Manual, Location = new Point(-20000, -20000) };
        using var reader = new HistoryTranscript { Dock = DockStyle.Fill };
        form.Controls.Add(reader);
        form.Show();
        var opened = new List<string>();
        reader.ActivateLink += opened.Add;
        void Click(int start)
        {
            Point point = reader.GetPositionFromCharIndex(start + 1);
            var xy = (point.X + 1) | ((point.Y + 2) << 16);
            Message(reader.Handle, 0x0201, 1, xy);
            Message(reader.Handle, 0x0202, 0, xy);
        }
        foreach (bool picture in new[] { false, true })
        {
            if (picture) images.Add(new(0, ByteString.CopyFrom([1, 2, 3])));
            reader.Display(("device", "chat"), lines, images, projectedLinks: links);
            Application.DoEvents();
            reader.Focus();
            int first = reader.Text.IndexOf("guide", StringComparison.Ordinal);
            int second = reader.Text.IndexOf("guide", first + 1, StringComparison.Ordinal);
            opened.Clear();
            Click(first);
            Click(second);
            if (!opened.SequenceEqual(new[] { "docs/a%20b.md", "docs/b.txt" }))
                throw new Exception($"Mouse link labels resolved to '{string.Join(",", opened)}' with picture={picture}");
            reader.Select(second, 0);
            Message(reader.Handle, 0x0100, 13, 0);
            if (opened.Count != 3 || opened[2] != "docs/b.txt") throw new Exception("Keyboard activation");
            reader.Display(("device", "other"), ["ordinary text"], [], projectedLinks: []);
            if (reader.TextLength != 0) throw new Exception("Stale rich transcript on session change");
        }
        Console.WriteLine("Passed native mouse and keyboard links, same labels, Markdown escapes, Unicode and image offsets.");
    }
}

using Google.Protobuf;
using Muster.App;
using Muster.Windows;
using System.Diagnostics;
using System.Drawing.Imaging;

internal static class HistoryImageTests
{
    [System.Runtime.InteropServices.DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint Scroll(nint window, uint message, nint action, nint unused);

    static ByteString Jpeg(Color color)
    {
        using var image = new Bitmap(180, 35);
        using var g = Graphics.FromImage(image);
        g.Clear(color);
        using var buffer = new MemoryStream();
        image.Save(buffer, ImageFormat.Jpeg);
        return ByteString.CopyFrom(buffer.ToArray());
    }

    static void Check(bool success, string why)
    {
        if (!success) throw new InvalidOperationException(why);
    }

    public static void Run(string? captures)
    {
        var red = Jpeg(Color.Red);
        var blue = Jpeg(Color.Blue);
        var bad = ByteString.CopyFrom(new byte[] { 0xff, 0xd8, 0xff, 0xd9 });
        const string literal = @"After {\field sample} 中🙂";
        var chat = new ChatView { DeviceId = "fixture", Session = new() { Id = "images" }, MessageCount = 3 };
        chat.Timeline.Add(new ChatBlock { Kind = "message", Message = new() { Role = "user", Text = "Before" } });
        chat.Timeline.Add(new ChatBlock { Kind = "message", Message = new() { Role = "user", Text = literal,
            PreviewJpegs = { red, blue, bad } } });
        var work = new ChatBlock { Id = "work", Kind = "working", Title = "Work", Children = {
            new ChatBlock { Kind = "message", Message = new() { Role = "assistant", PreviewJpegs = { red } } } } };
        chat.Timeline.Add(work);
        var steps = new TranscriptSteps();
        using var form = new Form { Size = new(760, 680), ShowInTaskbar = false, StartPosition = FormStartPosition.Manual,
            Location = new Point(-20000, -20000) };
        using var history = new HistoryTranscript { Dock = DockStyle.Fill };
        form.Controls.Add(history);
        form.Show();
        (List<string> Lines, List<TranscriptImage> Images) Show(ChatView view)
        {
            steps.Display(view);
            var lines = new List<string>();
            var images = new List<TranscriptImage>();
            var blocks = new List<TranscriptBlock>();
            var speakers = new HashSet<int>();
            steps.AppendTo(lines, images, blocks, speakers: speakers);
            history.Display((view.DeviceId, view.Session.Id), lines, images, blocks, latest: true, speakerLines: speakers);
            return (lines, images);
        }
        var parts = Show(chat);
        Check(parts.Images.Count == 3 && parts.Images.All(i => i.Line == parts.Lines.IndexOf(literal) && parts.Lines[i.Line - 1] == "You"), "Nested work or image order");
        var until = Stopwatch.StartNew();
        while (!history.Text.Contains("Image preview unavailable") && until.ElapsedMilliseconds < 8000)
        {
            Application.DoEvents();
            Thread.Sleep(5);
        }
        string rendered = history.Text;
        Check(rendered.Contains("Image preview unavailable") && rendered.Contains(literal), "Missing fallback or literal text: " + rendered);
        using var bitmap = new Bitmap(history.Width, history.Height);
        history.DrawToBitmap(bitmap, history.ClientRectangle);
        if (captures != null) bitmap.Save(Path.Combine(captures, "history-images.png"));
        int ColorRow(bool redPixel)
        {
            for (int y = 0; y < bitmap.Height; y++)
            {
                var c = bitmap.GetPixel(30, y);
                if (redPixel ? c.R > 200 && c.B < 40 : c.B > 200 && c.R < 40) return y;
            }
            return -1;
        }
        int redY = ColorRow(true), blueY = ColorRow(false);
        Check(redY > history.GetPositionFromCharIndex(rendered.IndexOf("Before")).Y && blueY > redY &&
            blueY < history.GetPositionFromCharIndex(rendered.IndexOf(literal)).Y,
            $"Inline pixel order: red={redY}, blue={blueY}");
        history.Select(rendered.IndexOf(literal), literal.Length);
        Show(chat.Clone());
        Check(history.SelectedText == literal, "Repeated projection changed selection");
        work.DefaultExpanded = true;
        parts = Show(chat);
        Check(parts.Images.Count == 4 && parts.Images.Last().Bytes == red, "Expanded image-only turn");
        var next = chat.Clone();
        next.DeviceId = "another";
        next.Timeline.Clear();
        Show(next);
        Application.DoEvents();
        Check(history.TextLength == 0, "Previous images or text survived navigation");
        var many = new ChatView { DeviceId = "fixture", Session = new() { Id = "many" }, MessageCount = 120 };
        for (int i = 0; i < 119; i++)
            many.Timeline.Add(new ChatBlock { Id = $"item-{i}", Kind = "message", Message = new() { Text = $"Sample {i}", PreviewJpegs = { red } } });
        many.Timeline.Add(new ChatBlock { Id = "last-image", Kind = "message", Message = new() { Text = "Last thumbnail", PreviewJpegs = { blue } } });
        Show(many);
        bool HasPixels(bool redPixel)
        {
            using var frame = new Bitmap(history.Width, history.Height);
            history.DrawToBitmap(frame, history.ClientRectangle);
            for (int y = 0; y < frame.Height; y++)
            {
                var c = frame.GetPixel(30, y);
                var c2 = frame.GetPixel(70, y);
                if (c.G < 40 && c2.G < 40 && (redPixel ? c.R > 200 && c.B < 40 && c2.R > 200 && c2.B < 40
                    : c.B > 200 && c.R < 40 && c2.B > 200 && c2.R < 40)) return true;
            }
            return false;
        }
        void WaitForPixels(bool redPixel)
        {
            var watch = Stopwatch.StartNew();
            do
            {
                Application.DoEvents();
                if (HasPixels(redPixel)) return;
                Thread.Sleep(10);
            } while (watch.ElapsedMilliseconds < 5000);
            throw new InvalidOperationException($"Scrolled thumbnail did not appear. Top character {history.GetCharIndexFromPosition(new Point(10,10))}, selected {history.SelectionStart}, last image Y {history.GetPositionFromCharIndex(history.TextLength-1).Y}");
        }
        WaitForPixels(true);
        // Select the middle of a long transcript, then prepend identical-looking blocks.
        history.Select(history.Text.IndexOf("Sample 60", StringComparison.Ordinal), 9);
        history.ScrollToCaret();
        Application.DoEvents();
        int selectionY = history.GetPositionFromCharIndex(history.SelectionStart).Y;
        for (int i = 0; i < 10; i++)
            many.Timeline.Insert(0, new ChatBlock { Id = $"prepend-{i}", Kind = "message",
                Message = new() { Text = "Sample 60", PreviewJpegs = { red } } });
        Show(many);
        Check(history.SelectedText == "Sample 60" && history.SelectionStart == history.Text.LastIndexOf("Sample 60", StringComparison.Ordinal) &&
            Math.Abs(history.GetPositionFromCharIndex(history.SelectionStart).Y - selectionY) <= history.Font.Height,
            "Prepending pictures lost the original selected block or its viewport");
        using var jump = new JumpToLatest();
        form.Controls.Add(jump);
        jump.BringToFront();
        int requests = 0;
        jump.Intent += intent => { Check(intent.AgentLoadEarlier?.Latest == true, "Expected latest history intent"); requests++; };
        history.Select(0, 4);
        jump.Display(many, history);
        Check(jump.Visible && jump.Enabled, "Jump missing above the last image");
        jump.PerformClick();
        Check(requests == 1 && history.SelectionLength == 4, "Image jump changed the selection");
        WaitForPixels(false);
        jump.UpdateAvailability();
        Check(!jump.Visible, "Jump still visible at last image");
        Scroll(history.Handle, 0x00B6, 0, -10000);
        WaitForPixels(true);
        // A full DIB for all 120 occurrences would exceed 6 MB of hex even for these small thumbnails.
        Check(history.Rtf!.Length < 3000000, "Offscreen thumbnails were expanded into full bitmaps");
        var race = new ChatView { DeviceId = "fixture", Session = new() { Id = "race" }, MessageCount = 1 };
        race.Timeline.Add(new ChatBlock { Message = new() { Text = "old scope", PreviewJpegs = { Jpeg(Color.Green) } } });
        Show(race);
        Show(next);
        var settle = Stopwatch.StartNew();
        while (settle.ElapsedMilliseconds < 130) { Application.DoEvents(); Thread.Sleep(5); }
        Check(history.TextLength == 0, "Async completion restored a previous conversation");
        using var plain = new TextBox { Dock = DockStyle.Fill, Multiline = true, ScrollBars = ScrollBars.Both,
            ReadOnly = true, WordWrap = false };
        form.Controls.Add(plain);
        plain.BringToFront();
        jump.BringToFront();
        plain.Text = "Short earlier page";
        jump.Display(many, plain);
        Check(!jump.Visible, "Short current page offers a jump");
        many.HasNewer = true;
        many.Loading = true;
        jump.Display(many, plain);
        Check(jump.Visible && !jump.Enabled, "In-flight earlier page allowed another request");
        many.Loading = false;
        jump.Display(many, plain);
        jump.PerformClick();
        plain.Lines = Enumerable.Range(1, 400).Select(i => $"Line {i}").ToArray();
        many.HasNewer = false;
        jump.Display(many, plain);
        int LastY() => plain.GetPositionFromCharIndex(plain.TextLength - 1).Y;
        Check(requests == 2 && !jump.Visible && LastY() >= 0 && LastY() < plain.ClientSize.Height,
            "Latest plain-text page did not land at its last line");
        plain.Select(0, 4);
        plain.ScrollToCaret();
        jump.UpdateAvailability();
        jump.PerformClick();
        plain.Lines = plain.Lines.Skip(180).ToArray();
        plain.Select(0, 0);
        plain.ScrollToCaret();
        jump.Display(many, plain);
        Check(requests == 3 && !jump.Visible && LastY() >= 0 && LastY() < plain.ClientSize.Height,
            "A shorter latest projection lost the viewport after a local jump");
        many.HasNewer = true;
        jump.Display(many, plain);
        jump.PerformClick();
        plain.Select(0, 4);
        plain.ScrollToCaret();
        jump.Display(next, plain);
        Check(requests == 4 && jump.Visible && plain.SelectionStart == 0 && LastY() >= plain.ClientSize.Height,
            "A pending jump scrolled the new device's transcript");
        jump.Display(null, null);
        Check(!jump.Visible, "Jump visible outside conversation");
        Console.WriteLine($"Passed jump to latest in plain and image transcripts at {history.DeviceDpi} DPI.");
        Console.WriteLine($"Passed inline history pixels, corrupt image, disclosure and selection at {history.DeviceDpi} DPI.");
    }
}

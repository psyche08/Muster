using Google.Protobuf;
using Muster.App;
using Muster.Windows;

internal static class Program
{
    sealed class FixtureGrid : TerminalGrid
    {
        public bool Key(Keys key)
        {
            var msg = Message.Create(Handle, 0x0100, (nint)(key & Keys.KeyCode), 0);
            return ProcessCmdKey(ref msg, key);
        }
        public void MessageInput(int id, char ch = '\0')
        {
            var msg = Message.Create(Handle, id, (nint)ch, 0);
            WndProc(ref msg);
        }
        public void Drag(Point from, Point to)
        {
            OnMouseDown(new(MouseButtons.Left, 1, from.X, from.Y, 0));
            OnMouseMove(new(MouseButtons.Left, 0, to.X, to.Y, 0));
            OnMouseUp(new(MouseButtons.Left, 1, to.X, to.Y, 0));
        }
    }

    static void Check(bool condition, string reason)
    {
        if (!condition) throw new InvalidOperationException(reason);
    }
    static void Pump(int ms)
    {
        var watch = System.Diagnostics.Stopwatch.StartNew();
        while (watch.ElapsedMilliseconds < ms) { Application.DoEvents(); Thread.Sleep(5); }
    }
    [STAThread]
    static void Main(string[] args)
    {
        Application.SetHighDpiMode(HighDpiMode.PerMonitorV2);
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        using var form = new Form { ClientSize = new(640, 300), ShowInTaskbar = false };
        using var grid = new FixtureGrid { Dock = DockStyle.Fill };
        var intents = new List<Intent>();
        grid.Intent += intents.Add;
        form.Controls.Add(grid);
        form.Show();
        var fixedView = new TerminalView { DeviceId = "fixture", Session = "native-window", Cols = 16, Rows = 8,
            CursorX = 7, CursorY = 2, CanPaste = true };
        var bg = ByteString.CopyFrom(new byte[] { 0, 4, 1, 0xff, 0xff, 0xff, 1, 0x15, 0x40, 0x75, 1 });
        fixedView.Lines.Add(new Muster.V1.TerminalLine { Index = 1, Text = "A🙂BC", Styles = bg });
        fixedView.Lines.Add(new Muster.V1.TerminalLine { Index = 2, Text = "> " });
        grid.Display(fixedView);
        grid.Focus();
        Pump(200);
        Check(intents.Count == 0, "Viewing a native terminal resized its host");
        Check(grid.AccessibilityObject.Value!.Contains("A🙂BC"), "Unicode frame not accessible");
        using var image = new Bitmap(grid.Width, grid.Height);
        grid.DrawToBitmap(image, grid.ClientRectangle);
        int blue = Color.FromArgb(0x15, 0x40, 0x75).ToArgb();
        int row = Enumerable.Range(0, image.Height).First(y => image.GetPixel(0, y).ToArgb() == blue);
        int width = Enumerable.Range(0, image.Width).TakeWhile(x => image.GetPixel(x, row).ToArgb() == blue).Count() / 4;
        Check(row > 0 && width > 0, "Missing indexed styled row");
        Check(image.GetPixel(7 * width, 2 * row).ToArgb() == Color.Silver.ToArgb(), "Projected cursor missing");
        grid.Drag(new(1 + width, row + 1), new(1 + 2 * width, row + 1));
        Check(grid.SelectedText() == "🙂B", "Selection split a Unicode scalar");
        if (args is ["--screenshots", var dir]) { Directory.CreateDirectory(dir); image.Save(Path.Combine(dir, "terminal-grid.png")); }
        else if (args.Length != 0) throw new ArgumentException("Usage: WindowsTerminal [--screenshots DIRECTORY]");

        foreach (char ch in "ab🙂") grid.MessageInput(0x0102, ch);
        grid.MessageInput(0x010d);
        grid.Key(Keys.Return);
        grid.MessageInput(0x0286, '中');
        grid.MessageInput(0x010e);
        grid.Key(Keys.Control | Keys.Alt | Keys.E);
        grid.MessageInput(0x0102, '€');
        grid.Key(Keys.Left);
        grid.Key(Keys.Return);
        grid.Key(Keys.Control | Keys.Shift | Keys.F5);
        grid.PasteText("one\r\ntwo");
        string[] actual = intents.Select(i => i.TerminalKey?.Key ?? i.TerminalText?.Text ?? i.TerminalPaste?.Text ?? "?").ToArray();
        Check(actual.SequenceEqual(new[] { "a", "b", "🙂", "中", "€", "left", "enter", "ctrl-shift-f5", "one\r\ntwo" }), "Input order or mapping: " + string.Join(", ", actual));
        Check(intents.Last().TerminalPaste != null, "Paste bypassed shared paste policy");

        grid.MessageInput(0x0102, '\ud83d');
        var pty = fixedView.Clone();
        pty.Session = "test-pty"; pty.Resizable = true;
        grid.Display(pty);
        intents.Clear();
        grid.MessageInput(0x0102, '\ude42');
        Pump(250);
        Check(grid.SelectedText() == "" && intents.Count == 1 && intents[0].TerminalResize is { Cols: >= 20, Rows: >= 5 },
              "Changed session retained previous input or failed to report its viewport");
        grid.Display(pty.Clone());
        Pump(160);
        Check(intents.Count == 1, "Unchanged viewport produced a second resize");

        var ended = pty.Clone(); ended.Ended = true; ended.CanPaste = false;
        grid.Display(ended); intents.Clear();
        grid.Key(Keys.Return); grid.MessageInput(0x0102, 'x'); grid.PasteText("text");
        form.Width += 40; Pump(180);
        Check(intents.Count == 0, "Ended terminal received input or resize");
        Console.WriteLine($"Passed styled grid, cursor, selection, Unicode input, paste and resize at {grid.DeviceDpi} DPI.");
    }
}

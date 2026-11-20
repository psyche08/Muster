using System.Buffers.Binary;
using System.Text;
using Muster.App;

namespace Muster.Windows;

internal partial class TerminalGrid : ScrollableControl
{
    const TextFormatFlags DrawFlags = TextFormatFlags.NoPadding | TextFormatFlags.NoPrefix | TextFormatFlags.SingleLine;
    readonly Dictionary<(float, FontStyle), Font> fonts = new();
    float pointSize = 11f;
    readonly System.Windows.Forms.Timer resizeTimer = new() { Interval = 100 };
    TerminalView? view;
    Size cell, reported;
    Point? anchor, head;
    public event Action<Intent>? Intent;
    public event Action<string>? ClipboardError;

    public TerminalGrid()
    {
        Name = "TerminalGrid";
        AccessibleName = "Terminal";
        AccessibleDescription = "Type to send terminal input. Ctrl+Shift+V pastes. Ctrl+Tab moves focus to other controls.";
        AccessibleRole = AccessibleRole.Text;
        BackColor = Color.Black;
        ForeColor = Color.White;
        TabStop = true;
        AutoScroll = true;
        SetStyle(ControlStyles.Selectable | ControlStyles.ResizeRedraw | ControlStyles.OptimizedDoubleBuffer |
                 ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint, true);
        resizeTimer.Tick += (_, _) => { resizeTimer.Stop(); ReportSize(); };
    }

    protected override AccessibleObject CreateAccessibilityInstance() => new GridAccessibility(this);
    sealed class GridAccessibility(TerminalGrid grid) : ControlAccessibleObject(grid)
    {
        public override string? Value
        {
            get => grid.view == null ? "" : string.Join(Environment.NewLine, grid.view.Lines.Select(l => l.Text));
            set { }
        }
    }

    public void Display(TerminalView? next)
    {
        bool changed = next?.DeviceId != view?.DeviceId || next?.Session != view?.Session;
        if (changed)
        {
            anchor = head = null;
            ResetKeyboard();
            reported = Size.Empty;
            AutoScrollPosition = Point.Empty;
        }
        if (Equals(next, view)) return;
        if (next?.Resizable != view?.Resizable) reported = Size.Empty;
        bool layoutChanged = changed || next?.Resizable != view?.Resizable || next?.Cols != view?.Cols || next?.Rows != view?.Rows || next?.Ended != view?.Ended;
        view = next;
        if (next == null) { resizeTimer.Stop(); return; }
        if (layoutChanged) LayoutGrid();
        Invalidate();
        if (changed && Visible) Focus();
    }

    Font GridFont(float size, FontStyle style)
    {
        if (!fonts.TryGetValue((size, style), out var font))
            fonts[(size, style)] = font = new Font("Consolas", size, style);
        return font;
    }

    void LayoutGrid()
    {
        if (view == null || !IsHandleCreated || ClientSize.Width == 0) return;
        using var graphics = CreateGraphics();
        Size Measure(float size) => TextRenderer.MeasureText(graphics, "M", GridFont(size, FontStyle.Regular), Size.Empty, DrawFlags);
        pointSize = 11f;
        cell = Measure(pointSize);
        while (!view.Resizable && pointSize > 8 && (long)cell.Width * view.Cols > ClientSize.Width)
        {
            pointSize -= 0.5f;
            cell = Measure(pointSize);
        }
        var extent = view.Resizable ? Size.Empty : new Size(
            (int)Math.Min(view.Cols, 65535) * cell.Width, (int)Math.Min(view.Rows, 65535) * cell.Height);
        if (AutoScrollMinSize != extent) AutoScrollMinSize = extent;
        if (view.Resizable && !view.Ended && Viewport != reported && !resizeTimer.Enabled) resizeTimer.Start();
    }

    Size Viewport => new(Math.Max(20, ClientSize.Width / Math.Max(1, cell.Width)),
                         Math.Max(5, ClientSize.Height / Math.Max(1, cell.Height)));

    void ReportSize()
    {
        if (view is not { Resizable: true, Ended: false } || !Visible || cell.IsEmpty || ClientSize.IsEmpty) return;
        Size size = Viewport;
        if (size == reported) return;
        reported = size;
        Intent?.Invoke(new() { TerminalResize = new() { Cols = (uint)size.Width, Rows = (uint)size.Height } });
    }

    protected override void OnHandleCreated(EventArgs e) { base.OnHandleCreated(e); LayoutGrid(); }
    protected override void OnSizeChanged(EventArgs e) { base.OnSizeChanged(e); LayoutGrid(); }
    protected override void OnDpiChangedAfterParent(EventArgs e) { base.OnDpiChangedAfterParent(e); LayoutGrid(); Invalidate(); }

    static readonly Color[] Palette = {
        Color.Black, Color.DarkRed, Color.Green, Color.Olive, Color.FromArgb(55,86,170), Color.DarkMagenta, Color.DarkCyan, Color.Silver,
        Color.Gray, Color.Red, Color.Lime, Color.Yellow, Color.DodgerBlue, Color.Magenta, Color.Cyan, Color.White
    };
    static Color DecodeColor(uint raw, Color fallback)
    {
        if ((raw >> 24) == 1) return Color.FromArgb((int)(raw >> 16) & 255, (int)(raw >> 8) & 255, (int)raw & 255);
        if ((raw >> 24) != 2) return fallback;
        int n = (int)(raw & 255);
        if (n < 16) return Palette[n];
        if (n >= 232) { int shade = 8 + (n - 232) * 10; return Color.FromArgb(shade, shade, shade); }
        static int Channel(int i) => i == 0 ? 0 : 55 + 40 * i;
        n -= 16;
        return Color.FromArgb(Channel(n / 36), Channel(n / 6 % 6), Channel(n % 6));
    }

    void DrawGlyph(Graphics graphics, Rune rune, Font font, Rectangle rect, Color color)
    {
        string text = rune.ToString();
        // System font fallback can choose a full-width glyph for a single projected cell.
        if (!rune.IsAscii)
        {
            int width = TextRenderer.MeasureText(graphics, text, font, Size.Empty, DrawFlags).Width;
            if (width > rect.Width)
            {
                float size = Math.Max(3f, MathF.Floor(2 * font.Size * rect.Width / width) / 2);
                font = GridFont(size, font.Style);
            }
        }
        TextRenderer.DrawText(graphics, text, font, rect, color, DrawFlags | TextFormatFlags.VerticalCenter);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        if (view == null || cell.IsEmpty) return;
        Point origin = AutoScrollPosition;
        int cols = (int)Math.Min(view.Cols, 65535);
        foreach (var line in view.Lines)
        {
            if (line.Index >= view.Rows || line.Index > 65535) continue;
            int y = origin.Y + (int)line.Index * cell.Height;
            if (y + cell.Height <= e.ClipRectangle.Top || y >= e.ClipRectangle.Bottom) continue;
            var text = line.Text.EnumerateRunes().GetEnumerator();
            var styles = line.Styles.Span;
            int remaining = 0;
            Color fg = ForeColor, bg = BackColor;
            Font font = GridFont(pointSize, FontStyle.Regular);
            for (int col = 0; col < cols; col++)
            {
                if (remaining == 0)
                {
                    int flags = 0;
                    fg = ForeColor; bg = BackColor;
                    while (styles.Length >= 11 && remaining == 0)
                    {
                        remaining = BinaryPrimitives.ReadUInt16BigEndian(styles);
                        fg = DecodeColor(BinaryPrimitives.ReadUInt32BigEndian(styles[2..]), ForeColor);
                        bg = DecodeColor(BinaryPrimitives.ReadUInt32BigEndian(styles[6..]), BackColor);
                        flags = styles[10];
                        styles = styles[11..];
                    }
                    if (remaining == 0) remaining = cols - col;
                    if ((flags & 8) != 0) (fg, bg) = (bg, fg);
                    if ((flags & 16) != 0) fg = Color.FromArgb((fg.R + bg.R) / 2, (fg.G + bg.G) / 2, (fg.B + bg.B) / 2);
                    FontStyle style = (FontStyle)(flags & 7);
                    if ((flags & 32) != 0) style |= FontStyle.Strikeout;
                    font = GridFont(pointSize, style);
                }
                remaining--;
                bool hasText = text.MoveNext();
                var rect = new Rectangle(origin.X + col * cell.Width, y, cell.Width, cell.Height);
                if (rect.Left >= e.ClipRectangle.Right) break;
                if (rect.Right <= e.ClipRectangle.Left) continue;
                bool selected = Selected(col, (int)line.Index);
                Color background = selected ? SystemColors.Highlight : bg;
                if (background.ToArgb() != BackColor.ToArgb())
                {
                    using var brush = new SolidBrush(background);
                    e.Graphics.FillRectangle(brush, rect);
                }
                if (hasText && (text.Current.Value != ' ' || font.Underline || font.Strikeout))
                    DrawGlyph(e.Graphics, text.Current, font, rect, selected ? SystemColors.HighlightText : fg);
            }
        }
        if (!view.Ended && view.CursorX < view.Cols && view.CursorY < view.Rows &&
            view.CursorX <= 65535 && view.CursorY <= 65535)
        {
            var rect = new Rectangle(origin.X + (int)view.CursorX * cell.Width, origin.Y + (int)view.CursorY * cell.Height,
                                     cell.Width - 1, cell.Height - 1);
            e.Graphics.DrawRectangle(Pens.Silver, rect);
        }
    }

    static int Compare(Point a, Point b) => a.Y == b.Y ? a.X.CompareTo(b.X) : a.Y.CompareTo(b.Y);
    bool Selected(int col, int row)
    {
        if (anchor is not {} a || head is not {} b || a == b) return false;
        if (Compare(a,b) > 0) (a,b) = (b,a);
        var p = new Point(col, row);
        return Compare(a,p) <= 0 && Compare(p,b) <= 0;
    }
    Point CellAt(Point p) => new(Math.Clamp((p.X - AutoScrollPosition.X) / Math.Max(1, cell.Width), 0, Math.Max(0, (int)Math.Min(view!.Cols, 65535) - 1)),
                                 Math.Clamp((p.Y - AutoScrollPosition.Y) / Math.Max(1, cell.Height), 0, Math.Max(0, (int)Math.Min(view.Rows, 65535) - 1)));

    protected override void OnMouseDown(MouseEventArgs e)
    {
        base.OnMouseDown(e);
        if (view != null && e.Button == MouseButtons.Left) { Focus(); Capture = true; head = anchor = CellAt(e.Location); Invalidate(); }
    }
    protected override void OnMouseMove(MouseEventArgs e)
    {
        base.OnMouseMove(e);
        if (view != null && Capture && e.Button == MouseButtons.Left) { head = CellAt(e.Location); Invalidate(); }
    }
    protected override void OnMouseUp(MouseEventArgs e)
    {
        base.OnMouseUp(e);
        if (e.Button == MouseButtons.Left) { Capture = false; if (view != null && anchor != null) head = CellAt(e.Location); Invalidate(); }
    }
    internal string SelectedText()
    {
        if (anchor is not {} a || head is not {} b || a == b || view == null) return "";
        if (Compare(a,b) > 0) (a,b) = (b,a);
        var lines = new string[b.Y - a.Y + 1];
        foreach (var line in view.Lines.Where(l => l.Index >= a.Y && l.Index <= b.Y))
        {
            int first = line.Index == a.Y ? a.X : 0;
            int count = line.Index == b.Y ? b.X - first + 1 : (int)Math.Min(view.Cols, 65535) - first;
            lines[(int)line.Index - a.Y] = string.Concat(line.Text.EnumerateRunes().Skip(first).Take(count)).TrimEnd(' ');
        }
        return string.Join(Environment.NewLine, lines);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) { resizeTimer.Dispose(); foreach (var font in fonts.Values) font.Dispose(); }
        base.Dispose(disposing);
    }
}

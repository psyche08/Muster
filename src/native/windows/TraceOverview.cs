using Muster.App;

namespace Muster.Windows;

internal sealed class TraceOverview : Control
{
    internal record struct Bar(string Id, int Lane, double Start, double Width, bool Turn);
    Bar[] bars = [];
    string? selected;
    bool noTiming;
    public event Action<string>? RecordSelected;

    public TraceOverview()
    {
        DoubleBuffered = true;
        AccessibleName = "Trace overview";
        SetStyle(ControlStyles.ResizeRedraw, true);
    }

    protected override void OnFontChanged(EventArgs e)
    {
        base.OnFontChanged(e);
        Height = (Font.Height + 4) * 3;
    }

    public void Display(IEnumerable<TraceSpan>? spans, bool timed)
    {
        var records = (spans ?? []).Where(s => !timed || s.StartedUnix > 0).ToArray();
        noTiming = timed && records.Length == 0;
        bars = LayoutBars(records, timed);
        Height = (Font.Height + 4) * 3;
        Invalidate();
    }

    internal static Bar[] LayoutBars(TraceSpan[] records, bool timed)
    {
        if (records.Length == 0) return [];
        decimal first = records.Min(s => s.StartedUnix);
        decimal last = records.Max(s => (decimal)s.StartedUnix + (s.DurationKnown ? s.DurationSeconds : 0));
        return records.Select((s, i) =>
        {
            bool byTime = timed && last > first;
            double start = byTime ? (double)((s.StartedUnix - first) / (last - first)) : (double)i / records.Length;
            double width = byTime ? (double)((s.DurationKnown ? s.DurationSeconds : 0) / (last - first)) : 1d / records.Length;
            return new Bar(s.Id, s.Kind switch { "reasoning" => 1, "tool" => 2, _ => 0 }, start, width,
                s.Kind == "turn" && s.Title == "User message");
        }).ToArray();
    }

    public void SelectRecord(string? id)
    {
        if (selected == id) return;
        selected = id;
        Invalidate();
    }

    Rectangle BoundsOf(Bar bar)
    {
        int left = LogicalToDeviceUnits(50);
        int width = Math.Max(1, ClientSize.Width - left - 2);
        int lane = Math.Max(1, Height / 3);
        int x = left + Math.Min(width - 1, (int)(width * bar.Start));
        return new Rectangle(x, lane * bar.Lane + 3, Math.Max(2, (int)(width * bar.Width) - 1), Math.Max(2, lane - 6));
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        string[] labels = ["Turn", "Think", "Tools"];
        for (int i = 0; i < labels.Length; i++)
            TextRenderer.DrawText(e.Graphics, labels[i], Font, new Point(0, Height * i / 3), ForeColor);
        if (noTiming)
            TextRenderer.DrawText(e.Graphics, "No recorded times", Font, new Point(LogicalToDeviceUnits(50), Height / 3), ForeColor);
        foreach (var bar in bars)
        {
            var rect = BoundsOf(bar);
            if (bar.Turn) e.Graphics.DrawLine(SystemPens.ControlDark, rect.Left, 0, rect.Left, Height);
            e.Graphics.FillRectangle(bar.Lane switch { 1 => Brushes.SlateBlue, 2 => Brushes.DarkGoldenrod, _ => Brushes.SteelBlue }, rect);
            if (bar.Id == selected) e.Graphics.DrawRectangle(SystemPens.WindowText, rect);
        }
    }

    protected override void OnMouseClick(MouseEventArgs e)
    {
        base.OnMouseClick(e);
        if (e.Button != MouseButtons.Left) return;
        for (int i = bars.Length - 1; i >= 0; i--)
            if (BoundsOf(bars[i]).Contains(e.Location))
            {
                RecordSelected?.Invoke(bars[i].Id);
                return;
            }
    }
}

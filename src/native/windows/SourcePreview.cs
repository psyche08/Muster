using System.Globalization;
using System.Text;
using Muster.App;

namespace Muster.Windows;

internal sealed class SourcePreview : UserControl
{
    static readonly UTF8Encoding Utf8 = new(false, true);
    readonly Label heading = new() { Dock = DockStyle.Top, AutoSize = true, Padding = new Padding(4) };
    readonly Panel gutter = new() { Dock = DockStyle.Left, BackColor = SystemColors.Control };
    readonly RichTextBox source = new() { Dock = DockStyle.Fill, ReadOnly = true, WordWrap = false,
        BorderStyle = BorderStyle.None, DetectUrls = false, BackColor = SystemColors.Window,
        ScrollBars = RichTextBoxScrollBars.Both, AccessibleName = "Preview contents" };
    readonly Font mono = new(FontFamily.GenericMonospace, 10);
    FileView? lastFile;
    GitView? lastGit;

    public SourcePreview()
    {
        source.Font = mono;
        Controls.Add(source);
        Controls.Add(gutter);
        Controls.Add(heading);
        source.VScroll += (_, _) => gutter.Invalidate();
        source.Resize += (_, _) => gutter.Invalidate();
        gutter.Paint += (_, e) =>
        {
            int start = source.GetLineFromCharIndex(source.GetCharIndexFromPosition(Point.Empty));
            for (int line = start; ; line++)
            {
                int index = source.GetFirstCharIndexFromLine(line);
                if (index < 0) break;
                int y = source.GetPositionFromCharIndex(index).Y;
                if (y >= source.Height) break;
                TextRenderer.DrawText(e.Graphics, (line + 1).ToString(), source.Font,
                    new Rectangle(0, y, gutter.Width - 2, source.Font.Height + 4), SystemColors.GrayText,
                    TextFormatFlags.Right | TextFormatFlags.NoPadding | TextFormatFlags.NoPrefix);
            }
        };
    }

    protected override void Dispose(bool disposing)
    {
        base.Dispose(disposing);
        if (disposing) mono.Dispose();
    }

    public void ShowFile(FileView file)
    {
        if (file.Equals(lastFile)) return;
        lastFile = file;
        lastGit = null;
        string caption = file.IsDiff ? "Recorded turn changes" : "Current file contents";
        if (file.Truncated) caption += " · Preview truncated";
        string text;
        bool content = false;
        if (file.Loading) text = "Loading file…";
        else if (file.Error.Length != 0) text = file.Error;
        else if (file.Content.IsEmpty) text = file.IsDiff ? "No recorded changes." : "Empty file.";
        else
        {
            try
            {
                text = Utf8.GetString(file.Content.Span);
                content = !text.Contains('\0');
            }
            catch (DecoderFallbackException) { text = ""; }
            if (!content) text = $"Binary file · {file.Content.Length:N0} bytes in preview";
        }
        Display(caption, text, content && file.IsDiff, content && !file.IsDiff);
    }

    public void ShowCommit(GitView git)
    {
        if (git.Equals(lastGit)) return;
        lastGit = git;
        lastFile = null;
        string text = git.Loading ? "Loading commit…" : git.Diff.Length == 0 ? "No diff to display." : git.Diff;
        Display("Commit " + git.DiffCommit, text, !git.Loading && git.Diff.Length != 0, false);
    }

    void Display(string caption, string text, bool diff, bool numbered)
    {
        heading.Text = caption;
        text = text.ReplaceLineEndings("\n");
        if (diff && !SystemInformation.HighContrast) source.Rtf = DiffRtf(text);
        else
        {
            source.Clear();
            source.SelectionColor = SystemColors.WindowText;
            source.SelectionFont = mono;
            source.Text = text;
        }
        gutter.Visible = numbered;
        if (numbered)
        {
            int count = source.GetLineFromCharIndex(source.TextLength) + 1;
            gutter.Width = TextRenderer.MeasureText(count.ToString(), source.Font).Width + 8;
            gutter.Invalidate();
        }
        source.Select(0, 0);
        source.ScrollToCaret();
    }

    static string DiffRtf(string text)
    {
        var rtf = new StringBuilder(@"{\rtf1\ansi\deff0{\fonttbl{\f0\fmodern Consolas;}}{\colortbl;\red0\green110\blue53;\red170\green35\blue35;\red30\green80\blue160;}\f0\fs20 ");
        var lines = text.AsSpan().EnumerateLines();
        bool first = true;
        foreach (var line in lines)
        {
            if (!first) rtf.Append(@"\par ");
            first = false;
            int color = line.StartsWith("@@") ? 3 : line.StartsWith("+") && !line.StartsWith("+++") ? 1
                : line.StartsWith("-") && !line.StartsWith("---") ? 2 : 0;
            rtf.Append(@"\cf").Append(color).Append(' ');
            foreach (char c in line)
            {
                if (c == '\t') rtf.Append(@"\tab ");
                else if (c is '\\' or '{' or '}') rtf.Append('\\').Append(c);
                else if (c is >= ' ' and <= '~') rtf.Append(c);
                else rtf.Append(@"\u").Append(unchecked((short)c).ToString(CultureInfo.InvariantCulture)).Append('?');
            }
        }
        return rtf.Append('}').ToString();
    }
}

using Google.Protobuf;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using Windows.Graphics.Imaging;

namespace Muster.Windows;

internal sealed record TranscriptImage(int Line, ByteString Bytes);

internal sealed class HistoryTranscript : RichTextBox
{
    sealed record Picture(Size Size, string Hex);
    readonly Dictionary<ByteString, Picture?> cache = new();
    List<string> lines = [];
    List<TranscriptImage> images = [];
    HashSet<ByteString> wanted = [];
    (string Device, string Session)? scope;
    // A single neutral pixel reserves the layout of offscreen image occurrences.
    // Only pictures around the viewport enter the native rich text document as full DIBs.
    const string Placeholder = "28000000010000000100000001001800000000000400000000000000000000000000000000000000EEEEEE00";
    readonly Dictionary<ByteString, Size> sizes = new();
    readonly List<int> offsets = [];
    int firstPicture, endPicture;
    bool loading, updating, scheduled;
    int layoutWidth;

    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint ScrollMessage(nint window, uint message, nint unused, ref Point position);

    public HistoryTranscript()
    {
        ReadOnly = true;
        DetectUrls = false;
        AccessibleName = "Conversation history";
        BackColor = SystemColors.Window;
        SelectionChanged += (_, _) => ScheduleViewport();
        VScroll += (_, _) => ScheduleViewport();
    }

    public void Display((string Device, string Session)? origin, List<string> text, List<TranscriptImage> pictures)
    {
        if (pictures.Count == 0 && images.Count == 0) return;
        bool same = origin == scope;
        if (same && images.SequenceEqual(pictures) && lines.SequenceEqual(text)) return;
        scope = origin;
        images = pictures;
        lines = pictures.Count == 0 ? [] : text;
        if (!same || pictures.Count == 0) { sizes.Clear(); cache.Clear(); }
        else if (sizes.Count != 0)
        {
            var present = pictures.Select(p => p.Bytes).ToHashSet();
            foreach (var key in sizes.Keys.Where(k => !present.Contains(k)).ToArray()) sizes.Remove(key);
        }
        firstPicture = endPicture = 0;
        wanted.Clear();
        RenderDocument(same);
        UpdateViewport();
    }

    protected override void WndProc(ref Message m)
    {
        base.WndProc(ref m);
        // Painting, scrollbar, wheel, line scroll and scroll-to-caret can move
        // the visible document without changing the projected conversation.
        if (m.Msg is 0x000F or 0x0115 or 0x020A or 0x00B6 or 0x00B7) ScheduleViewport();
    }

    void ScheduleViewport()
    {
        if (scheduled || updating || !IsHandleCreated || IsDisposed || images is not { Count: > 0 }) return;
        scheduled = true;
        BeginInvoke(new Action(() => { scheduled = false; UpdateViewport(); }));
    }

    void UpdateViewport()
    {
        if (updating || IsDisposed || offsets.Count == 0 || !IsHandleCreated) return;
        int top = GetCharIndexFromPosition(new Point(6, 4));
        int bottom = GetCharIndexFromPosition(new Point(Math.Max(6, ClientSize.Width - 8), Math.Max(4, ClientSize.Height - 6)));
        int start = offsets.FindIndex(offset => offset >= top);
        if (start < 0) start = offsets.Count;
        int end = offsets.FindIndex(start, offset => offset > bottom);
        if (end < 0) end = offsets.Count;
        start = Math.Max(0, start - 1);
        end = Math.Min(offsets.Count, end + 1);
        if (start == firstPicture && end == endPicture) return;
        firstPicture = start;
        endPicture = end;
        wanted = images.GetRange(start, end - start).Select(p => p.Bytes).ToHashSet();
        foreach (var key in cache.Keys.Where(k => !wanted.Contains(k)).ToArray()) cache.Remove(key);
        RenderDocument(true);
        StartLoading();
    }

    void StartLoading()
    {
        if (loading) return;
        var missing = wanted.Where(bytes => !cache.ContainsKey(bytes) &&
            (!sizes.TryGetValue(bytes, out var size) || !size.IsEmpty)).ToArray();
        if (missing.Length == 0) return;
        loading = true;
        _ = LoadPictures(missing);
    }

    async Task LoadPictures(ByteString[] batch)
    {
        var results = new Dictionary<ByteString, Picture?>();
        foreach (var bytes in batch)
        {
            if (bytes.Length < 2 || bytes[0] != 0xff || bytes[1] != 0xd8)
            {
                results.Add(bytes, null);
                continue;
            }
            Picture? picture = null;
            try
            {
                using var bitmap = await AttachmentPreview.Decode(bytes, 480, BitmapDecoder.JpegDecoderId).ConfigureAwait(false);
                using var buffer = new MemoryStream();
                bitmap.Save(buffer, ImageFormat.Bmp);
                // A DIB begins after the 14-byte BMP file header.
                picture = new(bitmap.Size, Convert.ToHexString(buffer.GetBuffer().AsSpan(14, (int)buffer.Length - 14)));
            }
            catch (Exception ex) when (ex is ArgumentException or IOException or COMException or NotSupportedException or InvalidOperationException or FormatException) { }
            results.Add(bytes, picture);
        }

        try
        {
            await InvokeAsync(() =>
            {
                loading = false;
                bool changed = false;
                foreach (var (bytes, picture) in results)
                {
                    if (!wanted.Contains(bytes)) continue;
                    cache[bytes] = picture;
                    sizes[bytes] = picture?.Size ?? Size.Empty;
                    changed = true;
                }
                if (changed) RenderDocument(true);
                UpdateViewport();
                StartLoading();
            });
        }
        catch (InvalidOperationException) when (IsDisposed || !IsHandleCreated) { }
    }

    int AvailableWidth => Math.Clamp((ClientSize.Width - SystemInformation.VerticalScrollBarWidth - 8) * 96 / DeviceDpi, 1, 320);

    protected override void OnResize(EventArgs e)
    {
        base.OnResize(e);
        if (!updating && images is { Count: > 0 } && IsHandleCreated)
        {
            if (AvailableWidth != layoutWidth) RenderDocument(true);
            ScheduleViewport();
        }
    }

    void RenderDocument(bool preservePosition)
    {
        offsets.Clear();
        if (images.Count == 0) { Clear(); return; }
        layoutWidth = AvailableWidth;
        var rtf = new StringBuilder(@"{\rtf1\ansi\deff0{\fonttbl{\f0\fnil Segoe UI;}}\f0\fs")
            .Append((int)Math.Round(Font.SizeInPoints * 2)).Append(' ');
        bool first = true;
        int offset = 0;
        void Paragraph()
        {
            if (!first) { rtf.Append(@"\par "); offset++; }
            first = false;
        }
        int next = 0;
        for (int line = 0; line <= lines.Count; line++)
        {
            while (next < images.Count && images[next].Line == line)
            {
                Paragraph();
                int index = next++;
                var bytes = images[index].Bytes;
                offsets.Add(offset);
                var size = sizes.GetValueOrDefault(bytes, new Size(240, 120));
                if (size.IsEmpty)
                {
                    const string unavailable = "Image preview unavailable";
                    rtf.Append(unavailable);
                    offset += unavailable.Length;
                    continue;
                }
                var picture = index >= firstPicture && index < endPicture ? cache.GetValueOrDefault(bytes) : null;
                var pixels = picture?.Size ?? new Size(1, 1);
                double scale = Math.Min(1d, Math.Min((double)layoutWidth / size.Width, 260d / size.Height));
                rtf.Append(@"{\pict\dibitmap0\picw").Append(pixels.Width).Append(@"\pich").Append(pixels.Height)
                    .Append(@"\picwgoal").Append((int)Math.Max(1, Math.Round(size.Width * scale * 15)))
                    .Append(@"\pichgoal").Append((int)Math.Max(1, Math.Round(size.Height * scale * 15)))
                    .Append(' ').Append(picture?.Hex ?? Placeholder).Append('}');
                offset++; // RichEdit counts an embedded object as one character.
            }
            if (line == lines.Count || lines[line].Length == 0) continue;
            foreach (var part in lines[line].AsSpan().EnumerateLines())
            {
                Paragraph();
                RichText.AppendLiteral(rtf, part);
                offset += part.Length;
            }
        }
        int anchor = preservePosition ? GetCharIndexFromPosition(new Point(6, 4)) : 0;
        int anchorY = preservePosition ? GetPositionFromCharIndex(anchor).Y : 0;
        var position = Point.Empty;
        int start = preservePosition ? SelectionStart : 0;
        int length = preservePosition ? SelectionLength : 0;
        if (preservePosition) ScrollMessage(Handle, 0x04DD, 0, ref position); // EM_GETSCROLLPOS
        updating = true;
        try
        {
            Rtf = rtf.Append('}').ToString();
            start = Math.Min(start, TextLength);
            Select(start, Math.Min(length, TextLength - start));
            if (preservePosition)
            {
                anchor = Math.Min(anchor, Math.Max(0, TextLength - 1));
                var after = Point.Empty;
                ScrollMessage(Handle, 0x04DD, 0, ref after);
                position.Y = Math.Max(0, after.Y + GetPositionFromCharIndex(anchor).Y - anchorY);
            }
            ScrollMessage(Handle, 0x04DE, 0, ref position);
        }
        finally { updating = false; }
    }
}

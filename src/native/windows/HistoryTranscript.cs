using Google.Protobuf;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using Windows.Graphics.Imaging;

namespace Muster.Windows;

internal sealed record TranscriptImage(int Line, ByteString Bytes);

internal sealed class HistoryTranscript : NamedRichTextBox
{
    sealed record Picture(Size Size, string Hex);
    readonly Dictionary<ByteString, Picture?> cache = new();
    List<string> lines = [];
    List<TranscriptBlock> blocks = [];
    public List<BlockOffset> BlockOffsets { get; private set; } = [];
    bool latestPage;
    List<TranscriptLink> links = [];
    readonly List<TranscriptLink> nativeLinks = [];
    public event Action<string>? ActivateLink;

    // CHARFORMATW through dwEffects. RichEdit requires the full native structure size.
    [StructLayout(LayoutKind.Sequential, Size = 92)]
    struct CharFormat { public uint Size, Mask, Effects; }

    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint FormatSelection(nint hwnd, uint message, nint selection, ref CharFormat format);

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

    public HistoryTranscript() : base("Conversation history")
    {
        ReadOnly = true;
        DetectUrls = false;
        LinkClicked += (_, e) =>
        {
            var link = nativeLinks.Find(l => l.Start == e.LinkStart && l.Length == e.LinkLength);
            if (link != null) ActivateLink?.Invoke(link.Destination);
        };
        KeyDown += (_, e) =>
        {
            if (e.KeyData != Keys.Enter) return;
            var link = nativeLinks.Find(l => SelectionStart >= l.Start && SelectionStart < l.Start + l.Length
                && SelectionStart + SelectionLength <= l.Start + l.Length);
            if (link == null) return;
            e.SuppressKeyPress = true;
            ActivateLink?.Invoke(link.Destination);
        };
        BackColor = SystemColors.Window;
        SelectionChanged += (_, _) => ScheduleViewport();
        VScroll += (_, _) => ScheduleViewport();
    }

    public void Display((string Device, string Session)? origin, List<string> text, List<TranscriptImage> pictures,
        List<TranscriptBlock>? projected = null, bool latest = false, List<TranscriptLink>? projectedLinks = null)
    {
        latestPage = latest;
        projected ??= [];
        projectedLinks ??= [];
        if (pictures.Count == 0 && images.Count == 0 && links.Count == 0 && projectedLinks.Count == 0) return;
        bool same = origin == scope;
        if (same && images.SequenceEqual(pictures) && lines.SequenceEqual(text) && blocks.SequenceEqual(projected)
            && links.SequenceEqual(projectedLinks)) return;
        scope = origin;
        blocks = projected;
        images = pictures;
        links = projectedLinks;
        lines = pictures.Count == 0 && links.Count == 0 ? [] : text;
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
        var viewport = preservePosition && IsHandleCreated ? new TranscriptViewport(this, BlockOffsets) : null;
        offsets.Clear();
        BlockOffsets = [];
        nativeLinks.Clear();
        if (images.Count == 0 && links.Count == 0) { Clear(); return; }
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
        var textOffsets = new int[lines.Count + 1];
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
            textOffsets[line] = first ? 0 : offset + 1;
            if (line == lines.Count || lines[line].Length == 0) continue;
            foreach (var part in lines[line].AsSpan().EnumerateLines())
            {
                Paragraph();
                RichText.AppendLiteral(rtf, part);
                offset += part.Length;
            }
        }
        foreach (var block in blocks)
        {
            int start = block.Picture < images.Count && images[block.Picture].Line == block.Line
                ? offsets[block.Picture] : textOffsets[block.Line];
            BlockOffsets.Add(new(block.Id, Math.Min(start, offset)));
        }
        updating = true;
        try
        {
            Rtf = rtf.Append('}').ToString();
            var format = new CharFormat { Size = 92, Mask = 0x20, Effects = 0x20 }; // CFM_LINK, CFE_LINK
            foreach (var link in links)
            {
                int start = textOffsets[link.Line] + link.Start;
                Select(start, link.Length);
                if (!SelectedText.AsSpan().SequenceEqual(lines[link.Line].AsSpan(link.Start, link.Length))) continue;
                FormatSelection(Handle, 0x0444, 1, ref format); // EM_SETCHARFORMAT, SCF_SELECTION
                nativeLinks.Add(link with { Start = start });
            }
            if (links.Count > 0) Select(0, 0);
            viewport?.Restore(this, BlockOffsets, latestPage);
        }
        finally { updating = false; }
    }
}

using Google.Protobuf;
using Microsoft.Win32;
using Muster.App;
using Muster.V1;
using System.Drawing.Imaging;

namespace Muster.Windows;

internal class ChatInput : TextBox
{
    readonly List<AgentAttachment> attachments = new();
    public IReadOnlyList<AgentAttachment> Attachments => attachments;
    public bool CanSubmit { get; private set; }
    public bool CanImport => CanSubmit && countLimit > 0;
    public bool HasPayload => attachments.Count != 0 || !string.IsNullOrWhiteSpace(Text);
    public bool FitsAttachmentLimits => attachments.Count <= countLimit && RemainingBytes >= 0;
    public bool CanSubmitPayload => CanSubmit && HasPayload && FitsAttachmentLimits;
    public string? DraftKey => key;
    public event Action? ReturnPressed;
    uint countLimit;
    ulong byteLimit;
    bool composing;
    string? key;
    string revision = "";
    bool rendering;
    (string Id, string Revision)? submitted;

    protected override void WndProc(ref Message message)
    {
        const int wmImeStartComposition = 0x010D, wmImeEndComposition = 0x010E, wmKillFocus = 0x0008;
        if (message.Msg == wmImeStartComposition) composing = true;
        if (message.Msg == wmImeEndComposition || message.Msg == wmKillFocus) composing = false;
        base.WndProc(ref message);
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (key != null && e.KeyData == Keys.Return && !composing)
        {
            e.SuppressKeyPress = true;
            if (CanSubmitPayload) ReturnPressed?.Invoke();
            return;
        }
        base.OnKeyDown(e);
    }

    protected override void OnTextChanged(EventArgs e)
    {
        if (!rendering && key != null)
            revision = Guid.NewGuid().ToString();
        base.OnTextChanged(e);
    }

    void DisplayText(string text)
    {
        if (Text == text)
            return;
        rendering = true;
        try { Text = text; }
        finally { rendering = false; }
    }

    public void ShowDraft(ChatView? chat)
    {
        var draft = chat?.Draft;
        CanSubmit = chat is { Draft: not null, Sending: false, Completing: false, ComposerDisabledReason.Length: 0 };
        countLimit = chat?.AttachmentCountLimit ?? 0;
        byteLimit = chat?.AttachmentBytesLimit ?? 0;
        if (key != draft?.Key)
        {
            attachments.Clear();
            key = draft?.Key;
            revision = string.IsNullOrEmpty(draft?.Revision) ? Guid.NewGuid().ToString() : draft.Revision;
            submitted = null;
            DisplayText(draft?.Text ?? "");
        }
        else if (draft != null && draft.Revision == revision && draft.Text.Length == 0)
        {
            // A receipt may have arrived while this conversation was closed.
            DisplayText("");
        }
        if (submitted is {} pending && pending.Id == chat?.SentDraftId)
        {
            if (pending.Revision == revision)
            {
                attachments.Clear();
                DisplayText("");
            }
            submitted = null;
        }
    }

    public Intent WithDraft(Intent action) => key == null ? action : new() {
        ComposerAction = new() {
            Action = action,
            Draft = new() { Key = key, Text = Text, Revision = revision }
        }
    };

    long RemainingBytes => checked((long)byteLimit) - attachments.Sum(a => (long)a.Data.Length);

    int ImportCapacity()
    {
        if (!CanImport || attachments.Count >= countLimit)
            throw new InvalidOperationException($"This conversation allows up to {countLimit} attachments.");
        return (int)Math.Clamp(RemainingBytes, 0, int.MaxValue);
    }

    void Add(string name, string mediaType, ByteString data)
    {
        attachments.Add(new AgentAttachment { Name = name, MediaType = mediaType, Data = data });
        revision = Guid.NewGuid().ToString();
    }

    public static bool CanImportData(IDataObject data) =>
        data.GetDataPresent(DataFormats.FileDrop) || data.GetDataPresent(DataFormats.Bitmap);

    public void ImportData(IDataObject data)
    {
        if (data.GetData(DataFormats.FileDrop) is string[] files)
        {
            foreach (var file in files) AddFile(file);
        }
        else if (data.GetData(DataFormats.Bitmap) is Image image)
            AddImage(image);
    }

    public void AddFile(string path)
    {
        int capacity = ImportCapacity();
        using var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
        if (file.Length > capacity)
            throw new InvalidOperationException($"The selected files exceed the {byteLimit:N0}-byte attachment limit.");
        var bytes = new byte[(int)file.Length];
        file.ReadExactly(bytes);
        var extension = Path.GetExtension(path);
        using var type = extension.Length > 0 ? Registry.ClassesRoot.OpenSubKey(extension) : null;
        Add(Path.GetFileName(path), type?.GetValue("Content Type") as string ?? "application/octet-stream",
            UnsafeByteOperations.UnsafeWrap(bytes));
    }

    // GDI+ can swallow an IStream write failure and return success for a partial PNG.
    sealed class PngBuffer(int limit) : MemoryStream(new byte[limit], 0, limit, true, true)
    {
        public bool Exceeded { get; private set; }
        void CheckEnd(long end)
        {
            if (end <= Capacity) return;
            Exceeded = true;
            throw new IOException("The PNG exceeds the attachment budget.");
        }
        public override void SetLength(long length) { CheckEnd(length); base.SetLength(length); }
        public override void Write(byte[] buffer, int offset, int count)
        {
            CheckEnd(Position + count);
            base.Write(buffer, offset, count);
        }
        public override void Write(ReadOnlySpan<byte> buffer)
        {
            CheckEnd(Position + buffer.Length);
            base.Write(buffer);
        }
        public override void WriteByte(byte value) { CheckEnd(Position + 1); base.WriteByte(value); }
    }

    public void AddImage(Image image)
    {
        int capacity = ImportCapacity();
        using var output = new PngBuffer(capacity);
        output.SetLength(0);
        try { image.Save(output, ImageFormat.Png); }
        catch (Exception ex) when (output.Exceeded || ex is NotSupportedException or System.Runtime.InteropServices.ExternalException)
        {
            throw new InvalidOperationException($"Could not encode this image within the remaining {capacity:N0} bytes.", ex);
        }
        if (output.Exceeded || output.Length == 0)
            throw new InvalidOperationException($"Could not encode this image within the remaining {capacity:N0} bytes.");
        Add("Pasted image.png", "image/png", ByteString.CopyFrom(output.GetBuffer(), 0, checked((int)output.Length)));
    }

    public void RemoveAttachment(AgentAttachment attachment)
    {
        if (attachments.Remove(attachment))
            revision = Guid.NewGuid().ToString();
    }

    public Intent? Submission(string releaseToken)
    {
        if (!CanSubmitPayload)
            return null;
        if (submitted?.Revision != revision || releaseToken.Length > 0)
            submitted = (Guid.NewGuid().ToString(), revision);
        return new() { AgentSend = new() {
            Text = Text, DraftId = submitted.Value.Id, ReleaseUnconfirmed = releaseToken, Attachments = { attachments }
        } };
    }
}

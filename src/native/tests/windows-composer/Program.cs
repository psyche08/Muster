using Muster.App;
using Muster.Windows;

internal static class Program
{
    static void Check(bool ok, string failure)
    {
        if (!ok) throw new InvalidOperationException(failure);
    }

    sealed class NativeInput : ChatInput
    {
        public bool KeyDownHandled(Keys keys)
        {
            var args = new KeyEventArgs(keys);
            OnKeyDown(args);
            return args.SuppressKeyPress;
        }

        public void WindowMessage(int id, int key = 0)
        {
            var message = Message.Create(Handle, id, key, 0);
            WndProc(ref message);
        }
    }

    static void Keyboard()
    {
        using var input = new NativeInput { Multiline = true, AcceptsReturn = true };
        input.CreateControl();
        var chat = new ChatView { Draft = new() { Key = "keyboard" } };
        input.ShowDraft(chat);
        input.Text = "draft!";
        int sends = 0;
        input.ReturnPressed += () => sends++;
        input.WindowMessage(0x0100, (int)Keys.Return); // WM_KEYDOWN through the native edit control
        Check(sends == 1 && input.Text == "draft!", "Return did not request exactly one submission");
        input.Select(5, 1);
        Check(!input.KeyDownHandled(Keys.Shift | Keys.Return), "Shift+Return intercepted");
        input.WindowMessage(0x0102, '\r'); // WM_CHAR produced by TranslateMessage
        Check(input.Text == "draft\r\n" && sends == 1, "Native newline did not replace selection");
        input.WindowMessage(0x010D); // WM_IME_STARTCOMPOSITION
        Check(!input.KeyDownHandled(Keys.Enter) && sends == 1, "IME Return submitted the draft");
        input.WindowMessage(0x010E); // WM_IME_ENDCOMPOSITION
        Check(!input.KeyDownHandled(Keys.ProcessKey) && !input.KeyDownHandled(Keys.Alt | Keys.Return), "Other keys intercepted");
        chat.Sending = true;
        input.ShowDraft(chat);
        Check(input.KeyDownHandled(Keys.Return) && sends == 1, "A busy composer submitted");
        input.ShowDraft(null);
        Check(!input.KeyDownHandled(Keys.Return) && sends == 1, "Non-chat input captured Return");
    }

    static void Attachments()
    {
        using var input = new ChatInput();
        var chat = new ChatView { Draft = new() { Key = "files" }, AttachmentBytesLimit = 1000, AttachmentCountLimit = 2 };
        input.ShowDraft(chat);
        string file = Path.GetTempFileName();
        try
        {
            File.WriteAllBytes(file, [1, 2, 3]);
            var drop = new DataObject(DataFormats.FileDrop, new[] { file });
            Check(ChatInput.CanImportData(drop), "Windows file drop was not accepted");
            input.ImportData(drop);
            var sent = input.Submission("")!.AgentSend;
            var item = sent.Attachments.Single();
            Check(sent.Text == "" && item.Name == Path.GetFileName(file) && item.Data.Span.SequenceEqual(new byte[] { 1, 2, 3 }), "File-only payload");
            using var image = new Bitmap(2, 2);
            var bitmap = new DataObject(DataFormats.Bitmap, image);
            Check(ChatInput.CanImportData(bitmap), "Windows clipboard bitmap was not accepted");
            input.ImportData(bitmap);
            Check(input.Attachments[1].MediaType == "image/png" && input.Attachments[1].Data.Span[0] == 0x89, "Clipboard image encoding");
            var encoded = input.Attachments[1].Data;
            using var preview = Task.Run(() => AttachmentPreview.Decode(encoded, 1)).GetAwaiter().GetResult();
            Check(preview.Width == 1 && preview.Height == 1, "Windows image preview was not scaled");
            Check(sent.Attachments.Count == 1, "Editing mutated a submitted payload");
            try { input.AddFile(file); throw new Exception("Count limit ignored"); }
            catch (InvalidOperationException) { }
            chat.SentDraftId = sent.DraftId;
            input.ShowDraft(chat);
            Check(input.Attachments.Count == 2, "Late receipt cleared an edited attachment list");
            input.RemoveAttachment(item);
            var imageOnly = input.Submission("token")!.AgentSend;
            Check(imageOnly.Attachments.Count == 1 && imageOnly.ReleaseUnconfirmed == "token", "Attachment retry");
            chat.SentDraftId = imageOnly.DraftId;
            input.ShowDraft(chat);
            Check(input.Attachments.Count == 0 && !input.HasPayload, "Matching receipt retained attachments");
            chat.AttachmentBytesLimit = 2;
            input.ShowDraft(chat);
            try { input.AddFile(file); throw new Exception("Byte limit ignored"); }
            catch (InvalidOperationException) { }
            try { input.AddImage(image); throw new Exception("Oversized clipboard image accepted"); }
            catch (InvalidOperationException) { }
            chat.AttachmentBytesLimit = (ulong)encoded.Length - 1;
            input.ShowDraft(chat);
            try { input.AddImage(image); throw new Exception("Truncated clipboard image accepted"); }
            catch (InvalidOperationException) { }
            Check(input.Attachments.Count == 0, "Failed image import left a partial attachment");
            chat.AttachmentBytesLimit = 3;
            input.ShowDraft(chat);
            input.AddFile(file);
            try { input.AddFile(file); throw new Exception("Combined file sizes exceeded the byte limit"); }
            catch (InvalidOperationException) { }
            chat.AttachmentCountLimit = 0;
            input.ShowDraft(chat);
            Check(input.Attachments.Count == 1 && !input.CanSubmitPayload && input.Submission("") == null, "Reduced limits lost or submitted an attachment");
            Check(!ChatInput.CanImportData(new DataObject(DataFormats.UnicodeText, "plain text")), "Plain text paste was intercepted");
            input.ShowDraft(new ChatView { Draft = new() { Key = "no-files" } });
            Check(!input.CanImport && input.Attachments.Count == 0 && input.Submission("") == null, "Attachment crossed conversation boundary");
        }
        finally { File.Delete(file); }
    }

    [STAThread]
    static void Main()
    {
        using var input = new ChatInput();
        input.CreateControl();
        var chat = new ChatView { Draft = new() { Key = "fixture-a" } };
        input.ShowDraft(chat);
        Intent back = new() { Navigate = new() { Back = true } };
        ChatDraft Snapshot() => input.WithDraft(back).ComposerAction.Draft;
        var initial = Snapshot();
        Check(initial.Key == "fixture-a" && initial.Revision.Length > 0, "New chats need a valid revision");

        input.Text = "草稿";
        var submitted = Snapshot();
        Check(submitted.Revision != initial.Revision, "An edit needs its own revision");
        var first = input.Submission("")!.AgentSend;
        Check(first.Text == submitted.Text && first.ReleaseUnconfirmed == "", "Normal send payload");
        input.AppendText(" edited");
        input.Text = submitted.Text;
        var retyped = Snapshot();
        Check(retyped.Revision != submitted.Revision, "Retyping identical text needs a new revision");
        chat.Draft = submitted.Clone();
        chat.Draft.Text = "";
        chat.SentDraftId = first.DraftId;
        input.ShowDraft(chat);
        Check(Snapshot().Equals(retyped), "A late receipt erased a later edit");

        var navigation = input.WithDraft(back).ComposerAction;
        Check(navigation.Draft.Equals(retyped) && navigation.Action.Equals(back), "Navigation lost the draft");
        var retry = input.WithDraft(input.Submission("fixture-token")!).ComposerAction;
        Check(retry.Draft.Equals(retyped) && retry.Action.AgentSend.ReleaseUnconfirmed == "fixture-token", "Retry lost the draft or token");
        var dismiss = input.WithDraft(new() { ResolveAgentSend = new() { Token = "fixture-token", Dismiss = true } });
        Check(dismiss.ComposerAction.Action.ResolveAgentSend.Dismiss && !dismiss.ComposerAction.Action.ResolveAgentSend.Delivered, "Dismissal confirmed delivery");
        chat.SentDraftId = retry.Action.AgentSend.DraftId;
        input.ShowDraft(chat);
        Check(input.TextLength == 0 && input.Submission("") == null, "Matching receipt did not clear the editor");

        var second = new ChatView { Draft = new() { Key = "fixture-b", Text = "other", Revision = "saved" } };
        input.ShowDraft(second);
        Check(Snapshot().Equals(second.Draft), "Switching conversations did not restore the saved draft");
        chat.Draft = retyped.Clone();
        input.ShowDraft(chat);
        Check(Snapshot().Equals(retyped), "Reopening lost the saved revision");
        chat.Draft.Text = "";
        input.ShowDraft(chat);
        Check(input.TextLength == 0, "A Rust clear for the restored revision was missed");
        input.ShowDraft(null);
        Check(input.WithDraft(back) == back, "An unrelated page attached a chat draft");
        Keyboard();
        Attachments();
        Console.WriteLine("Windows draft, native key and attachment checks passed");
    }
}

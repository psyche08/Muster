using Muster.App;

namespace Muster.Windows;

internal sealed class ChatInput : TextBox
{
    string? key;
    string revision = "";
    bool rendering;
    (string Id, string Revision)? submitted;

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
        if (key != draft?.Key)
        {
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
                DisplayText("");
            submitted = null;
        }
    }

    public Intent WithDraft(Intent action) => key == null ? action : new() {
        ComposerAction = new() {
            Action = action,
            Draft = new() { Key = key, Text = Text, Revision = revision }
        }
    };

    public Intent? Submission(string releaseToken)
    {
        if (key == null || string.IsNullOrWhiteSpace(Text))
            return null;
        if (submitted?.Revision != revision || releaseToken.Length > 0)
            submitted = (Guid.NewGuid().ToString(), revision);
        return new() { AgentSend = new() {
            Text = Text, DraftId = submitted.Value.Id, ReleaseUnconfirmed = releaseToken
        } };
    }
}

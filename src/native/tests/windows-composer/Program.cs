using Muster.App;
using Muster.Windows;

internal static class Program
{
    static void Check(bool ok, string failure)
    {
        if (!ok) throw new InvalidOperationException(failure);
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
        Console.WriteLine("Windows composer checks passed");
    }
}

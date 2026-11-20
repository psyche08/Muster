using Muster.App;
using Muster.V1;
using Muster.Windows;

internal static class ChatOptionsTests
{
    public static void Run()
    {
        var chat = new ChatView { DeviceId = "fixture-host", Session = new AgentSession { Id = "fixture-chat" },
                                  CanChooseModel = true, ChatOptions = new AgentChatOptions {
            Session = "fixture-chat", Effort = "Low", Note = "Fixture provider note",
            Models = { new AgentChatOption { Name = "Fast", Detail = "Low latency", Selected = true },
                       new AgentChatOption { Name = "Balanced", Detail = "Standard model" } },
            Efforts = { new AgentChatOption { Name = "Balanced", Detail = "Standard reasoning" } }
        } };
        var sent = new List<Intent>();
        using var owner = new Form { Opacity = 0, ShowInTaskbar = false };
        using var dialog = new ChatOptionsDialog(chat, sent.Add) { Opacity = 0 };
        T Find<T>(string name) where T : Control => (T)dialog.Controls.Find(name, true).Single();
        var grid = Find<DataGridView>("Choices");
        var apply = Find<Button>("ApplyChoice");
        var refresh = Find<Button>("RefreshChoices");
        var report = Find<Label>("Report");
        Exception? failure = null;
        void Check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        dialog.Shown += (_, _) => dialog.BeginInvoke(new Action(() =>
        {
            try
            {
                Check(sent.Count == 1 && sent[0].ChatOptionsLoad != null, "One load on open");
                Check(grid.Rows.Count == 3 && (string?)grid.Rows[0].Cells[3].Value == "Yes" &&
                      (string?)grid.Rows[1].Cells[2].Value == "Standard model" &&
                      report.Text.Contains("Low") && report.Text.Contains("Fixture provider note"), "Provider information");
                grid.CurrentCell = grid.Rows[0].Cells[1];
                Check(!apply.Enabled, "Current option cannot be applied");
                grid.CurrentCell = grid.Rows[1].Cells[1];
                apply.PerformClick();
                Check(sent[^1].ChatSetOptions is { Model: "Balanced", Effort: "" }, "Model routed by category");
                var busy = chat.Clone();
                busy.ChatOptionsBusy = true;
                dialog.Display(busy);
                Check(!apply.Enabled && !refresh.Enabled && !grid.Enabled && report.Text.Contains("host"), "Busy state");
                dialog.Display(chat);
                grid.CurrentCell = grid.Rows[2].Cells[1];
                apply.PerformClick();
                Check(sent[^1].ChatSetOptions is { Effort: "Balanced", Model: "" }, "Reasoning routed by category");
                var failed = chat.Clone();
                failed.ChatOptions = null;
                failed.ChatOptionsError = "Fixture refresh failed";
                dialog.Display(failed);
                Check(grid.Rows.Count == 0 && !apply.Enabled && report.Text == failed.ChatOptionsError, "Stale choices cleared");
                refresh.PerformClick();
                Check(sent.Count == 4 && sent[^1].ChatOptionsLoad != null, "Refresh after failure");
                dialog.Display(chat);
                var other = chat.Clone();
                other.DeviceId = "other-host";
                dialog.Display(other);
                Check(dialog.DialogResult == DialogResult.Cancel, "Close on host replacement");
            }
            catch (Exception error) { failure = error; dialog.Close(); }
        }));
        owner.Show();
        dialog.ShowDialog(owner);
        if (failure != null) throw new InvalidOperationException("Conversation options form", failure);
    }
}

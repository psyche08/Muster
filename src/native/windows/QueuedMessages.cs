using Muster.App;
using Muster.V1;

namespace Muster.Windows;

internal sealed class QueuedMessages : UserControl
{
    readonly Label heading = new() { Dock = DockStyle.Top, AutoSize = true };
    readonly TableLayoutPanel body = new() { Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 1 };
    readonly ListBox list = new() { Name = "QueuedMessageList", AccessibleName = "Queued messages",
        Dock = DockStyle.Fill, IntegralHeight = false, DisplayMember = nameof(PendingInteraction.Prompt) };
    readonly TextBox preview = new() { Name = "QueuedPrompt", AccessibleName = "Selected queued message",
        Dock = DockStyle.Fill, Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical };
    readonly FlowLayoutPanel options = new() { Dock = DockStyle.Bottom, AutoSize = true, WrapContents = true };
    readonly TextBox error = new() { Name = "QueueError", AccessibleName = "Queue error", Dock = DockStyle.Bottom,
        Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical };
    (string Device, string Session)? scope;
    QueuedMessagesView? view;
    bool updating;

    public event Action<Intent>? Intent;

    public QueuedMessages()
    {
        Name = "QueuedMessages";
        AccessibleName = "Provider message queue";
        Dock = DockStyle.Bottom;
        Visible = false;
        var detail = new Panel { Dock = DockStyle.Fill };
        detail.Controls.Add(preview);
        detail.Controls.Add(options);
        body.ColumnStyles.Add(new(SizeType.Percent, 40));
        body.ColumnStyles.Add(new(SizeType.Percent, 60));
        body.RowStyles.Add(new(SizeType.Percent, 100));
        body.Controls.Add(list, 0, 0);
        body.Controls.Add(detail, 1, 0);
        Controls.Add(body);
        Controls.Add(error);
        Controls.Add(heading);
        list.SelectedIndexChanged += (_, _) => { if (!updating) ShowSelection(); };
        FontChanged += (_, _) => SizePanel();
        DpiChangedAfterParent += (_, _) => SizePanel();
    }

    void SizePanel()
    {
        error.Height = Font.Height * 2 + LogicalToDeviceUnits(8);
        Height = heading.PreferredHeight +
            (view is { Messages.Count: > 0 } ? Font.Height * 3 + options.PreferredSize.Height + LogicalToDeviceUnits(12) : 0) +
            (view is { Error.Length: > 0 } ? error.Height : 0);
    }

    void ShowSelection()
    {
        while (options.Controls.Count > 0) options.Controls[0].Dispose();
        var selected = list.SelectedItem as PendingInteraction;
        preview.Text = selected?.Prompt ?? "";
        if (selected != null)
        {
            var origin = scope;
            foreach (var option in selected.Options)
            {
                var button = new Button { Text = option, AutoSize = true };
                button.Click += (_, _) =>
                {
                    if (origin == scope && view is { Submitting: false, OutcomeUnknown: false } &&
                        list.SelectedItem is PendingInteraction row && row.Id == selected.Id && row.Options.Contains(option))
                        Intent?.Invoke(new() { AgentRespond = new() { Interaction = row.Id, Option = option } });
                };
                options.Controls.Add(button);
            }
        }
        SizePanel();
    }

    public void Display(ChatView? chat)
    {
        (string Device, string Session)? nextScope = chat?.Session is {} session ? (chat.DeviceId, session.Id) : null;
        var next = nextScope == null ? null : chat!.Queue;
        bool sameScope = nextScope == scope;
        if (sameScope && Equals(view, next)) return;
        var old = view;
        view = next;
        scope = nextScope;
        bool hasMessages = next is { Messages.Count: > 0 };
        bool hasError = next is { Error.Length: > 0 };
        heading.Text = $"Queued messages ({next?.Messages.Count ?? 0})";
        error.Text = next?.Error ?? "";
        error.Visible = hasError;
        body.Visible = hasMessages;
        options.Enabled = next is { Submitting: false, OutcomeUnknown: false };
        if (!sameScope || old?.Messages.Equals(next?.Messages) != true)
        {
            var selected = (list.SelectedItem as PendingInteraction)?.Id;
            var top = list.TopIndex >= 0 && list.TopIndex < list.Items.Count ? ((PendingInteraction)list.Items[list.TopIndex]).Id : null;
            updating = true;
            list.BeginUpdate();
            try
            {
                list.Items.Clear();
                if (hasMessages)
                {
                    list.Items.AddRange(next!.Messages.Cast<object>().ToArray());
                    list.SelectedItem = sameScope && old is { Messages.Count: > 0 }
                        ? next.Messages.FirstOrDefault(i => i.Id == selected) : next.Messages[0];
                    if (sameScope && top != null && next.Messages.FirstOrDefault(i => i.Id == top) is {} first)
                        list.TopIndex = list.Items.IndexOf(first);
                }
            }
            finally { list.EndUpdate(); updating = false; }
            ShowSelection();
        }
        else SizePanel();
        Visible = hasMessages || hasError;
    }
}

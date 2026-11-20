using Muster.App;
using Muster.V1;

namespace Muster.Windows;

internal sealed class DesktopChatSidebar : UserControl
{
    readonly ComboBox provider = new() { Name = "Provider", AccessibleName = "Chat app",
        DropDownStyle = ComboBoxStyle.DropDownList, DisplayMember = nameof(ChatProvider.Label), Dock = DockStyle.Top };
    readonly FlowLayoutPanel bar = new() { Dock = DockStyle.Top, AutoSize = true, WrapContents = true };
    readonly Button create = new() { Name = "NewChat", Text = "New chat", AutoSize = true };
    readonly Button switchMode = new() { Name = "SwitchMode", Text = "Switch to Chat mode", AutoSize = true };
    readonly Label status = new() { Name = "CatalogStatus", Dock = DockStyle.Top, AutoSize = true };
    readonly ListBox chats = new() { Name = "Conversations", AccessibleName = "Conversations", Dock = DockStyle.Fill,
        DisplayMember = nameof(AgentChat.Title), HorizontalScrollbar = true, IntegralHeight = false };
    DesktopChatsView? view;
    string shown = "";
    (string Provider, string Device, string Id)? pressed;
    public event Action<Intent>? Intent;

    public DesktopChatSidebar()
    {
        AccessibleName = "Desktop chats";
        Padding = new Padding(0, 0, 8, 0);
        bar.Controls.AddRange([create, switchMode]);
        Controls.AddRange([chats, status, bar, provider]);
        Resize += (_, _) => status.MaximumSize = new Size(Math.Max(1, ClientSize.Width - Padding.Horizontal), 0);
        provider.SelectedIndexChanged += (_, _) =>
        {
            if (provider.SelectedItem is ChatProvider p && p.Id != view?.Provider)
                Intent?.Invoke(new() { OpenDesktopChats = new() { Provider = p.Id } });
        };
        create.Click += (_, _) => Open("", false);
        switchMode.Click += (_, _) => Open("", true);
        chats.MouseDown += (_, e) => pressed = e.Button == MouseButtons.Left ? At(e.Location) : null;
        chats.MouseClick += (_, e) =>
        {
            var row = At(e.Location);
            if (e.Button == MouseButtons.Left && row != null && row == pressed)
                Open(row.Value.Id, false);
            pressed = null;
        };
        chats.KeyDown += (_, e) =>
        {
            if (e.KeyData != Keys.Enter) return;
            e.SuppressKeyPress = true;
            if (chats.SelectedItem is AgentChat row) Open(row.Id, false);
        };
    }

    (string, string, string Id)? At(Point point)
    {
        var index = chats.IndexFromPoint(point);
        return view != null && index >= 0 && chats.Items[index] is AgentChat row
            ? (view.Provider, view.DeviceId, row.Id) : null;
    }

    void Open(string id, bool desktop)
    {
        if (view is not { DeviceId.Length: > 0, Opening.Length: 0 }) return;
        Intent?.Invoke(new() { DesktopChatOpen = new() { Chat = id, Desktop = desktop } });
    }

    int Index(string id)
    {
        if (id.Length > 0)
            for (int i = 0; i < chats.Items.Count; ++i)
                if (((AgentChat)chats.Items[i]).Id == id) return i;
        return -1;
    }

    public void Display(DesktopChatsView? next, string currentDevice = "", string currentSession = "")
    {
        var old = view;
        view = next;
        bool sameHost = old != null && next != null && old.Provider == next.Provider && old.DeviceId == next.DeviceId;
        if (!sameHost) pressed = null;
        if (old?.Choices.Equals(next?.Choices) != true && (old != null || next != null))
        {
            provider.Items.Clear();
            if (next != null) provider.Items.AddRange(next.Choices.Cast<object>().ToArray());
        }
        if ((provider.SelectedItem as ChatProvider)?.Id != next?.Provider)
            provider.SelectedItem = provider.Items.Cast<ChatProvider>().FirstOrDefault(p => p.Id == next?.Provider);
        bool canOpen = next is { DeviceId.Length: > 0, Opening.Length: 0 };
        create.Enabled = chats.Enabled = switchMode.Enabled = canOpen;
        switchMode.Visible = next?.Catalog is { ChatMode: false };
        string selected = currentDevice == next?.DeviceId ? currentSession : "";
        var rows = next?.Catalog?.Chats;
        if (old?.Catalog?.Chats != rows && old?.Catalog?.Chats.Equals(rows) != true)
        {
            string? previous = (chats.SelectedItem as AgentChat)?.Id;
            string? top = chats.Items.Count > 0 && chats.TopIndex >= 0 ? ((AgentChat)chats.Items[chats.TopIndex]).Id : null;
            chats.BeginUpdate();
            try
            {
                chats.Items.Clear();
                if (rows != null) chats.Items.AddRange(rows.Cast<object>().ToArray());
                bool keepPosition = sameHost && selected == shown;
                int previousIndex = keepPosition && previous != null ? Index(previous) : -1;
                chats.SelectedIndex = previousIndex >= 0 ? previousIndex : Index(selected);
                if (keepPosition && top != null)
                {
                    int first = Index(top);
                    if (first >= 0) chats.TopIndex = first;
                }
            }
            finally { chats.EndUpdate(); }
        }
        else if (!sameHost || selected != shown)
            chats.SelectedIndex = Index(selected);
        shown = selected;
        var lines = new List<string>();
        if (next is { DeviceName.Length: > 0 }) lines.Add("Through " + next.DeviceName);
        if (next?.Loading == true) lines.Add("Updating conversations…");
        if (next is { Opening.Length: > 0 }) lines.Add("Opening conversation…");
        if (next is { Error.Length: > 0 }) lines.Add(next.Error);
        else if (next is { Loading: false, Catalog: { ChatMode: true, Chats.Count: 0 } }) lines.Add("No conversations");
        status.Text = string.Join(Environment.NewLine, lines);
    }
}

using System.Reflection;
using Muster.App;
using Muster.V1;
using Muster.Windows;

internal static class DesktopChatSidebarTests
{
    public static void Run(string? captures)
    {
        static void Check(bool condition, string name)
        {
            if (!condition) throw new InvalidOperationException(name);
        }
        using var host = new Form { ShowInTaskbar = false, StartPosition = FormStartPosition.Manual,
            Location = new Point(-20000, -20000), ClientSize = new Size(440, 380) };
        var sidebar = new DesktopChatSidebar { Dock = DockStyle.Fill };
        host.Controls.Add(sidebar);
        host.Show();
        var intents = new List<Intent>();
        sidebar.Intent += intents.Add;
        var list = (ListBox)sidebar.Controls.Find("Conversations", true).Single();
        var combo = (ComboBox)sidebar.Controls.Find("Provider", true).Single();
        var status = (Label)sidebar.Controls.Find("CatalogStatus", true).Single();
        var create = (Button)sidebar.Controls.Find("NewChat", true).Single();
        var mode = (Button)sidebar.Controls.Find("SwitchMode", true).Single();
        var view = new DesktopChatsView { DeviceId = "host-a", DeviceName = "Test desktop", Provider = "codex",
            Choices = { new ChatProvider { Id = "codex", Label = "ChatGPT" }, new ChatProvider { Id = "claude", Label = "Claude" } },
            Catalog = new() { ChatMode = true, Chats = { new AgentChat { Id = "one", Title = "Planning" },
                new AgentChat { Id = "two", Title = "Planning" } } } };
        void Enter() => typeof(Control).GetMethod("OnKeyDown", BindingFlags.NonPublic | BindingFlags.Instance)!
            .Invoke(list, [new KeyEventArgs(Keys.Enter)]);
        void Mouse(string method, Point point) => typeof(Control).GetMethod(method, BindingFlags.NonPublic | BindingFlags.Instance)!
            .Invoke(list, [new MouseEventArgs(MouseButtons.Left, 1, point.X, point.Y, 0)]);
        sidebar.Display(view, "host-a", "two");
        Check(list.SelectedIndex == 1 && combo.Text == "ChatGPT" && status.Text.Contains("Test desktop"), "shown chat, host and provider");
        var point = new Point(6, list.GetItemRectangle(0).Top + 2);
        Mouse("OnMouseDown", point);
        Mouse("OnMouseClick", point);
        Check(intents.Single().DesktopChatOpen.Chat == "one", "single click selects exact row with duplicate titles");
        list.SelectedIndex = 1;
        Enter();
        Check(intents.Last().DesktopChatOpen.Chat == "two", "Enter selects the listed opaque ID");
        var moved = view.Clone();
        moved.Catalog.Chats.Insert(0, new AgentChat { Id = "three", Title = "Review" });
        sidebar.Display(moved, "host-a", "two");
        Check(list.SelectedIndex == 2 && status.Bottom <= list.Top && list.Height > list.ItemHeight * 2, "selection and native layout on refresh");
        if (captures != null)
        {
            using var image = new Bitmap(sidebar.Width, sidebar.Height);
            sidebar.DrawToBitmap(image, sidebar.ClientRectangle);
            image.Save(Path.Combine(captures, "desktop-chat-sidebar.png"), System.Drawing.Imaging.ImageFormat.Png);
        }
        Mouse("OnMouseDown", point);
        var other = moved.Clone();
        other.DeviceId = "host-b";
        sidebar.Display(other);
        Mouse("OnMouseClick", point);
        Check(intents.Count == 2 && list.SelectedIndex == -1, "stale mouse gesture on host change");
        other = other.Clone();
        other.Opening = "two";
        sidebar.Display(other);
        list.SelectedIndex = 1;
        Enter();
        Check(!list.Enabled && !create.Enabled && intents.Count == 2 && status.Text.Contains("Opening"), "one open at a time");
        var offline = other.Clone();
        offline.Opening = ""; offline.DeviceId = ""; offline.DeviceName = ""; offline.Error = "Test host unavailable";
        sidebar.Display(offline);
        Enter();
        Check(!list.Enabled && intents.Count == 2 && list.Items.Count == 3 && status.Text == offline.Error, "cached directory without host");
        var empty = view.Clone();
        empty.Catalog.Chats.Clear();
        sidebar.Display(empty);
        Check(status.Text.Contains("No conversations") && create.Enabled, "observed empty catalog");
        var loading = empty.Clone(); loading.Loading = true;
        sidebar.Display(loading);
        Check(status.Text.Contains("Updating") && !status.Text.Contains("No conversations"), "pending catalog");
        var work = empty.Clone(); work.Catalog.ChatMode = false;
        sidebar.Display(work);
        mode.PerformClick(); create.PerformClick();
        Check(intents[^2].DesktopChatOpen.Desktop && !intents[^1].DesktopChatOpen.Desktop, "switch mode and new chat");
        int previousCount = intents.Count;
        combo.SelectedIndex = 1;
        Check(intents.Count == previousCount + 1 && intents[^1].OpenDesktopChats.Provider == "claude", "selected provider ID");
        sidebar.Display(work);
        Check(intents.Count == previousCount + 1, "rendering the current provider does not navigate");
        sidebar.Display(null);
        Check(list.Items.Count == 0 && !list.Enabled, "leaving desktop chats");
        Console.WriteLine($"Passed desktop chat directory selection, refresh, busy, cache and empty states at {host.DeviceDpi} DPI.");
    }
}

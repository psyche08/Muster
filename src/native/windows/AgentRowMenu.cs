using Muster.App;
using Organize = Muster.V1.AgentOrganizeRequest.Types.Action;

namespace Muster.Windows;

internal static class AgentRowMenu
{
    public static void Populate(ContextMenuStrip menu, AgentCatalogRow row, Action<Intent> send)
    {
        void Item(string label, Intent intent, bool enabled)
        {
            var item = new ToolStripMenuItem(label) { Enabled = enabled };
            item.Click += (_, _) => send(intent);
            menu.Items.Add(item);
        }
        void OrganizeItem(string label, Organize action) =>
            Item(label, new() { AgentOrganize = new() { RowKey = row.Key, Action = action } }, !row.Organizing);

        if (row.CanOrganize)
        {
            if (row.Session.Pinned) OrganizeItem("Unpin", Organize.Unpin);
            else OrganizeItem("Pin", Organize.Pin);
        }
        if (row.CanArchive) OrganizeItem("Archive", Organize.Archive);
        if (row.Session.ParentSessionId.Length > 0)
            Item("Open parent conversation", new() { OpenAgent = new()
                { Session = row.Session.ParentSessionId, DeviceId = row.Session.DeviceId } }, true);
    }
}

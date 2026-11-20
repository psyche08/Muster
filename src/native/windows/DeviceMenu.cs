using Muster.V1;

namespace Muster.Windows;

internal static class DeviceMenu
{
    internal static string Label(IEnumerable<MemberView> devices, string selected) =>
        selected is "" or "*" ? "All devices" : devices.FirstOrDefault(d => d.DeviceId == selected)?.Name ?? selected;

    internal static ContextMenuStrip Create(IEnumerable<MemberView> devices, string selected, Action<string> choose)
    {
        var menu = new ContextMenuStrip();
        void Choice(string label, string id, bool active)
        {
            var item = new ToolStripMenuItem(label) { Checked = active };
            item.Click += (_, _) => choose(id);
            menu.Items.Add(item);
        }
        Choice("All devices", "*", selected is "" or "*");
        foreach (var device in devices)
            Choice(device.Name, device.DeviceId, selected == device.DeviceId);
        return menu;
    }
}

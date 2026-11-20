namespace Muster.Windows;

internal static class ButtonMenu
{
    public static void ShowBelow(this ContextMenuStrip menu, Control anchor)
    {
        void Release(object? sender, EventArgs e) => menu.Dispose();
        anchor.HandleDestroyed += Release;
        menu.Disposed += (_, _) => anchor.HandleDestroyed -= Release;
        menu.Closed += (_, _) =>
        {
            // Let the selected item's click finish. The anchor also releases
            // the menu if its handle disappears before this callback runs.
            if (!menu.Disposing && !menu.IsDisposed && anchor.IsHandleCreated)
                anchor.BeginInvoke(new Action(menu.Dispose));
        };
        menu.Show(anchor, new Point(0, anchor.Height));
    }
}

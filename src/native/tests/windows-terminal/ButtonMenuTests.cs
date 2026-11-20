using Muster.Windows;

internal static class ButtonMenuTests
{
    sealed class KeyboardMenu : ContextMenuStrip
    {
        public void ChooseSelected() => ProcessDialogKey(Keys.Enter);
    }

    public static void Run()
    {
        using var form = new Form { ShowInTaskbar = false, Opacity = 0 };
        var anchor = new Button { Text = "Fixture actions" };
        form.Controls.Add(anchor);
        form.Show();
        using var selected = new KeyboardMenu();
        int calls = 0;
        var item = selected.Items.Add("Fixture choice", null, (_, _) => calls++);
        selected.ShowBelow(anchor);
        item.Select();
        selected.ChooseSelected();
        Application.DoEvents();
        if (calls != 1 || !selected.IsDisposed)
            throw new InvalidOperationException("Menu selection must run once and release the menu");

        using var open = new ContextMenuStrip();
        open.Items.Add("Fixture choice");
        open.ShowBelow(anchor);
        if (!open.Visible) throw new InvalidOperationException("Expected visible menu");
        form.Close();
        Application.DoEvents();
        if (!open.IsDisposed) throw new InvalidOperationException("Open menu survived the anchor");
        Console.WriteLine("Passed action menu selection and shutdown with the menu open.");
    }
}

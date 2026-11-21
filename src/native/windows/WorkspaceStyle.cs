namespace Muster.Windows;

internal static class WorkspaceStyle
{
    public static Color Canvas => SystemColors.Window;
    public static Color Sidebar => SystemInformation.HighContrast ? SystemColors.Window : Color.FromArgb(246, 245, 242);
    public static Color Ink => SystemColors.WindowText;
    public static Color Muted => SystemInformation.HighContrast ? Ink : Color.FromArgb(92, 91, 87);
    public static Color Selected => SystemInformation.HighContrast ? SystemColors.Highlight : Color.FromArgb(62, 55, 51);
    public static Color SelectedText => SystemInformation.HighContrast ? SystemColors.HighlightText : Color.White;
    public static Color Edge => SystemInformation.HighContrast ? SystemColors.WindowText : Color.FromArgb(217, 214, 208);

    public static Label DirectoryHeading(string text)
    {
        var label = new Label { Text = text, AutoSize = true, Dock = DockStyle.Top,
            ForeColor = Ink, Font = new Font("Segoe UI", 14, FontStyle.Bold), Padding = new Padding(0, 0, 0, 8) };
        label.Disposed += (_, _) => label.Font.Dispose();
        return label;
    }

    public static void StyleButton(Button button)
    {
        button.AutoSizeMode = AutoSizeMode.GrowAndShrink;
        button.FlatStyle = FlatStyle.Flat;
        button.FlatAppearance.BorderSize = 1;
        button.FlatAppearance.BorderColor = Edge;
        button.BackColor = Canvas;
        button.ForeColor = Ink;
        button.Padding = new Padding(6, 2, 6, 2);
        button.Margin = new Padding(2);
    }
}

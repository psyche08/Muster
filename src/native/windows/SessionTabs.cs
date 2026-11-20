using Muster.App;

namespace Muster.Windows;

internal enum SessionTab { Conversation, Trace, Subagents, Sessions, Terminal }

internal sealed class SessionTabs : FlowLayoutPanel
{
    public event Action<SessionTab>? Chosen;

    public SessionTabs()
    {
        AccessibleName = "Session views";
        Dock = DockStyle.Top;
        AutoScroll = true;
        WrapContents = false;
        foreach (var tab in Enum.GetValues<SessionTab>())
        {
            var button = new RadioButton { Name = tab + "Tab", Text = tab.ToString(), Tag = tab,
                Appearance = Appearance.Button, AutoCheck = false, AutoSize = true, TabStop = true };
            button.Click += (_, _) => Chosen?.Invoke(tab);
            Controls.Add(button);
        }
    }

    protected override void OnLayout(LayoutEventArgs e)
    {
        base.OnLayout(e);
        if (Controls.Count == 0) return;
        int width = Controls.Cast<Control>().Sum(b => b.Width + b.Margin.Horizontal);
        int row = Controls.Cast<Control>().Max(b => b.Height + b.Margin.Vertical);
        int height = row + (width > ClientSize.Width ? SystemInformation.HorizontalScrollBarHeight : 0);
        if (height != Height) Height = height;
    }

    public void Display(ChatView chat, SessionTab selected)
    {
        foreach (RadioButton button in Controls)
        {
            var tab = (SessionTab)button.Tag!;
            int count = tab switch { SessionTab.Subagents => chat.Session.Subagents.Count,
                SessionTab.Sessions => chat.Session.Subsessions.Count, _ => 0 };
            string label = count > 0 ? $"{tab} ({count})" : tab.ToString();
            if (button.Text != label) button.Text = label;
            button.Enabled = tab != SessionTab.Terminal || chat.Terminal?.Available == true;
            if (button.Checked == (tab == selected)) continue;
            button.Checked = tab == selected;
            if (button.Checked) ScrollControlIntoView(button);
        }
    }
}

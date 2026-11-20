using Muster.App;

namespace Muster.Windows;

internal sealed class RunHistoryDialog : Form
{
    sealed record Choice(string Id, string Label)
    {
        public override string ToString() => Label;
    }

    readonly ComboBox runs = new() { Name = "Runs", AccessibleName = "Recent runs",
        Dock = DockStyle.Top, DropDownStyle = ComboBoxStyle.DropDownList };
    readonly TextBox details = new() { Name = "RunDetails", AccessibleName = "Execution result",
        Dock = DockStyle.Fill, Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Both, WordWrap = false };
    readonly Button returnResult = new() { Name = "ReturnResult", AutoSize = true, Text = "Return result to conversation" };
    readonly string device, session;
    ChatView? current;

    public RunHistoryDialog(ChatView chat, Action<Intent> send)
    {
        device = chat.DeviceId;
        session = chat.Session.Id;
        Text = "Recent runs";
        AutoScaleDimensions = new SizeF(96, 96);
        AutoScaleMode = AutoScaleMode.Dpi;
        MinimumSize = new(330, 220);
        Padding = new(12);
        StartPosition = FormStartPosition.CenterParent;
        ShowInTaskbar = false;
        var close = new Button { AutoSize = true, Text = "Close", DialogResult = DialogResult.Cancel };
        var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true,
            FlowDirection = FlowDirection.RightToLeft };
        buttons.Controls.Add(close);
        buttons.Controls.Add(returnResult);
        Controls.Add(details);
        Controls.Add(runs);
        Controls.Add(buttons);
        CancelButton = close;
        runs.SelectedIndexChanged += (_, _) => ShowSelection();
        returnResult.Click += (_, _) =>
        {
            if (Selected() is { CanReturn: true } selection)
                send(new() { CodeAction = new() { Action = CodeAction.Types.Action.Return, Id = selection.Run.Id } });
        };
        Display(chat);
    }

    protected override void OnLoad(EventArgs e)
    {
        base.OnLoad(e);
        var area = System.Windows.Forms.Screen.FromControl(Owner ?? this).WorkingArea;
        var preferred = SizeFromClientSize(LogicalToDeviceUnits(new Size(640, 430)));
        Size = new Size(Math.Min(area.Width, preferred.Width), Math.Min(area.Height, preferred.Height));
        if (Owner != null) CenterToParent();
    }

    CodeRunView? Selected() => runs.SelectedItem is Choice choice
        ? current?.CodeRuns.FirstOrDefault(item => item.Run?.Id == choice.Id) : null;

    public void Display(ChatView? chat)
    {
        if (chat?.Session?.Id != session || chat.DeviceId != device)
        {
            current = null;
            Close();
            return;
        }
        if (current != null && current.CodeRuns.Equals(chat.CodeRuns) && current.CodeError == chat.CodeError) return;
        bool first = current == null;
        var selected = (runs.SelectedItem as Choice)?.Id;
        current = chat;
        var choices = chat.CodeRuns.Where(item => item.Run != null).Select(item =>
        {
            var run = item.Run;
            var line = run.Code.Split('\n', 2)[0].Trim();
            return new Choice(run.Id, $"{run.State} — {line}");
        }).ToArray();
        if (!runs.Items.Cast<Choice>().SequenceEqual(choices))
        {
            runs.BeginUpdate();
            runs.Items.Clear();
            runs.Items.AddRange(choices);
            runs.SelectedIndex = first && choices.Length > 0 ? 0 : Array.FindIndex(choices, c => c.Id == selected);
            runs.EndUpdate();
        }
        ShowSelection();
    }

    void ShowSelection()
    {
        var item = Selected();
        var lines = new List<string>();
        if (current?.CodeError is { Length: > 0 } error) lines.Add(error);
        if (item?.Run is {} run)
        {
            lines.Add($"Execution: {run.State}");
            if (item.Delivery.Length > 0) lines.Add($"Result delivery: {item.Delivery}");
            if (item.Error.Length > 0) lines.Add(item.Error);
            if (run.ExitKnown) lines.Add($"Exit code: {run.ExitCode}");
            if (run.Cwd.Length > 0) lines.Add($"Directory: {run.Cwd}");
            if (run.Language.Length > 0) lines.Add($"Language: {run.Language}");
            lines.Add("");
            lines.Add("Command");
            lines.Add(run.Code);
            lines.Add("");
            lines.Add(run.Truncated ? "Output (truncated)" : "Output");
            lines.Add(run.Output.Length > 0 ? run.Output : "No output recorded");
        }
        else lines.Add("Select a run to view its result.");
        string value = string.Join(Environment.NewLine, lines).ReplaceLineEndings(Environment.NewLine);
        if (value != details.Text) details.Text = value;
        returnResult.Visible = item?.CanReturn == true;
    }
}

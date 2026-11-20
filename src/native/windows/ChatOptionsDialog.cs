using Muster.App;
using Muster.V1;

namespace Muster.Windows;

internal sealed class ChatOptionsDialog : Form
{
    sealed record Choice(bool Model, AgentChatOption Option);
    readonly string device, session;
    readonly Action<Intent> send;
    readonly DataGridView choices = new() { Name = "Choices", AccessibleName = "Model and reasoning choices",
        Dock = DockStyle.Fill, ReadOnly = true, AllowUserToAddRows = false, AllowUserToDeleteRows = false,
        AllowUserToResizeRows = false, RowHeadersVisible = false, MultiSelect = false,
        AutoSizeColumnsMode = DataGridViewAutoSizeColumnsMode.Fill,
        AutoSizeRowsMode = DataGridViewAutoSizeRowsMode.AllCellsExceptHeaders,
        SelectionMode = DataGridViewSelectionMode.FullRowSelect };
    readonly Label report = new() { Name = "Report", Dock = DockStyle.Top, AutoSize = true };
    readonly Button apply = new() { Name = "ApplyChoice", Text = "Apply choice", AutoSize = true };
    readonly Button refresh = new() { Name = "RefreshChoices", Text = "Refresh", AutoSize = true };
    ChatView? current;

    public ChatOptionsDialog(ChatView chat, Action<Intent> send)
    {
        this.send = send;
        device = chat.DeviceId;
        session = chat.Session.Id;
        Text = "Model and effort";
        StartPosition = FormStartPosition.CenterParent;
        ShowInTaskbar = false;
        AutoScaleDimensions = new SizeF(96, 96);
        AutoScaleMode = AutoScaleMode.Dpi;
        MinimumSize = new Size(360, 240);
        Padding = new Padding(12);
        choices.DefaultCellStyle.WrapMode = DataGridViewTriState.True;
        foreach (var (name, weight) in new[] { ("Setting", 16), ("Option", 28), ("Description", 43), ("Current", 13) })
            choices.Columns.Add(new DataGridViewTextBoxColumn { HeaderText = name, FillWeight = weight,
                SortMode = DataGridViewColumnSortMode.NotSortable });
        var close = new Button { Text = "Done", AutoSize = true, DialogResult = DialogResult.Cancel };
        var buttons = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true, FlowDirection = FlowDirection.RightToLeft };
        buttons.Controls.AddRange([close, apply, refresh]);
        Controls.AddRange([choices, report, buttons]);
        CancelButton = close;
        choices.SelectionChanged += (_, _) => UpdateApply();
        choices.CellDoubleClick += (_, e) => { if (e.RowIndex >= 0) Apply(); };
        choices.KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter) { e.SuppressKeyPress = true; Apply(); }
        };
        apply.Click += (_, _) => Apply();
        refresh.Click += (_, _) => LoadChoices();
        Shown += (_, _) => LoadChoices();
        Resize += (_, _) => report.MaximumSize = new Size(Math.Max(1, ClientSize.Width - Padding.Horizontal), 0);
        Display(chat);
    }

    protected override void OnLoad(EventArgs e)
    {
        base.OnLoad(e);
        var area = System.Windows.Forms.Screen.FromControl(Owner ?? this).WorkingArea;
        var preferred = SizeFromClientSize(LogicalToDeviceUnits(new Size(600, 360)));
        Size = new Size(Math.Min(area.Width, preferred.Width), Math.Min(area.Height, preferred.Height));
        if (Owner != null) CenterToParent();
    }

    Choice? Selected => choices.SelectedRows.Count == 1 ? choices.SelectedRows[0].Tag as Choice : null;
    void UpdateApply() => apply.Enabled = current is { ChatOptionsBusy: false } && Selected is { Option.Selected: false };
    void LoadChoices()
    {
        if (current is { ChatOptionsBusy: false }) send(new() { ChatOptionsLoad = new() });
    }
    void Apply()
    {
        if (current is not { ChatOptionsBusy: false } || Selected is not { Option.Selected: false } choice) return;
        send(new() { ChatSetOptions = choice.Model ? new() { Model = choice.Option.Name } : new() { Effort = choice.Option.Name } });
    }

    public void Display(ChatView? chat)
    {
        if (chat?.Session?.Id != session || chat.DeviceId != device || !chat.CanChooseModel)
        {
            current = null;
            DialogResult = DialogResult.Cancel;
            return;
        }
        var options = chat.ChatOptions;
        var old = current?.ChatOptions;
        current = chat;
        if (!Equals(options, old))
        {
            var previous = Selected;
            choices.Rows.Clear();
            void AddChoices(IEnumerable<AgentChatOption> items, bool model)
            {
                foreach (var option in items)
                {
                    var row = choices.Rows[choices.Rows.Add(model ? "Model" : "Reasoning", option.Name,
                        option.Detail, option.Selected ? "Yes" : "")];
                    row.Tag = new Choice(model, option);
                    if (previous?.Model == model && previous.Option.Name == option.Name)
                        choices.CurrentCell = row.Cells[1];
                }
            }
            if (options != null) { AddChoices(options.Models, true); AddChoices(options.Efforts, false); }
        }
        var messages = new List<string>();
        if (chat.ChatOptionsBusy) messages.Add("Reading options from the host…");
        if (chat.ChatOptionsError.Length > 0) messages.Add(chat.ChatOptionsError);
        if (options is { Effort.Length: > 0 }) messages.Add("Current reasoning: " + options.Effort);
        if (options is { Note.Length: > 0 }) messages.Add(options.Note);
        report.Text = string.Join(Environment.NewLine, messages);
        refresh.Enabled = choices.Enabled = !chat.ChatOptionsBusy;
        UpdateApply();
    }
}

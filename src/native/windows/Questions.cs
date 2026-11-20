using Muster.V1;

namespace Muster.Windows;

internal sealed class Questions : Form
{
    readonly List<Func<QuestionAnswer>> readers = new();
    public IEnumerable<QuestionAnswer> Answers => readers.Select(read => read());

    public Questions(PendingInteraction interaction)
    {
        Text = interaction.Prompt;
        Size = new Size(620, 540);
        StartPosition = FormStartPosition.CenterParent;
        var body = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown,
                                         WrapContents = false, AutoScroll = true, Padding = new Padding(12) };
        foreach (var question in interaction.Questions)
        {
            var group = new FlowLayoutPanel { AutoSize = true, FlowDirection = FlowDirection.TopDown };
            group.Controls.Add(new Label { Text = question.Prompt, AutoSize = true, MaximumSize = new Size(540, 0) });
            var selected = new List<Func<string?>>();
            foreach (var option in question.Options)
            {
                var label = option.Label + (option.Description.Length > 0 ? "  " + option.Description : "");
                if (question.MultiSelect)
                {
                    var check = new CheckBox { Text = label, AutoSize = true };
                    group.Controls.Add(check);
                    selected.Add(() => check.Checked ? option.Id : null);
                }
                else
                {
                    var radio = new RadioButton { Text = label, AutoSize = true };
                    group.Controls.Add(radio);
                    selected.Add(() => radio.Checked ? option.Id : null);
                }
            }
            var custom = question.AllowText ? new TextBox { Width = 500, PlaceholderText = "Your answer" } : null;
            if (custom != null)
                group.Controls.Add(custom);
            readers.Add(() => new QuestionAnswer { QuestionId = question.Id, Text = custom?.Text.Trim() ?? "",
                                                   OptionIds = { selected.Select(read => read()).OfType<string>() } });
            body.Controls.Add(group);
        }
        var submit = new Button { Text = "Submit answers", Dock = DockStyle.Bottom, Height = 34 };
        submit.Click += (_, _) =>
        {
            if (Answers.Any(a => a.Text.Length == 0 && a.OptionIds.Count == 0))
            {
                MessageBox.Show(this, "Choose an option or enter an answer for each question.");
                return;
            }
            DialogResult = DialogResult.OK;
        };
        Controls.Add(body);
        Controls.Add(submit);
    }
}

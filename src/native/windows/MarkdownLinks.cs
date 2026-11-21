using System.Runtime.CompilerServices;
using System.Text;
using Markdig;
using Markdig.Syntax;
using Markdig.Syntax.Inlines;
using Markdig.Extensions.Tables;

namespace Muster.Windows;

internal sealed record TranscriptLink(int Line, int Start, int Length, string Destination);
[Flags]
internal enum TextStyle { Normal = 0, Bold = 1, Italic = 2, Code = 4, Heading = 8, Quote = 16, Prompt = 32 }
internal sealed record TranscriptStyle(int Line, int Start, int Length, TextStyle Style);

internal static class MarkdownLinks
{
    sealed record Parsed(string Text, List<TranscriptLink> Links, List<TranscriptStyle> Styles);
    static readonly ConditionalWeakTable<string, Parsed> cache = new();
    static readonly MarkdownPipeline pipeline = new MarkdownPipelineBuilder().UsePipeTables().Build();

    public static string Display(string text, string prefix, int line, List<TranscriptLink> links, List<TranscriptStyle>? styles = null)
    {
        var parsed = cache.GetValue(text, Parse);
        foreach (var link in parsed.Links) links.Add(link with { Line = line, Start = prefix.Length + link.Start });
        if (styles != null)
            foreach (var span in parsed.Styles) styles.Add(span with { Line = line, Start = prefix.Length + span.Start });
        return prefix + parsed.Text;
    }

    static Parsed Parse(string text)
    {
        string source = text.ReplaceLineEndings("\n");
        var output = new StringBuilder();
        List<TranscriptLink> links = [];
        List<TranscriptStyle> styles = [];
        void Write(string value, TextStyle style)
        {
            if (style != TextStyle.Normal && value.Length > 0)
                styles.Add(new(0, output.Length, value.Length, style));
            output.Append(value);
        }
        void Inline(Inline item, TextStyle style)
        {
            switch (item)
            {
                case LiteralInline literal: Write(literal.Content.ToString(), style); break;
                case CodeInline code: Write(code.Content, style | TextStyle.Code); break;
                case LineBreakInline: output.Append('\n'); break;
                case HtmlEntityInline entity: Write(entity.Transcoded.ToString(), style); break;
                case AutolinkInline auto:
                    AddLink(auto.IsEmail ? "mailto:" + auto.Url : auto.Url, () => Write(auto.Url, style));
                    break;
                case LinkInline link when !string.IsNullOrWhiteSpace(link.Url):
                    AddLink(link.Url, () => { foreach (var child in link) Inline(child, style); });
                    break;
                case ContainerInline container:
                    if (container is EmphasisInline emphasis)
                        style |= emphasis.DelimiterCount == 1 ? TextStyle.Italic : TextStyle.Bold;
                    foreach (var child in container) Inline(child, style);
                    break;
                default: Write(source.Substring(item.Span.Start, item.Span.Length), style); break;
            }
        }
        void AddLink(string target, Action label)
        {
            if (links.Count > 0 && links[^1].Start + links[^1].Length == output.Length) output.Append(' ');
            int start = output.Length;
            label();
            if (output.Length == start) output.Append(target);
            links.Add(new(0, start, output.Length - start, target));
        }
        void Blocks(ContainerBlock container, TextStyle style, int depth, string separator)
        {
            bool first = true;
            foreach (var block in container)
            {
                if (block is LinkReferenceDefinitionGroup) continue;
                if (!first) output.Append(separator);
                first = false;
                switch (block)
                {
                    case HeadingBlock heading when heading.Inline != null:
                        Inline(heading.Inline, style | TextStyle.Heading | TextStyle.Bold);
                        break;
                    case CodeBlock code:
                        Write(code.Lines.ToString().TrimEnd('\n'), style | TextStyle.Code);
                        break;
                    case LeafBlock leaf when leaf.Inline != null: Inline(leaf.Inline, style); break;
                    case ListBlock list:
                        int.TryParse(list.OrderedStart, out int number);
                        for (int i = 0; i < list.Count; ++i)
                        {
                            if (i > 0) output.Append('\n');
                            output.Append(' ', depth * 2).Append(list.IsOrdered ? $"{number + i}. " : "• ");
                            Blocks((ListItemBlock)list[i], style, depth + 1, "\n");
                        }
                        break;
                    case QuoteBlock quote:
                        Write("│ ", TextStyle.Quote);
                        Blocks(quote, style | TextStyle.Quote, depth, "\n");
                        break;
                    case Table table:
                        foreach (var row in table.OfType<TableRow>())
                        {
                            if (row != table[0]) output.Append('\n');
                            Blocks(row, row.IsHeader ? style | TextStyle.Bold : style, depth, "   │   ");
                        }
                        break;
                    case ContainerBlock nested: Blocks(nested, style, depth, "\n"); break;
                    default: Write(source.Substring(block.Span.Start, block.Span.Length), style); break;
                }
            }
        }
        Blocks(Markdown.Parse(source, pipeline), TextStyle.Normal, 0, "\n\n");
        return new(output.ToString(), links, styles);
    }
}

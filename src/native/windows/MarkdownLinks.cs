using System.Runtime.CompilerServices;
using System.Text;
using Markdig;
using Markdig.Syntax;
using Markdig.Syntax.Inlines;

namespace Muster.Windows;

internal sealed record TranscriptLink(int Line, int Start, int Length, string Destination);

internal static class MarkdownLinks
{
    sealed record Parsed(string Text, List<TranscriptLink> Links);
    static readonly ConditionalWeakTable<string, Parsed> cache = new();
    static readonly MarkdownPipeline pipeline = new MarkdownPipelineBuilder().UsePreciseSourceLocation().Build();

    public static string Display(string text, string prefix, int line, List<TranscriptLink> links)
    {
        if (!text.Contains('[') && !text.Contains('<')) return prefix + text;
        var parsed = cache.GetValue(text, Parse);
        foreach (var link in parsed.Links)
            links.Add(link with { Line = line, Start = prefix.Length + link.Start });
        return prefix + parsed.Text;
    }

    static Parsed Parse(string text)
    {
        string source = text.ReplaceLineEndings("\n");
        var result = new StringBuilder();
        List<TranscriptLink> links = [];
        int next = 0;
        foreach (var node in Markdown.Parse(source, pipeline).Descendants())
        {
            if (node.Span.Start < next) continue; // A link can wrap an image.
            string? url;
            string label;
            switch (node)
            {
                case LinkInline link when !string.IsNullOrWhiteSpace(link.Url):
                    url = link.Url;
                    label = string.Concat(link.Descendants().Select(child => child switch {
                        LiteralInline literal => literal.Content.ToString(),
                        CodeInline code => code.Content,
                        LineBreakInline => " ",
                        _ => ""
                    }));
                    break;
                case AutolinkInline auto:
                    url = auto.IsEmail ? "mailto:" + auto.Url : auto.Url;
                    label = auto.Url;
                    break;
                default:
                    continue;
            }
            if (string.IsNullOrWhiteSpace(label)) label = url;
            result.Append(source.AsSpan(next, node.Span.Start - next));
            // Keep adjacent native link runs distinct for RichEdit hit testing.
            if (links.Count > 0 && links[^1].Start + links[^1].Length == result.Length) result.Append(' ');
            links.Add(new(0, result.Length, label.Length, url));
            result.Append(label);
            next = node.Span.End + 1;
        }
        if (links.Count == 0) return new(text, links);
        return new(result.Append(source.AsSpan(next)).ToString(), links);
    }
}

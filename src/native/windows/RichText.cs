using System.Globalization;
using System.Text;

namespace Muster.Windows;

internal static class RichText
{
    public static void AppendLiteral(StringBuilder rtf, ReadOnlySpan<char> line)
    {
        foreach (char c in line)
        {
            if (c == '\t') rtf.Append(@"\tab ");
            else if (c is '\\' or '{' or '}') rtf.Append('\\').Append(c);
            else if (c is >= ' ' and <= '~') rtf.Append(c);
            else rtf.Append(@"\u").Append(unchecked((short)c).ToString(CultureInfo.InvariantCulture)).Append('?');
        }
    }
}

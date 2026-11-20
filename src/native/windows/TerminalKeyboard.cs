using System.Runtime.InteropServices;
using Muster.App;

namespace Muster.Windows;

internal partial class TerminalGrid
{
    char? highSurrogate;
    bool composing;

    void ResetKeyboard() { highSurrogate = null; composing = false; }
    protected override void OnLostFocus(EventArgs e) { ResetKeyboard(); base.OnLostFocus(e); }

    internal void PasteText(string text)
    {
        if (view is { Ended: false, CanPaste: true } && text.Length > 0)
            Intent?.Invoke(new() { TerminalPaste = new() { Text = text } });
    }
    public void PasteClipboard()
    {
        if (view is not { Ended: false, CanPaste: true }) return;
        try { PasteText(Clipboard.GetText()); }
        catch (ExternalException e) { ClipboardError?.Invoke(e.Message); }
    }
    void CopySelection()
    {
        string text = SelectedText();
        if (text.Length == 0) return;
        try { Clipboard.SetText(text); }
        catch (ExternalException e) { ClipboardError?.Invoke(e.Message); }
    }

    static string? KeyName(Keys data)
    {
        Keys code = data & Keys.KeyCode;
        bool ctrl = data.HasFlag(Keys.Control), alt = data.HasFlag(Keys.Alt), shift = data.HasFlag(Keys.Shift);
        string? key = code switch
        {
            Keys.Return => "enter", Keys.Tab => "tab", Keys.Back => "backspace",
            Keys.Escape => "escape", Keys.Up => "up", Keys.Down => "down", Keys.Left => "left", Keys.Right => "right",
            Keys.Home => "home", Keys.End => "end", Keys.Insert => "insert", Keys.Delete => "delete",
            Keys.PageUp => "pageup", Keys.PageDown => "pagedown",
            >= Keys.F1 and <= Keys.F12 => "f" + (1 + code - Keys.F1),
            // Windows delivers AltGr text through WM_CHAR with Ctrl+Alt held.
            >= Keys.A and <= Keys.Z when ctrl != alt => ((char)('a' + (code - Keys.A))).ToString(),
            >= Keys.D2 and <= Keys.D8 when ctrl && !alt => ((char)('2' + (code - Keys.D2))).ToString(),
            Keys.Space when ctrl && !alt => "space",
            _ => null
        };
        return key == null ? null : (ctrl ? "ctrl-" : "") + (alt ? "alt-" : "") + (shift ? "shift-" : "") + key;
    }

    protected override bool ProcessCmdKey(ref Message msg, Keys data)
    {
        if (composing || data == (Keys.Alt | Keys.F4) || data == (Keys.Alt | Keys.Space))
            return base.ProcessCmdKey(ref msg, data);
        if (data == (Keys.Control | Keys.Tab) || data == (Keys.Control | Keys.Shift | Keys.Tab))
            return FindForm()?.SelectNextControl(this, !data.HasFlag(Keys.Shift), true, true, true) == true;
        if (data == (Keys.Control | Keys.Shift | Keys.C) || data == (Keys.Control | Keys.C) && anchor != head)
        {
            CopySelection();
            return true;
        }
        if (data == (Keys.Control | Keys.Shift | Keys.V) || data == (Keys.Shift | Keys.Insert))
        {
            PasteClipboard();
            return true;
        }
        if (KeyName(data) is not {} key) return base.ProcessCmdKey(ref msg, data);
        if (view is { Ended: false }) Intent?.Invoke(new() { TerminalKey = new() { Key = key } });
        return true;
    }

    void Character(char ch)
    {
        if (view is not { Ended: false } || !Focused) return;
        char? previous = highSurrogate;
        highSurrogate = null;
        if (char.IsHighSurrogate(ch)) { highSurrogate = ch; return; }
        if (char.IsControl(ch) || char.IsLowSurrogate(ch) && previous == null) return;
        string text = char.IsLowSurrogate(ch) ? new string(new[] { previous!.Value, ch }) : ch.ToString();
        Intent?.Invoke(new() { TerminalText = new() { Text = text } });
    }

    protected override void WndProc(ref Message m)
    {
        const int wmChar = 0x0102, wmImeStart = 0x010d, wmImeEnd = 0x010e, wmImeChar = 0x0286, wmPaste = 0x0302;
        switch (m.Msg)
        {
            case wmImeStart: composing = true; break;
            case wmImeEnd: composing = false; break;
            case wmChar:
            case wmImeChar:
                Character((char)(int)m.WParam);
                return;
            case wmPaste:
                PasteClipboard();
                return;
        }
        base.WndProc(ref m);
    }
}

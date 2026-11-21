using System.Runtime.InteropServices;

namespace Muster.Windows;

internal class NamedRichTextBox : RichTextBox
{
    public NamedRichTextBox(string name) => AccessibleName = name;

    [DllImport("user32.dll", EntryPoint = "SendMessageW", CharSet = CharSet.Unicode)]
    static extern nint SetName(nint window, uint message, nint unused, string name);

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        // RichEdit provides its own UIA Document and otherwise guesses a name from nearby controls.
        SetName(Handle, 0x0400 + 320, 0, AccessibleName!); // EM_SETUIANAME
    }
}

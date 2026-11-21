using System.Runtime.InteropServices;
using Muster.App;

namespace Muster.Windows;

internal sealed class JumpToLatest : Button
{
    [StructLayout(LayoutKind.Sequential)]
    struct ScrollInfo
    {
        public uint Size, Mask;
        public int Minimum, Maximum;
        public uint Page;
        public int Position, TrackPosition;
    }

    [DllImport("user32.dll")]
    static extern bool GetScrollInfo(nint window, int bar, ref ScrollInfo info);

    [DllImport("user32.dll", EntryPoint = "SendMessageW")]
    static extern nint SendMessage(nint window, uint message, nint action, nint unused);

    TextBoxBase? reader;
    (string Device, string Session)? scope;
    bool hasNewer, loading, waitingForLatest;
    public event Action<Intent>? Intent;

    public JumpToLatest()
    {
        Text = "Jump to latest";
        AutoSize = true;
        Visible = false;
    }

    public void Display(ChatView? chat, TextBoxBase? transcript)
    {
        (string, string)? next = chat?.Session is {} session ? (chat.DeviceId, session.Id) : null;
        if (next != scope) waitingForLatest = false;
        scope = next;
        reader = next == null ? null : transcript;
        hasNewer = chat?.HasNewer == true;
        loading = chat?.Loading == true;
        if (waitingForLatest && !hasNewer)
        {
            waitingForLatest = false;
            ScrollToEnd();
        }
        UpdateAvailability();
    }

    public void UpdateAvailability()
    {
        bool show = false;
        if (reader is { IsHandleCreated: true, Visible: true })
        {
            // SB_VERT, with range, page size and current thumb position.
            var info = new ScrollInfo { Size = (uint)Marshal.SizeOf<ScrollInfo>(), Mask = 7 };
            show = hasNewer || (GetScrollInfo(reader.Handle, 1, ref info) && info.Page > 0 &&
                info.Position < (long)info.Maximum - info.Page + 1);
        }
        Visible = show;
        Enabled = !hasNewer || !loading;
    }

    protected override void OnClick(EventArgs e)
    {
        base.OnClick(e);
        if (reader == null || !Enabled) return;
        waitingForLatest = hasNewer;
        reader.Focus();
        ScrollToEnd();
        Intent?.Invoke(new Intent { AgentLoadEarlier = new() { Latest = true } });
        UpdateAvailability();
    }

    void ScrollToEnd()
    {
        if (reader is { IsHandleCreated: true })
            SendMessage(reader.Handle, 0x0115, 7, 0); // WM_VSCROLL / SB_BOTTOM preserves selection.
    }
}

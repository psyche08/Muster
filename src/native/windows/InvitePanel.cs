using Muster.App;
using ZXing;
using ZXing.Common;
using ZXing.QrCode;

namespace Muster.Windows;

internal sealed class InvitePanel : UserControl
{
    readonly Panel qr = new() { Name = "InviteQR", Dock = DockStyle.Fill, BackColor = Color.White, AccessibleName = "Network invite QR code" };
    readonly TextBox text = new() { Name = "InviteText", Dock = DockStyle.Fill, ReadOnly = true, Multiline = true, ScrollBars = ScrollBars.Vertical,
        AccessibleName = "Invitation text" };
    readonly Label expiry = new() { Name = "InviteExpiry", AutoSize = true };
    readonly Button copy = new() { Name = "CopyInvite", Text = "Copy invite", AutoSize = true };
    readonly Button renew = new() { Name = "RenewInvite", Text = "New invite", AutoSize = true };
    readonly System.Windows.Forms.Timer clock = new() { Interval = 1000 };

    BitMatrix? matrix;
    ulong expires;

    public InvitePanel(Action<string> copyText, Action newInvite)
    {
        Visible = false;
        Dock = DockStyle.Fill;
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(8), ColumnCount = 1, RowCount = 5 };
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        layout.RowStyles.Add(new RowStyle(SizeType.Absolute, text.Font.Height * 4 + 12));
        for (int i = 2; i < 5; i++) layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.Controls.Add(qr, 0, 0);
        layout.Controls.Add(text, 0, 1);
        layout.Controls.Add(expiry, 0, 2);
        var buttons = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoSize = true };
        buttons.Controls.Add(copy);
        buttons.Controls.Add(renew);
        layout.Controls.Add(buttons, 0, 3);
        layout.Controls.Add(new Label { Text = "Scan on a phone or paste on a desktop. Approve the joining device afterwards.", AutoSize = true }, 0, 4);
        Controls.Add(layout);
        qr.Paint += PaintQr;
        qr.Resize += (_, _) => qr.Invalidate();
        copy.Click += (_, _) =>
        {
            try { copyText(text.Text); copy.Text = "Copied"; }
            catch (System.Runtime.InteropServices.ExternalException) { copy.Text = "Clipboard busy — retry"; }
        };
        renew.Click += (_, _) => newInvite();
        clock.Tick += (_, _) => UpdateExpiry();
    }

    internal void Display(InviteView? invite, bool active)
    {
        var payload = active ? invite?.Text ?? "" : "";
        if (text.Text != payload)
        {
            matrix = null;
            if (payload.Length > 0)
            {
                try { matrix = new QRCodeWriter().encode(payload, BarcodeFormat.QR_CODE, 0, 0); }
                catch (WriterException) { /* The text can still be copied. */ }
            }
            text.Text = payload;
            copy.Text = "Copy invite";
            qr.Invalidate();
        }
        expires = invite?.ExpiresUnix ?? 0;
        copy.Visible = renew.Visible = text.Visible = payload.Length > 0;
        Visible = active;
        UpdateExpiry();
    }

    void UpdateExpiry()
    {
        ulong now = (ulong)DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        ulong left = expires > now ? expires - now : 0;
        expiry.Text = text.TextLength == 0 ? "Preparing the invite…" :
            expires == 0 ? "Expiry not reported" : left == 0 ? "Expired" : $"Expires in {left / 60}:{left % 60:00}";
        if (text.TextLength > 0 && matrix == null) expiry.Text += " · QR unavailable; use the invitation text";
        clock.Enabled = Visible && text.TextLength > 0 && left > 0;
    }

    void PaintQr(object? sender, PaintEventArgs e)
    {
        if (matrix == null) return;
        int scale = Math.Min(qr.ClientSize.Width, qr.ClientSize.Height) / matrix.Width;
        if (scale < 1) return;
        int x0 = (qr.ClientSize.Width - matrix.Width * scale) / 2;
        int y0 = (qr.ClientSize.Height - matrix.Height * scale) / 2;
        for (int y = 0; y < matrix.Height; y++)
            for (int x = 0; x < matrix.Width; x++)
                if (matrix[x, y]) e.Graphics.FillRectangle(Brushes.Black, x0 + x * scale, y0 + y * scale, scale, scale);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) clock.Dispose();
        base.Dispose(disposing);
    }
}

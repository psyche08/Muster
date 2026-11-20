using Muster.App;
using Muster.Windows;
using ZXing;

internal static class InvitePanelTests
{
    public static void Run()
    {
        string? copied = null;
        int renewed = 0;
        using var host = new Form { ClientSize = new Size(560, 500), ShowInTaskbar = false, Opacity = 0 };
        using var panel = new InvitePanel(value => copied = value, () => renewed++);
        host.Controls.Add(panel);
        host.Show();

        T Field<T>(string name) where T : Control => (T)panel.Controls.Find(name, true).Single();
        var qr = Field<Panel>("InviteQR");
        var copy = Field<Button>("CopyInvite");
        var renew = Field<Button>("RenewInvite");
        var expiry = Field<Label>("InviteExpiry");
        var original = "muster1:" + new string('a', 420);
        var invite = new InviteView { Text = original, ExpiresUnix = (ulong)DateTimeOffset.UtcNow.ToUnixTimeSeconds() + 600 };
        panel.Display(invite, true);
        host.PerformLayout();
        using var bitmap = new Bitmap(qr.Width, qr.Height);
        qr.DrawToBitmap(bitmap, qr.ClientRectangle);
        var pixels = new byte[bitmap.Width * bitmap.Height];
        for (int y = 0; y < bitmap.Height; y++)
            for (int x = 0; x < bitmap.Width; x++)
                pixels[y * bitmap.Width + x] = bitmap.GetPixel(x, y).R;
        var read = new BarcodeReaderGeneric().Decode(new RGBLuminanceSource(
            pixels, bitmap.Width, bitmap.Height, RGBLuminanceSource.BitmapFormat.Gray8));
        if (read?.Text != original || !expiry.Text.StartsWith("Expires in "))
            throw new InvalidOperationException("Rendered invite or expiry differs");
        copy.PerformClick();
        renew.PerformClick();
        if (copied != original || renewed != 1) throw new InvalidOperationException("Invite actions differ");

        panel.Display(null, true);
        if (Field<TextBox>("InviteText").TextLength != 0 || copy.Visible || expiry.Text != "Preparing the invite…")
            throw new InvalidOperationException("Preparing retained prior invite");
        invite.Text = "muster1:replacement";
        invite.ExpiresUnix = 1;
        panel.Display(invite, true);
        copy.PerformClick();
        if (copied != invite.Text || expiry.Text != "Expired") throw new InvalidOperationException("Stale invite");
        panel.Display(invite, false);
        if (panel.Visible || Field<TextBox>("InviteText").TextLength != 0)
            throw new InvalidOperationException("Inactive invite retained text");
    }
}

using Google.Protobuf;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Graphics.Imaging;

namespace Muster.Windows;

internal static class AttachmentPreview
{
    public static async Task<Bitmap> Decode(ByteString bytes, uint longestSide)
    {
        using var source = new MemoryStream(bytes.ToByteArray(), writable: false);
        using var stream = source.AsRandomAccessStream();
        var decoder = await BitmapDecoder.CreateAsync(stream);
        double scale = Math.Min(1, (double)longestSide / Math.Max(decoder.PixelWidth, decoder.PixelHeight));
        var transform = new BitmapTransform {
            ScaledWidth = Math.Max(1, (uint)(decoder.PixelWidth * scale)),
            ScaledHeight = Math.Max(1, (uint)(decoder.PixelHeight * scale))
        };
        using var decoded = await decoder.GetSoftwareBitmapAsync(BitmapPixelFormat.Bgra8,
            BitmapAlphaMode.Premultiplied, transform, ExifOrientationMode.RespectExifOrientation,
            ColorManagementMode.ColorManageToSRgb);
        var pixels = new byte[checked(decoded.PixelWidth * decoded.PixelHeight * 4)];
        decoded.CopyToBuffer(pixels.AsBuffer());
        var image = new Bitmap(decoded.PixelWidth, decoded.PixelHeight, PixelFormat.Format32bppPArgb);
        try
        {
            var bitmap = image.LockBits(new Rectangle(Point.Empty, image.Size), ImageLockMode.WriteOnly, image.PixelFormat);
            try { Marshal.Copy(pixels, 0, bitmap.Scan0, pixels.Length); }
            finally { image.UnlockBits(bitmap); }
            return image;
        }
        catch { image.Dispose(); throw; }
    }
}

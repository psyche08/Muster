using Microsoft.Win32.SafeHandles;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace Muster.Windows;

internal static class Presence
{
    const string KeyName = "Muster.Desktop.Presence.P256.v1";
    static readonly CngProvider Provider = CngProvider.MicrosoftPlatformCryptoProvider;
    const int Silent = 0x40;
    const int SilentContext = unchecked((int)0x80090022);
    const int UiRequired = unchecked((int)0x8009002e);

    [DllImport("ncrypt.dll")]
    static extern int NCryptSignHash(SafeNCryptKeyHandle key, nint padding, byte[] hash, int hashLength,
                                     byte[] signature, int capacity, out int written, int flags);

    // Some TPM providers ignore an unsupported UI policy. A public candidate
    // may only be registered if an independent, silent handle refuses to sign.
    static void Validate(CngKey key)
    {
        if (key.Provider != Provider || key.Algorithm != CngAlgorithm.ECDsaP256 ||
            key.ExportPolicy != CngExportPolicies.None ||
            (key.UIPolicy.ProtectionLevel & CngUIProtectionLevels.ForceHighProtection) == 0)
            throw new CryptographicException("The TPM did not retain mandatory interactive P-256 key protection.");
        using var handle = key.Handle;
        byte[] probe = RandomNumberGenerator.GetBytes(32);
        int result = NCryptSignHash(handle, 0, probe, probe.Length, new byte[64], 64, out _, Silent);
        if (result != SilentContext && result != UiRequired)
            throw new CryptographicException(result == 0 ? "The TPM permits silent signing with this key." :
                                            $"Could not verify interactive TPM protection (0x{result:X8}).");
    }

    static byte[] Public(CngKey key)
    {
        using var signer = new ECDsaCng(key);
        var point = signer.ExportParameters(false).Q;
        if (point.X?.Length != 32 || point.Y?.Length != 32)
            throw new CryptographicException("Unexpected TPM public key.");
        return [4, .. point.X, .. point.Y];
    }

    public static nint CopyCandidate(nint context, nint output, nuint capacity)
    {
        try
        {
            using var key = CngKey.Open(KeyName, Provider, CngKeyOpenOptions.Silent);
            Validate(key);
            var pub = Public(key);
            if (capacity < (nuint)pub.Length) return -1;
            Marshal.Copy(pub, 0, output, pub.Length);
            return pub.Length;
        }
        catch { return -1; } // FFI callbacks must not unwind into Rust.
    }

    public static async Task Register(nint parent)
    {
        byte[] candidate = await Task.Run(() => {
            if (!CngKey.Exists(KeyName, Provider))
            {
                var parameters = new CngKeyCreationParameters {
                    Provider = Provider, KeyUsage = CngKeyUsages.Signing,
                    ExportPolicy = CngExportPolicies.None, ParentWindowHandle = parent,
                    UIPolicy = new CngUIPolicy(CngUIProtectionLevels.ForceHighProtection,
                        "Muster presence", "Confirm each protected Muster operation.", null, "Muster")
                };
                using var created = CngKey.Create(CngAlgorithm.ECDsaP256, KeyName, parameters);
            }
            using var key = CngKey.Open(KeyName, Provider, CngKeyOpenOptions.Silent);
            Validate(key);
            return Public(key);
        });
        var info = new ProcessStartInfo(Path.Combine(AppContext.BaseDirectory, "cli", "muster.exe"))
            { UseShellExecute = false, CreateNoWindow = true, RedirectStandardError = true };
        info.ArgumentList.Add("presence");
        info.ArgumentList.Add("--register");
        info.ArgumentList.Add(Convert.ToBase64String(candidate));
        using var cli = Process.Start(info) ?? throw new InvalidOperationException("Could not register the presence key.");
        string error = await cli.StandardError.ReadToEndAsync();
        await cli.WaitForExitAsync();
        if (cli.ExitCode != 0) throw new InvalidOperationException(error);
    }

    public static byte[] Sign(nint parent, string reason, byte[] message)
    {
        if (!message.AsSpan().StartsWith("MUSTER-PRESENCE-V1"u8) || message.Length <= "MUSTER-PRESENCE-V1"u8.Length + 8)
            throw new CryptographicException("Invalid Muster presence challenge.");
        using (var probe = CngKey.Open(KeyName, Provider, CngKeyOpenOptions.Silent)) Validate(probe);
        using var key = CngKey.Open(KeyName, Provider);
        key.ParentWindowHandle = parent;
        key.SetProperty(new CngProperty("Use Context", System.Text.Encoding.Unicode.GetBytes(reason + "\0"),
                                        CngPropertyOptions.None));
        using var signer = new ECDsaCng(key);
        return signer.SignData(message, HashAlgorithmName.SHA256, DSASignatureFormat.Rfc3279DerSequence);
    }
}

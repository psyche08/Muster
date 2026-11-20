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
    static byte[] ValidatedPublicKey(CngKey key)
    {
        if (key.Provider != Provider || key.AlgorithmGroup != CngAlgorithmGroup.ECDsa ||
            key.KeyUsage != CngKeyUsages.Signing || key.ExportPolicy != CngExportPolicies.None ||
            (key.UIPolicy.ProtectionLevel & CngUIProtectionLevels.ForceHighProtection) == 0)
            throw new CryptographicException("The TPM did not retain mandatory interactive P-256 key protection.");
        // PCP can reopen an ECDSA_P256 key under the generic ECDSA name.
        using var signer = new ECDsaCng(key);
        var parameters = signer.ExportParameters(false);
        var point = parameters.Q;
        if (parameters.Curve.Oid.Value != "1.2.840.10045.3.1.7" ||
            point.X?.Length != 32 || point.Y?.Length != 32)
            throw new CryptographicException("The TPM presence key must use the NIST P-256 curve.");
        using var handle = key.Handle;
        byte[] probe = RandomNumberGenerator.GetBytes(32);
        int result = NCryptSignHash(handle, 0, probe, probe.Length, new byte[64], 64, out _, Silent);
        if (result != SilentContext && result != UiRequired)
            throw new CryptographicException(result == 0 ? "The TPM permits silent signing with this key." :
                                            $"Could not verify interactive TPM protection (0x{result:X8}).");
        return [4, .. point.X, .. point.Y];
    }

    public static nint CopyCandidate(nint context, nint output, nuint capacity)
    {
        try
        {
            using var key = CngKey.Open(KeyName, Provider, CngKeyOpenOptions.Silent);
            var pub = ValidatedPublicKey(key);
            if (capacity < (nuint)pub.Length) return -1;
            Marshal.Copy(pub, 0, output, pub.Length);
            return pub.Length;
        }
        catch { return -1; } // FFI callbacks must not unwind into Rust.
    }

    public static async Task Register(nint parent, string socket)
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
            return ValidatedPublicKey(key);
        });
        var info = new ProcessStartInfo(Path.Combine(AppContext.BaseDirectory, "cli", "muster.exe"))
            { UseShellExecute = false, CreateNoWindow = true, RedirectStandardError = true };
        info.ArgumentList.Add("--socket");
        info.ArgumentList.Add(socket);
        info.ArgumentList.Add("presence");
        info.ArgumentList.Add("--register");
        info.ArgumentList.Add(Convert.ToBase64String(candidate));
        using var cli = Process.Start(info) ?? throw new InvalidOperationException("Could not register the presence key.");
        string error = await cli.StandardError.ReadToEndAsync();
        await cli.WaitForExitAsync();
        if (cli.ExitCode != 0) throw new InvalidOperationException(error);
    }

    [DllImport("muster_ffi.dll", CallingConvention = CallingConvention.Cdecl)]
    static extern byte muster_is_presence_challenge(byte[] message, nuint length);

    public static byte[] Sign(nint parent, string reason, byte[] message)
    {
        // The shared core validates and encodes the requested operation, including
        // both RPC presence and domain-separated Locked Use challenges.
        if (string.IsNullOrWhiteSpace(reason) || muster_is_presence_challenge(message, (nuint)message.Length) != 1)
            throw new CryptographicException("Invalid Muster presence challenge.");
        using (var probe = CngKey.Open(KeyName, Provider, CngKeyOpenOptions.Silent)) ValidatedPublicKey(probe);
        using var key = CngKey.Open(KeyName, Provider);
        key.ParentWindowHandle = parent;
        key.SetProperty(new CngProperty("Use Context", System.Text.Encoding.Unicode.GetBytes(reason + "\0"),
                                        CngPropertyOptions.None));
        using var signer = new ECDsaCng(key);
        return signer.SignData(message, HashAlgorithmName.SHA256, DSASignatureFormat.Rfc3279DerSequence);
    }
}

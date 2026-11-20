using Google.Protobuf;
using Muster.App;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text.Json;

[assembly:DefaultDllImportSearchPaths(DllImportSearchPath.ApplicationDirectory | DllImportSearchPath.System32)]

namespace Muster.Windows;

internal sealed class Core : IDisposable
{
    const string Library = "muster_ffi.dll";
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void ViewCallback(nint ctx, nint bytes, nuint length);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void EffectCallback(nint ctx, ulong id, nint bytes, nuint length);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    delegate nint PresenceCallback(nint ctx, nint bytes, nuint capacity);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    delegate int ServiceCallback(uint action);
    [StructLayout(LayoutKind.Sequential)]
    struct Platform
    {
        public nint Context;
        public ViewCallback View;
        public EffectCallback Effect;
        public nint Key, Sign;
        public PresenceCallback Presence;
    }

    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern int muster_desktop_check([MarshalAs(UnmanagedType.LPUTF8Str)] string socket, byte managed,
                                           byte resumed, ServiceCallback inspect, out nint error);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern int muster_desktop_prepare([MarshalAs(UnmanagedType.LPUTF8Str)] string socket, byte managed,
                                             ServiceCallback start, out nint error);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern nint muster_core_new(byte[] config, nuint length, ref Platform platform, out nint error);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern void muster_log_init_file(uint level);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern void muster_string_free(nint value);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern void muster_core_intent(nint core, byte[] data, nuint length);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern void muster_core_effect_result(nint core, ulong id, byte[] data, nuint length);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern void muster_core_lifecycle(nint core, uint state);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
    static extern void muster_core_free(nint core);
    Platform platform;
    nint handle;
    public Core(Action<ViewModel> view, Action<ulong, Effect> effect)
    {
        muster_log_init_file(3);
        platform.Presence = Presence.CopyCandidate;
        platform.View = (_, p, n) => view(ViewModel.Parser.ParseFrom(Copy(p, n)));
        platform.Effect = (_, id, p, n) => effect(id, Effect.Parser.ParseFrom(Copy(p, n)));
    }
    static byte[] Copy(nint pointer, nuint length)
    {
        byte[] bytes = new byte[checked((int)length)];
        Marshal.Copy(pointer, bytes, 0, bytes.Length);
        return bytes;
    }
    static Exception Error(nint pointer)
    {
        try
        {
            return new InvalidOperationException(Marshal.PtrToStringUTF8(pointer) ?? "Muster failed to start.");
        }
        finally
        {
            if (pointer != 0)
                muster_string_free(pointer);
        }
    }
    static string StateDirectory => Environment.GetEnvironmentVariable("MUSTER_DIR") ??
                     Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Muster");
    static string DaemonPath => Path.Combine(AppContext.BaseDirectory, "musterd.exe");
    static int Inspect(uint action) => action == 3 && File.Exists(DaemonPath) ? 1 : -1;
    public bool NeedsStart()
    {
        int phase = muster_desktop_check(Path.Combine(StateDirectory, "musterd.sock"), 1, 0, Inspect, out var error);
        return phase switch
        {
            0 => false,
            1 => true,
            -1 => throw Error(error),
            _ => throw new InvalidOperationException($"Unexpected desktop startup phase {phase}")
        };
    }
    public void Open()
    {
        string dir = StateDirectory;
        string socket = Path.Combine(dir, "musterd.sock");
        using var owned = new Process { StartInfo = new ProcessStartInfo(
                                            DaemonPath,
                                            "--log") { UseShellExecute = false, CreateNoWindow = true } };
        bool started = false;
        int Start(uint action)
        {
            try
            {
                // Inspect describes the installed daemon without launching it.
                if (action == 3)
                    return Inspect(action);
                if (action != 0 && action != 1)
                    return 2;
                if (started)
                {
                    if (!owned.HasExited)
                        return 0;
                    if (action != 1)
                        return 2;
                }
                started = owned.Start();
                return started ? 0 : 2;
            }
            catch
            {
                return 2;
            }
        }
        if (muster_desktop_prepare(socket, 1, Start, out var error) != 0)
            throw Error(error);
        byte[] config = JsonSerializer.SerializeToUtf8Bytes(new { dir, socket, mode = "socket" });
        handle = muster_core_new(config, (nuint)config.Length, ref platform, out error);
        if (handle == 0)
            throw Error(error);
    }
    public void Send(Intent intent)
    {
        if (handle != 0)
        {
            var b = intent.ToByteArray();
            muster_core_intent(handle, b, (nuint)b.Length);
        }
    }
    public void Answer(ulong id, EffectResult result)
    {
        if (id != 0 && handle != 0)
        {
            var b = result.ToByteArray();
            muster_core_effect_result(handle, id, b, (nuint)b.Length);
        }
    }
    public void Lifecycle(uint state)
    {
        if (handle != 0)
            muster_core_lifecycle(handle, state);
    }
    public void Dispose()
    {
        if (handle != 0)
        {
            Lifecycle(3);
            muster_core_free(handle);
            handle = 0;
        }
        GC.KeepAlive(platform);
    }
}

using Microsoft.Win32;
using System.Runtime.InteropServices;
using System.Security;

namespace Muster.Windows;

internal sealed class PowerMenu : MenuStrip
{
    internal const string PreferenceKey = @"HKEY_CURRENT_USER\Software\Muster\Desktop";
    const string PreferenceValue = "KeepAwakeFlags";
    const int SystemFlag = 1, DisplayFlag = 2;
    const uint Continuous = 0x80000000;

    [DllImport("kernel32.dll")]
    static extern uint SetThreadExecutionState(uint flags);

    readonly ToolStripMenuItem awake = new("Keep Awake");
    readonly ToolStripMenuItem display = new("Keep Screen On");
    readonly ToolStripMenuItem error = new() { Enabled = false, Visible = false };
    readonly string key;
    int choice = SystemFlag;
    uint originalState;

    internal PowerMenu(string key)
    {
        this.key = key;
        var power = new ToolStripMenuItem("&Power");
        Items.Add(power);
        power.DropDownItems.AddRange([awake, display, error]);
        awake.Click += (_, _) => Apply(choice ^ SystemFlag, save: true);
        display.Click += (_, _) => Apply(choice ^ DisplayFlag, save: true);

        try
        {
            if (Registry.GetValue(key, PreferenceValue, null) is int flags && flags is >= 0 and <= 3)
                choice = flags;
        }
        catch (Exception ex) when (IsPreferenceError(ex))
        {
            ShowError("Could not read power preference: " + ex.Message);
        }
        Apply(choice, save: false);
    }

    static bool IsPreferenceError(Exception ex) => ex is IOException or SecurityException or UnauthorizedAccessException;

    void ShowError(string message)
    {
        error.Text = message;
        error.Visible = true;
    }

    void Apply(int next, bool save)
    {
        bool keepAwake = (next & SystemFlag) != 0;
        uint previous = SetThreadExecutionState(Continuous | (keepAwake ? (uint)next : 0));
        if (previous == 0)
        {
            ShowError("Windows could not apply the power request.");
            display.Enabled = awake.Checked;
            return;
        }
        if (originalState == 0) originalState = previous;
        choice = next;
        awake.Checked = keepAwake;
        display.Checked = (next & DisplayFlag) != 0;
        display.Enabled = keepAwake;
        if (!save) return;
        try
        {
            Registry.SetValue(key, PreferenceValue, next, RegistryValueKind.DWord);
            error.Visible = false;
        }
        catch (Exception ex) when (IsPreferenceError(ex))
        {
            ShowError("Applied for this run. Could not save power preference: " + ex.Message);
        }
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing && originalState != 0)
        {
            SetThreadExecutionState(originalState);
            originalState = 0;
        }
        base.Dispose(disposing);
    }
}

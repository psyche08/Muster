using System.Diagnostics;
using Microsoft.Win32;
using Muster.App;

namespace Muster.Windows;

internal sealed class BrowserPanel : UserControl
{
    readonly Label connection = new() { AutoSize = true, Name = "BrowserConnection" };
    readonly Label guidance = new() { AutoSize = true };
    readonly TextBox pathBox = new() { AccessibleName = "Extension folder", ReadOnly = true, Dock = DockStyle.Fill };
    readonly TextBox request = new() { Name = "BrowserRequest", AccessibleName = "Browser request",
        Multiline = true, ScrollBars = ScrollBars.Vertical, Dock = DockStyle.Fill };
    readonly TextBox result = new() { Name = "BrowserResult", AccessibleName = "Browser result", ReadOnly = true,
        Multiline = true, ScrollBars = ScrollBars.Vertical, Dock = DockStyle.Fill };
    readonly Button folder, extensions, run;
    readonly FlowLayoutPanel setup = Buttons(), operations = Buttons();
    readonly Action<ProcessStartInfo> start;
    BrowserView? current;
    public event Action<Intent>? Intent;

    static FlowLayoutPanel Buttons() => new() { AutoSize = true, Dock = DockStyle.Fill, WrapContents = true };
    static Button Add(FlowLayoutPanel row, string text, Action click)
    {
        var button = new Button { Text = text, AutoSize = true, UseMnemonic = false };
        button.Click += (_, _) => click();
        row.Controls.Add(button);
        return button;
    }

    public BrowserPanel() : this(info => { using var process = Process.Start(info); }) { }

    internal BrowserPanel(Action<ProcessStartInfo> start)
    {
        this.start = start;
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 1, RowCount = 9, AutoScroll = true };
        layout.ColumnStyles.Add(new(SizeType.Percent, 100));
        for (int i = 0; i < layout.RowCount; i++)
            layout.RowStyles.Add(i is 5 or 8 ? new(SizeType.Percent, i == 5 ? 40 : 60) : new(SizeType.AutoSize));
        Controls.Add(layout);
        layout.Controls.Add(connection, 0, 0);
        layout.Controls.Add(guidance, 0, 1);
        layout.Controls.Add(pathBox, 0, 2);
        layout.Controls.Add(setup, 0, 3);
        layout.Controls.Add(new Label { Text = "Browser request", AutoSize = true }, 0, 4);
        layout.Controls.Add(request, 0, 5);
        layout.Controls.Add(operations, 0, 6);
        layout.Controls.Add(new Label { Text = "Result", AutoSize = true }, 0, 7);
        layout.Controls.Add(result, 0, 8);
        layout.SizeChanged += (_, _) =>
        {
            int width = Math.Max(1, layout.ClientSize.Width - guidance.Margin.Horizontal - SystemInformation.VerticalScrollBarWidth);
            guidance.MaximumSize = connection.MaximumSize = new Size(width, 0);
        };
        folder = Add(setup, "Show extension folder", ShowFolder);
        extensions = Add(setup, "Open Chrome extensions", OpenExtensions);
        Add(setup, "Check connection", () => Intent?.Invoke(new() { BrowserSetup = new() }));
        Add(operations, "List tabs", () => request.Text = "{\"op\":\"tabs\"}");
        Add(operations, "Connection test", () => request.Text = "{\"op\":\"ping\"}");
        run = Add(operations, "Run", () => Intent?.Invoke(new() { BrowserRun = new() { Json = request.Text } }));
    }

    public void Display(BrowserView? next)
    {
        Visible = next != null;
        if (next == null) { current = null; return; }
        if (current == null || current.Device != next.Device || current.Draft != next.Draft)
            request.Text = next.Draft;
        current = next;
        string target = next.Local ? "Local Chrome" : "Remote Chrome";
        connection.Text = next.Busy ? $"{target} · Waiting for browser…" :
            next.Status == null ? $"{target} · Connection not reported" :
            $"{target} · {(next.Status.Connected ? "Connected" : "Setup needed")}\r\n{next.Status.Message}";
        guidance.Text = next.Local
            ? (next.Status?.Installed == false ? "Run Apply-Update.cmd from the extracted Muster package to register the native host. " : "") +
              "In Chrome extensions, turn on Developer mode, choose Load unpacked and select the AgentHaloBrowser folder."
            : "Complete Chrome extension setup on the target device, then check the connection here.";
        guidance.Visible = next.Status?.Connected != true;
        string path = next.Local ? next.Status?.ExtensionPath ?? "" : "";
        if (pathBox.Text != path) pathBox.Text = path;
        pathBox.Visible = path.Length > 0;
        folder.Visible = extensions.Visible = next.Local;
        folder.Enabled = !string.IsNullOrEmpty(next.Status?.ExtensionPath);
        setup.Enabled = operations.Enabled = request.Enabled = !next.Busy;
        run.Text = next.Local ? "Run" : "Run & authenticate";
        if (result.Text != next.Result) result.Text = next.Result;
    }

    void ShowFolder()
    {
        if (current is not { Local: true, Status.ExtensionPath: { Length: > 0 } path }) return;
        if (!Path.IsPathFullyQualified(path) || !Directory.Exists(path))
        {
            MessageBox.Show(this, "The reported extension folder is unavailable. Check the connection again.", "Chrome extension");
            return;
        }
        Launch(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Windows), "explorer.exe"), path);
    }

    void OpenExtensions()
    {
        if (current?.Local != true) return;
        string? chrome = FindChrome();
        if (chrome != null) Launch(chrome, "chrome://extensions/");
        else MessageBox.Show(this, "Open chrome://extensions/ in Google Chrome.", "Chrome extension");
    }

    internal static string? FindChrome()
    {
        const string key = @"Software\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe";
        foreach (var hive in new[] { Registry.CurrentUser, Registry.LocalMachine })
        {
            try
            {
                using var app = hive.OpenSubKey(key);
                if (app?.GetValue("") is not string value) continue;
                string registered = value.Trim('"');
                if (Path.IsPathFullyQualified(registered) &&
                    string.Equals(Path.GetFileName(registered), "chrome.exe", StringComparison.OrdinalIgnoreCase) && File.Exists(registered))
                    return registered;
            }
            catch (Exception ex) when (ex is UnauthorizedAccessException or System.Security.SecurityException or IOException)
            {
                // Continue with the standard installation directories if App Paths is inaccessible.
            }
        }
        foreach (var location in new[] { Environment.SpecialFolder.LocalApplicationData, Environment.SpecialFolder.ProgramFiles,
                                         Environment.SpecialFolder.ProgramFilesX86 })
        {
            string chrome = Path.Combine(Environment.GetFolderPath(location), "Google", "Chrome", "Application", "chrome.exe");
            if (File.Exists(chrome)) return chrome;
        }
        return null;
    }

    void Launch(string executable, string argument)
    {
        try
        {
            start(new ProcessStartInfo(executable) { UseShellExecute = false, ArgumentList = { argument } });
        }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or IOException or UnauthorizedAccessException)
        {
            MessageBox.Show(this, ex.Message, "Chrome extension");
        }
    }
}

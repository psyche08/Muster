#if os(macOS)
import SwiftUI
import ServiceManagement
import MusterCore

/// Where the installed Muster lives on a Mac, and its launchd daemon.
public enum MacInstall {
    public static let app = "/Applications/Muster.app"
    public static let daemon = app + "/Contents/MacOS/musterd"
    public static let daemonLabel = "dev.linsheng.musterd"
    public static var daemonService: String { "gui/\(getuid())/\(daemonLabel)" }
    /// This is the installed app, not a development build or a second
    /// daemon's ($MUSTER_DIR).
    public static var isInstalled: Bool {
        ProcessInfo.processInfo.environment["MUSTER_DIR"] == nil && Bundle.main.bundleURL.path == app
    }
}

/// OS-only service registration. Rust owns installation, retries and readiness.
public enum DesktopHelper {
    public static func register(openSettings: Bool) throws {
        guard MacInstall.isInstalled else {
            throw NSError(domain: "Muster", code: 1, userInfo: [NSLocalizedDescriptionKey: "请从 /Applications 启动 Muster。"])
        }
        let service = SMAppService.daemon(plistName: "dev.linsheng.muster.helper.plist")
        if service.status == .notRegistered || service.status == .notFound {
            try service.register()
        }
        if openSettings { SMAppService.openSystemSettingsLoginItems() }
    }

    /// At launch: registers the installed app's helper, logging a failure.
    public static func registerIfInstalled() {
        guard MacInstall.isInstalled else { return }
        do {
            try register(openSettings: false)
        } catch {
            NSLog("muster: root helper registration failed: %@", "\(error)")
        }
    }
}

struct DesktopSetupView: View {
    let setup: Muster_App_DesktopSetupView
    var busy = false
    let accessibility: () -> Void
    let install: () -> Void
    let iterm: () -> Void
    let later: () -> Void
    var refresh: () -> Void = {}
    var restart: () -> Void = {}

    /// The terminal app the Mac's terminal source is (Rust names it).
    private var terminalApp: String { setup.terminalApp.isEmpty ? "iTerm2" : setup.terminalApp }

    var body: some View {
        ScrollView {
        VStack(alignment: .leading, spacing: 22) {
            Label("完成 Mac 设置", systemImage: "desktopcomputer").font(.title2.bold())
            Text("从其他设备操作这台 Mac 的 Codex、Claude 和 \(terminalApp)，需要以下权限与设置。")
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 10) {
                Label("辅助功能权限", systemImage: setup.accessibilityReady ? "checkmark.circle.fill" : "exclamationmark.circle")
                    .font(.headline)
                if setup.accessibilityReady {
                    Text("musterd 已获授权。").foregroundStyle(.secondary)
                } else {
                    Text("在系统设置 → 隐私与安全性 → 辅助功能中启用 musterd。若未列出，点“＋”添加下方程序。")
                    Text(MacInstall.daemon)
                        .font(.caption.monospaced()).textSelection(.enabled)
                    Button("打开辅助功能设置…", action: accessibility).buttonStyle(.borderedProminent)
                }
            }
            Divider()
            VStack(alignment: .leading, spacing: 10) {
                Label("Locked Use 插件", systemImage: setup.pluginReady ? "checkmark.circle.fill" : "lock.shield")
                    .font(.headline)
                Text(setup.pluginMessage).foregroundStyle(.secondary)
                if setup.checking { ProgressView().controlSize(.small) }
                if setup.canInstall && !setup.pluginReady {
                    Button("授权并安装…", action: install).buttonStyle(.borderedProminent)
                    Text("通过 Muster 的系统 helper 自动安装，无需另开安装器或在终端运行 sudo。")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if setup.showIterm {
            Divider()
            VStack(alignment: .leading, spacing: 10) {
                Label(terminalApp == "iTerm2" ? "iTerm2 自动化与 API" : "\(terminalApp) 自动化", systemImage: setup.itermReady ? "checkmark.circle.fill" : "terminal")
                    .font(.headline)
                Text(setup.itermMessage).foregroundStyle(.secondary)
                if !setup.itermReady {
                    Button(busy ? "正在检查…" : "授权并检查 \(terminalApp)…", action: iterm).buttonStyle(.borderedProminent).disabled(busy)
                }
            }
            }
            if setup.restartRequired {
                Label("新授权需要重启 Muster 后生效", systemImage: "arrow.clockwise").font(.headline)
                Text("重启应用和后台服务，保留配对与数据。远程连接将短暂中断。")
                    .font(.callout).foregroundStyle(.secondary)
                Button("重启 Muster", action: restart).buttonStyle(.borderedProminent).disabled(!setup.canRestart)
            }
            if !setup.error.isEmpty { Label(setup.error, systemImage: "exclamationmark.circle").foregroundStyle(.red).font(.callout) }
            HStack {
                Text("完成后会自动更新状态。也可稍后从网络设置继续。").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("稍后", action: later).keyboardShortcut(.cancelAction)
            }
        }
        .padding(28)
        }.frame(width: 560, height: 640).background(Color(nsColor: .windowBackgroundColor))
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in refresh() }
    }
}
#endif

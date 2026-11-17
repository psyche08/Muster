// What a desktop app shows before its main page while musterd is not
// running, or runs from an older registration (ADR 0227). Shared Rust
// decides which; this view only words it and forwards the answer.
#if canImport(SwiftUI)
import SwiftUI

struct DesktopGateView: View {
    let gate: DesktopGate
    let busy: Bool
    let error: String?
    let start: () -> Void
    let later: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label(title, systemImage: symbol).font(.title2.bold())
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(line).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            if let error {
                Label(error, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 12) {
                Button(action: start) { Text(primary) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                if case .migrate = gate {
                    Button("暂不迁移", action: later)
                }
                if busy { ProgressView().controlSize(.small) }
            }
            .disabled(busy)
        }
        .padding(32)
        .frame(maxWidth: 560, alignment: .leading)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var title: String {
        switch gate {
        case .start: "启动 Muster 后台服务"
        case .migrate: "改由系统登录代理运行 musterd"
        }
    }

    private var symbol: String {
        switch gate {
        case .start: "play.circle"
        case .migrate: "arrow.triangle.2.circlepath"
        }
    }

    private var lines: [String] {
        let password = "这一步要把 musterd 的登录代理写入 /Library/LaunchAgents，macOS 会请你输入管理员密码。之后升级 Muster 不再需要密码。"
        switch gate {
        case .start(_, managed: false):
            return ["后台服务 musterd 没有在运行。这是开发构建，不会注册登录代理：请先手动启动 musterd，再点“重试”。"]
        case .start(let needsAdmin, managed: true):
            let first = "后台服务 musterd 没有在运行。Muster 需要它连接你的其他设备、运行会话；启动后才会进入主页面。"
            return needsAdmin ? [first, password] : [first]
        case .migrate(let needsAdmin):
            let first = "musterd 正在运行，但不是由 /Library/LaunchAgents 中当前版本的登录代理启动的：旧版 Muster 用登录项启动它，或者登录代理已经过时。迁移会更新登录代理，并重启一次 musterd。"
            let wait = "有正在进行的任务时可以先选“暂不迁移”，下次打开 Muster 会再问。"
            return needsAdmin ? [first, password, wait] : [first, wait]
        }
    }

    private var primary: String {
        switch gate {
        case .start(_, managed: false): "重试"
        case .start(let needsAdmin, managed: true): needsAdmin ? "安装并启动…" : "启动 musterd"
        case .migrate(let needsAdmin): needsAdmin ? "迁移…" : "迁移"
        }
    }
}
#endif

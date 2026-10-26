#if canImport(SwiftUI)
import MusterCore
import SwiftUI
import WebKit

struct MulticaSettings: View {
    let core: MusterCore
    let status: Muster_V1_MulticaStatus
    @State private var draft = Muster_V1_MulticaRequest()
    @State private var loaded = false

    var body: some View {
        Section("Multica") {
            Text(status.message).textSelection(.enabled)
            TextField("数据目录", text: $draft.dataDir)
            TextField("容器名称", text: $draft.containerName)
                .disabled(status.installed)
            TextField("安装脚本 HTTPS 地址", text: $draft.installerURL)
            TextField("Multica 服务 HTTPS 地址", text: $draft.serverURL)
            TextField("Multica 页面 HTTPS 地址", text: $draft.appURL)
            Toggle("保持 Multica 容器和 daemon 运行", isOn: $draft.keepAlive)
            Text("新安装默认使用 ~/Documents/Multica。迁移已有数据前，请先关闭保活并停止容器；保存后移动数据，保留旧挂载路径的兼容链接。")
                .font(.footnote).foregroundStyle(.secondary)
            HStack {
                Button(status.installed ? "保存设置" : "安装 Multica") {
                    var request = draft
                    request.action = status.installed ? .save : .install
                    core.multica(request)
                }.disabled(status.busy)
                Button("刷新状态") { core.multica(.init()) }.disabled(status.busy)
                if status.busy { ProgressView().controlSize(.small) }
            }
            if !status.installed {
                Text("安装会运行你填写的可信 Multica 安装脚本，并安装 Apple 容器内及本机的 Multica CLI。")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            if status.installed && !status.loginCommand.isEmpty {
                Text("首次使用请在终端完成登录：").font(.footnote)
                Text(status.loginCommand).font(.caption.monospaced()).textSelection(.enabled)
            }
        }
        .onAppear { load() }
        .onChange(of: status.dataDir) { _, _ in if !loaded { load() } }
    }
    private func load() {
        guard !loaded && !status.dataDir.isEmpty else { return }
        loaded = true
        draft.dataDir = status.dataDir
        draft.containerName = status.containerName
        draft.installerURL = status.installerURL
        draft.serverURL = status.serverURL
        draft.appURL = status.appURL
        draft.keepAlive = status.installed ? status.keepAlive : true
    }
}

struct MulticaView: View {
    let core: MusterCore
    let status: Muster_V1_MulticaStatus
    var body: some View {
        Group {
            if !status.appURL.isEmpty, let url = URL(string: status.appURL) {
                MulticaWebPage(url: url).id(status.appURL)
            } else {
                ContentUnavailableView {
                    Label("Multica", systemImage: "globe")
                } description: {
                    Text("在设置中填写 Multica 页面地址。")
                } actions: {
                    Button("打开设置") { core.navigate(to: .settings) }
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { core.navigate(to: .settings) } label: { Label("Settings", systemImage: "gearshape") }
            }
        }
    }
}

// WebKit owns browser navigation and its login session. Container and product
// state come solely from Rust; no host credentials are injected here.
#if os(macOS)
private struct MulticaWebPage: NSViewRepresentable {
    let url: URL
    func makeNSView(context: Context) -> WKWebView {
        let view = WKWebView()
        view.load(URLRequest(url: url))
        view.allowsBackForwardNavigationGestures = true
        return view
    }
    func updateNSView(_ view: WKWebView, context: Context) {}
}
#elseif os(iOS)
private struct MulticaWebPage: UIViewRepresentable {
    let url: URL
    func makeUIView(context: Context) -> WKWebView {
        let view = WKWebView()
        view.load(URLRequest(url: url))
        view.allowsBackForwardNavigationGestures = true
        return view
    }
    func updateUIView(_ view: WKWebView, context: Context) {}
}
#endif
#endif

import MusterCore
import SwiftUI

/// The owner's own vision-capable LLM, which looks at the app window a native
/// send failed in (ADR 0116). The key field is write-only: musterd keeps the
/// key in its own keychain item and only says whether one is stored.
struct VisionSettings: View {
    let core: MusterCore
    let status: Muster_V1_VisionStatus
    @State private var format = "openai"
    @State private var baseURL = ""
    @State private var model = ""
    @State private var apiKey = ""
    @State private var enabled = false
    @State private var loaded = false

    var body: some View {
        Section {
            Picker("API 格式", selection: $format) {
                Text("OpenAI 兼容").tag("openai")
                Text("Anthropic Messages").tag("anthropic")
            }
            .accessibilityIdentifier("vision-format")
            TextField("API 地址", text: $baseURL, prompt: Text(placeholder))
                .autocorrectionDisabled()
                .accessibilityIdentifier("vision-base-url")
            TextField("模型", text: $model, prompt: Text("支持图片输入的模型"))
                .autocorrectionDisabled()
                .accessibilityIdentifier("vision-model")
            SecureField("API Key", text: $apiKey, prompt: Text(status.keyStored ? "已保存；留空则保留" : "必填"))
                .accessibilityIdentifier("vision-api-key")
            Toggle("发送失败时截图分析", isOn: $enabled)
                .accessibilityIdentifier("vision-enabled")
            LabeledContent("屏幕录制") {
                Text(status.screenRecording ? "musterd 已获准" : "未获准：只发送失败原因")
                    .foregroundStyle(status.screenRecording ? Color.secondary : Color.orange)
            }
            HStack {
                Button("保存") { save() }.disabled(status.busy)
                Button("测试连接") { core.vision(.with { $0.action = .test }) }
                    .disabled(status.busy || !status.keyStored || baseURL.isEmpty || model.isEmpty)
                if status.keyStored {
                    Button("删除 API Key", role: .destructive) {
                        apiKey = ""
                        enabled = false
                        core.vision(.with { $0.action = .clearKey })
                    }.disabled(status.busy)
                }
                if status.busy { ProgressView().controlSize(.small) }
            }
            if !status.message.isEmpty {
                Text(status.message)
                    .font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                    .accessibilityIdentifier("vision-message")
            }
        } header: {
            Text("失败截图分析")
        } footer: {
            Text("开启后，这台 Mac 上的原生发送失败时（非 Locked Use、未锁屏），musterd 会截取目标 App 的窗口，连同 App 名和失败原因发送到上面配置的服务，并把不超过两句的分析附在手机看到的失败信息后。截图包含窗口里可见的一切，包括对话内容。分析只是参考，不会重试或改变发送结果。API Key 保存在 musterd 自己的钥匙串条目里，不会再显示。屏幕录制权限请在“系统设置 › 隐私与安全性 › 屏幕与系统录音”中授予 musterd。")
        }
        .onAppear { load() }
        .onChange(of: status.baseURL) { _, _ in if !loaded { load() } }
    }

    private var placeholder: String {
        format == "anthropic" ? "https://api.anthropic.com" : "https://api.openai.com/v1"
    }

    private func load() {
        guard !loaded && (status.keyStored || !status.baseURL.isEmpty) else { return }
        loaded = true
        format = status.apiFormat.isEmpty ? "openai" : status.apiFormat
        baseURL = status.baseURL
        model = status.model
        enabled = status.enabled
    }

    private func save() {
        let key = apiKey
        apiKey = ""
        loaded = true
        core.vision(.with {
            $0.action = .save
            $0.enabled = enabled
            $0.apiFormat = format
            $0.baseURL = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
            $0.model = model.trimmingCharacters(in: .whitespacesAndNewlines)
            $0.apiKey = key
        })
    }
}

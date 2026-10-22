#if os(iOS)
import Foundation
import MusterCore
import SwiftUI

struct PerformanceMonitorView: View {
    let core: MusterCore
    /// Members that can receive a report (the Macs).
    let devices: [Muster_V1_MemberView]
    @State private var status: PerformanceMonitorStatus?
    @State private var exportURL: URL?
    @State private var preparing = false
    @State private var uploading = false
    @State private var error: String?

    var body: some View {
        Form {
            Section {
                LabeledContent("监控", value: status?.recording == true ? "前台记录中" : "暂停或等待前台")
                Text("前台每 10 秒记录一次，后台暂停。高 CPU 时自动采集忙碌线程的调用栈，每分钟最多一次。日志跨重启保留，达到大小上限后自动覆盖最早的记录。")
                    .font(.footnote).foregroundStyle(.secondary)
                if let status, let sampledAt = status.sampledAt {
                    LabeledContent("最近采样") { Text(sampledAt, style: .time) }
                    LabeledContent("CPU（100% 为单核）", value: String(format: "%.1f%%", status.cpuPercent))
                    LabeledContent("内存占用", value: String(format: "%.1f MB", Double(status.memoryBytes) / 1_048_576))
                    LabeledContent("系统热状态", value: thermalLabel(status.thermalState))
                } else {
                    Text("等待首条采样。进入聊天页后可直接使用，无需开启诊断模式。")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                Button("刷新读数") { Task { await refresh() } }
            } header: { Text("性能监控") } footer: {
                Text("热状态由 iOS 提供，不是温度读数。此页显示最近一次采样，不持续刷新界面。")
            }
            Section {
                Button("准备导出日志") {
                    preparing = true; error = nil; exportURL = nil
                    Task {
                        do { exportURL = try await PerformanceDiagnostics.shared.export() }
                        catch { self.error = "暂时无法导出，请稍后重试。" }
                        preparing = false
                    }
                }.disabled(preparing)
                if preparing { ProgressView() }
                if let exportURL { ShareLink("分享性能日志", item: exportURL) }
                if let error { Text(error).foregroundStyle(.red) }
                if let message = status?.error { Text(message).foregroundStyle(.red) }
            } header: { Text("导出") } footer: {
                Text("记录性能数值、页面类型、消息数量及高 CPU 线程栈地址，不包含聊天正文、设备身份或密钥。日志仅保存在本机，分享由你选择。滚动日志最多 4 MB，导出快照另占最多 4 MB。")
            }
            Section {
                if devices.isEmpty {
                    Text("没有可接收日志的设备。").foregroundStyle(.secondary)
                }
                ForEach(devices, id: \.deviceID) { device in
                    Button { upload(to: device) } label: {
                        Label("发送到 \(device.name)", systemImage: "paperplane")
                    }
                    .disabled(uploading)
                }
                if uploading { ProgressView() }
            } header: { Text("上报日志") } footer: {
                Text("把同一份日志快照发送到所选设备，保存在该设备的 /tmp/muster-reports 目录（设备重启后清除）。结果以提示显示。")
            }
        }
        .navigationTitle("性能监控")
        .task { await refresh() }
    }

    private func refresh() async { status = await PerformanceDiagnostics.shared.currentStatus() }

    private func upload(to device: Muster_V1_MemberView) {
        uploading = true; error = nil
        Task {
            do {
                let url = try await PerformanceDiagnostics.shared.export()
                core.uploadDiagnostics(device: device.deviceID, data: try Data(contentsOf: url))
            } catch { self.error = "暂时无法读取日志，请稍后重试。" }
            uploading = false
        }
    }

    private func thermalLabel(_ value: Int) -> String {
        switch value {
        case 0: "正常（nominal）"
        case 1: "轻度压力（fair）"
        case 2: "严重压力（serious）"
        case 3: "临界压力（critical）"
        default: "未知"
        }
    }
}
#endif

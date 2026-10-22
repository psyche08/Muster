#if canImport(SwiftUI)
import MusterCore
import SwiftUI
#if os(macOS)
import AppKit
#endif

struct BrowserView: View {
    let core: MusterCore
    let vm: ViewModel
    @State private var request = ""
    private var browser: Muster_App_BrowserView { vm.browser }
    var body: some View {
        Form {
            Section("Chrome extension") {
                Label(browser.status.connected ? "Connected" : "Setup needed",
                      systemImage: browser.status.connected ? "checkmark.circle.fill" : "puzzlepiece.extension")
                    .foregroundStyle(browser.status.connected ? .green : .secondary)
                Text(browser.status.message.isEmpty ? "Checking the target device…" : browser.status.message)
                if browser.local {
                    Button("1. Set up native host") { core.browserSetup(install: true) }
                    #if os(macOS)
                    Button("2. Show extension folder") {
                        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: browser.status.extensionPath)])
                    }.disabled(browser.status.extensionPath.isEmpty)
                    Button("3. Open Chrome extensions") { openChromeExtensions() }
                    #endif
                    Text("In Chrome, turn on Developer mode, choose Load unpacked, and select the AgentHaloBrowser folder. Chrome asks for browser debugging permission.")
                    if !browser.status.extensionPath.isEmpty {
                        Text(browser.status.extensionPath).font(.caption.monospaced()).textSelection(.enabled)
                    }
                } else {
                    Text("On the target Mac, open Muster → Settings → Chrome extension and complete installation. Then return here and check the connection.")
                }
                Button("Check connection") { core.browserSetup() }
            }
            Section("Remote access & Locked Use") {
                Text("Grant this device browser.view to read pages and browser.control to change them in Network → device → permissions. These permissions are separate from agent access.")
                Text("For a locked Mac, also grant locked.control and enable control while locked on the target. Authenticate each remote request. Browser operations keep the Mac locked; no screen cover or temporary unlock is needed.")
            }
            Section("Browser request") {
                HStack {
                    Button("List tabs") { request = "{\"op\":\"tabs\"}" }
                    Button("Connection test") { request = "{\"op\":\"ping\"}" }
                }
                #if os(iOS)
                MusterTextEditor("{\"op\":\"tabs\"}", text: $request, monospace: true, minimumHeight: 120, maximumHeight: 240)
                #else
                TextEditor(text: $request).font(.system(.body, design: .monospaced)).frame(minHeight: 120)
                #endif
                Text("Use snapshot with a tabId first. Click/type/press also need document and ref from that snapshot.").font(.footnote).foregroundStyle(.secondary)
                Button(browser.local ? "Run" : "Run & authenticate") { core.browserRun(request) }
                    .buttonStyle(.borderedProminent)
            }
            if !browser.result.isEmpty {
                Section("Result") { Text(browser.result).font(.system(.body, design: .monospaced)).textSelection(.enabled) }
            }
        }
        .groupedForm()
        .disabled(browser.busy)
        .overlay { if browser.busy { ProgressView("Waiting for browser…").padding().background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12)) } }
        .navigationTitle("Agent Halo Browser")
        .onAppear { request = browser.draft }
        .onChange(of: browser.draft) { _, draft in request = draft }
    }
    #if os(macOS)
    private func openChromeExtensions() {
        guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.google.Chrome"),
              let url = URL(string: "chrome://extensions") else { return }
        NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
    }
    #endif
}

#endif

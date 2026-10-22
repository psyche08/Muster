// Local display preferences and keyboard geometry for the half-fold
// workspace (the HarmonyOS shell's FoldDisplay, without a hinge: iOS
// exposes no fold geometry, so Automatic never splits here and the forced
// layouts split at the middle). Business state remains in MusterCore.
#if canImport(SwiftUI)
import Combine
import Foundation
import MusterCore
import SwiftUI
#if os(iOS)
import UIKit
#endif

@MainActor
final class FoldDisplay: ObservableObject {
    static let shared = FoldDisplay()
    static let preferenceKey = "flexPreference"
    @Published var preference: FlexPreference {
        didSet { UserDefaults.standard.set(preference.rawValue, forKey: Self.preferenceKey) }
    }
    /// The keyboard over the window (the system's or the in-app one), in points.
    @Published private(set) var keyboardHeight: Double = 0
    private var observers: [Any] = []

    private init() {
        preference = FlexPreference(stored: UserDefaults.standard.string(forKey: Self.preferenceKey) ?? "")
        #if os(iOS)
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: UIResponder.keyboardWillChangeFrameNotification, object: nil, queue: .main) { [weak self] note in
            guard let frame = (note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue else { return }
            let screen = UIScreen.main.bounds
            let height = max(0, screen.maxY - max(frame.minY, screen.minY))
            Task { @MainActor in self?.keyboardHeight = frame.minY >= screen.maxY ? 0 : height }
        })
        observers.append(center.addObserver(forName: UIResponder.keyboardWillHideNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.keyboardHeight = 0 }
        })
        #endif
    }

    var geometry: FoldGeometry { FoldGeometry(keyboardHeight: keyboardHeight) }

    /// The split for a workspace of this size under the preference.
    func layout(for size: CGSize) -> WorkspaceLayout {
        workspaceLayout(area: WorkspaceArea(width: size.width, height: size.height), fold: geometry, preference: preference)
    }
}

extension FlexPreference {
    var title: String {
        switch self {
        case .automatic: "自动"
        case .always: "上下布局"
        case .side: "左右分栏"
        case .disabled: "单页"
        }
    }
}

private struct WorkspaceLayoutKey: EnvironmentKey {
    static let defaultValue = WorkspaceLayout()
}

extension EnvironmentValues {
    /// The split the root view resolved for the workspace on screen.
    var workspaceLayout: WorkspaceLayout {
        get { self[WorkspaceLayoutKey.self] }
        set { self[WorkspaceLayoutKey.self] = newValue }
    }
}

/// The layout choices in Settings.
struct LayoutPicker: View {
    @ObservedObject private var display = FoldDisplay.shared

    init() {}

    var body: some View {
        Picker("显示方式", selection: $display.preference) {
            ForEach(FlexPreference.allCases, id: \.self) { Text($0.title).tag($0) }
        }
    }
}

#endif

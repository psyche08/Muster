import SwiftUI
import Observation

/// Only rendering choices live here. Rust still owns the timeline and defaults.
final class TranscriptDisclosures {
    @Observable final class Choice {
        var override: Bool?
    }
    private var choices: [[String]: Choice] = [:]

    func choice(_ key: [String]) -> Choice {
        if let existing = choices[key] { return existing }
        let value = Choice(); choices[key] = value; return value
    }
}

private struct TranscriptDisclosuresKey: EnvironmentKey {
    static let defaultValue: TranscriptDisclosures? = nil
}
private struct TranscriptDisclosurePathKey: EnvironmentKey {
    static let defaultValue: [String] = []
}
extension EnvironmentValues {
    var transcriptDisclosures: TranscriptDisclosures? {
        get { self[TranscriptDisclosuresKey.self] }
        set { self[TranscriptDisclosuresKey.self] = newValue }
    }
    var transcriptDisclosurePath: [String] {
        get { self[TranscriptDisclosurePathKey.self] }
        set { self[TranscriptDisclosurePathKey.self] = newValue }
    }
}

/// Row-local fallback preserves desktop and standalone rendering behavior.
struct TranscriptDisclosure: DynamicProperty {
    @Environment(\.transcriptDisclosures) private var store
    @Environment(\.transcriptDisclosurePath) private var path
    @State private var localOverride: Bool?

    func binding(_ kind: String, id: String, defaultValue: Bool = false) -> Binding<Bool> {
        if let store {
            let choice = store.choice(path + [kind, id])
            return Binding(get: { choice.override ?? defaultValue }, set: { choice.override = $0 })
        }
        return Binding(get: { localOverride ?? defaultValue }, set: { localOverride = $0 })
    }
}

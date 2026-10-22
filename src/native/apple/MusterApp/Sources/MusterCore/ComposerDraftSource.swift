import Foundation

/// A narrow input callback, owned by the visible native editor. The shell
/// reports its buffer with an action; Rust decides when to persist it.
public final class ComposerDraftSource {
    private var source: (id: UUID, read: () -> Muster_App_ChatDraft)?

    public func register(_ read: @escaping () -> Muster_App_ChatDraft) -> UUID {
        let id = UUID()
        source = (id, read)
        return id
    }

    public func unregister(_ id: UUID) {
        if source?.id == id { source = nil }
    }

    func attaching(to intent: Intent) -> Intent {
        guard let draft = source?.read() else { return intent }
        return .with {
            $0.composerAction = .with { $0.draft = draft; $0.action = intent }
        }
    }
}

import Foundation

/// Viewport input only: one near-top intent per user gesture, never on layout,
/// tab restoration, or a failed load. Rust owns availability and serialization.
struct HistoryScrollTrigger {
    private var sent = true
    mutating func beginGesture() { sent = false }
    mutating func request(offset: CGFloat, hasEarlier: Bool, loading: Bool, failed: Bool) -> Bool {
        guard !sent, offset <= 160, hasEarlier, !loading, !failed else { return false }
        sent = true
        return true
    }
}

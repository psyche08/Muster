#if os(macOS)
import AppKit
import SwiftUI
import XCTest
import MusterCore
@testable import MusterUI

final class CacheFooterTests: XCTestCase {
    @MainActor
    func testCacheExpiryRoundTripAndPhoneWidthFooter() throws {
        let summary = Muster_V1_AgentMessage.with {
            $0.tsUnix = 1790832600
            $0.usage = Muster_V1_TurnUsage.with {
                $0.model = "claude-opus-5-5"
                $0.tokensKnown = true; $0.inputTokens = 150000; $0.outputTokens = 4000
                $0.cacheHitKnown = true; $0.cacheHitPercent = 86.5; $0.requestCount = 7
                $0.cacheReadTokens = 129750; $0.cacheWriteTokens = 20000
                $0.firstRequestCacheHitKnown = true; $0.firstRequestCacheHitPercent = 20
                $0.costKnown = true; $0.costSource = "estimated"; $0.costUsd = 15.02373
                $0.cacheExpiry = Muster_V1_CacheExpiry.with {
                    $0.fromUnix = 1790832800; $0.untilUnix = 1790836100
                    $0.label = "Estimated cache expiry"
                    $0.detail = "Mixed 5-minute / 1-hour cache writes. Later reuse may refresh matching prefixes."
                    $0.observedGapSeconds = 900
                }
            }
        }
        let decoded = try Muster_V1_AgentMessage(serializedBytes: summary.serializedData())
        XCTAssertEqual(decoded, summary)
        for (name, value, details) in [("range", decoded, false), ("details", decoded, true),
                                        ("unknown", Muster_V1_AgentMessage(), false)] {
            let host = NSHostingView(rootView: TurnFooter(message: value, cacheDetails: details).padding(12).frame(width: 350)
                .background(Color.white).environment(\.colorScheme, .light))
            host.frame = NSRect(origin: .zero, size: host.fittingSize)
            let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false; window.contentView = host
            host.layoutSubtreeIfNeeded()
            XCTAssertEqual(host.fittingSize.width, 350, accuracy: 1)
            XCTAssertLessThan(host.fittingSize.height, 350)
            let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
                .write(to: URL(fileURLWithPath: "/private/tmp/muster-cache-footer-\(name).png"))
            window.close()
        }
    }
}
#endif

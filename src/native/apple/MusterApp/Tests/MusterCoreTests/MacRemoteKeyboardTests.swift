#if os(macOS)
import AppKit
import XCTest
import MusterCore
@testable import MusterUI

@MainActor
final class MacRemoteKeyboardTests: XCTestCase {
    func testFocusedSurfaceTypesTextAndKeysUntilGrantEnds() throws {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 80, height: 60),
                              styleMask: [.titled], backing: .buffered, defer: false)
        let surface = MacPointer.Surface()
        window.contentView = surface
        var sent: [Muster_V1_DesktopKeyboard.OneOf_Input] = []
        XCTAssertFalse(surface.acceptsFirstResponder)
        surface.keyboardInput = { sent.append($0) }

        func press(_ code: UInt16, _ chars: String, _ modifiers: NSEvent.ModifierFlags = []) throws {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero,
                modifierFlags: modifiers, timestamp: 0, windowNumber: window.windowNumber,
                context: nil, characters: chars, charactersIgnoringModifiers: chars,
                isARepeat: false, keyCode: code))
            surface.keyDown(with: event)
        }

        XCTAssertTrue(window.makeFirstResponder(surface))
        try press(15, "R", .shift)
        try press(26, "7")
        try press(0, "é", .option)
        try press(51, "\u{7f}")
        try press(36, "\r")
        try press(76, "\r")
        try press(53, "\u{1b}")
        try press(53, "\u{1b}", .option)
        try press(8, "c", .command)
        try press(0, "\u{1}", .control)
        try press(123, "\u{f702}", .function)
        try press(48, "\t")
        let expected: [Muster_V1_DesktopKeyboard.OneOf_Input] = [
            .text("R"), .text("7"), .text("é"), .key(.backspace), .key(.return), .key(.return), .key(.escape)
        ]
        XCTAssertEqual(sent, expected)

        window.makeFirstResponder(nil)
        try press(53, "\u{1b}")
        try press(0, "a")
        XCTAssertEqual(sent, expected)
        XCTAssertTrue(window.makeFirstResponder(surface))
        surface.keyboardInput = nil
        XCTAssertFalse(surface.acceptsFirstResponder)
        XCTAssertFalse(window.firstResponder === surface)
        try press(0, "a")
        XCTAssertEqual(sent, expected)
    }
}
#endif

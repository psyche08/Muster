// DeepSeek Harness (DSH) design tokens for the session screen chrome and the
// execution trace. Values are read from the installed DeepSeek Harness 0.2.1
// client's design-platform/alias CSS (see ADR 0083). Presentation only: no
// session, authorization or send state is derived here.
#if canImport(SwiftUI)
import SwiftUI

/// The DSH alias palette, resolved for the current light or dark appearance.
struct SessionChromePalette {
    let scheme: ColorScheme

    private func color(_ light: UInt32, _ dark: UInt32) -> Color {
        let value = scheme == .dark ? dark : light
        return Color(.sRGB,
                     red: Double((value >> 16) & 255) / 255,
                     green: Double((value >> 8) & 255) / 255,
                     blue: Double(value & 255) / 255,
                     opacity: 1)
    }

    private func color(_ light: UInt32, _ dark: UInt32, opacity: Double) -> Color {
        color(light, dark).opacity(opacity)
    }

    // Surfaces.
    var bgBase: Color { color(0xffffff, 0x151517) }
    var bgLayer1: Color { color(0xffffff, 0x232324) }
    var bgLayer2: Color { color(0xffffff, 0x2c2c2e) }
    var bgLayer3: Color { color(0xffffff, 0x353638) }
    var sidebarFill: Color { color(0xf9fafb, 0x1b1b1c) }
    var modulePlatform: Color { color(0xf5f6f7, 0x353638) }
    var userBubble: Color { color(0xedf3fe, 0x2c2c2e) }
    var codeBlock: Color { color(0xf9fafb, 0x1b1b1c) }

    // Labels.
    var labelPrimary: Color { color(0x0f1115, 0xf9fafb) }
    var labelSecondary: Color { color(0x61666b, 0xcfd3d6) }
    var labelTertiary: Color { color(0x81858c, 0xadb2b8) }
    var labelCaption: Color { color(0xadb2b8, 0x81858c) }
    var labelDimmed: Color { color(0xe1e5ee, 0x43454a) }

    // Borders, at DSH's four levels.
    var borderL1: Color { color(0x000000, 0xffffff, opacity: scheme == .dark ? 0.06 : 0.04) }
    var borderL2: Color { color(0x000000, 0xffffff, opacity: scheme == .dark ? 0.12 : 0.10) }
    var borderL3: Color { color(0x000000, 0xffffff, opacity: scheme == .dark ? 0.16 : 0.12) }
    var borderL4: Color { color(0x000000, 0xffffff, opacity: scheme == .dark ? 0.20 : 0.16) }

    // Interaction and states.
    var businessPrimary: Color { color(0x4176e6, 0x7aaaff) }
    var businessTertiary: Color { color(0xe4edfd, 0x34415b) }
    var hover: Color { color(0x263148, 0xffffff, opacity: scheme == .dark ? 0.08 : 0.06) }
    var active: Color { color(0x263148, 0xffffff, opacity: scheme == .dark ? 0.14 : 0.10) }
    var successPrimary: Color { color(0x22c55e, 0x22c55e) }
    var successTertiary: Color { color(0xe6faed, 0x233c2c) }
    var warnLabel: Color { color(0xdd8629, 0xdd8629) }
    var warnTertiary: Color { color(0xfef5e7, 0x27241f) }
    var errorPrimary: Color { color(0xec1313, 0xf25a5a) }
    var errorTertiary: Color { color(0xfee2e2, 0x570c0c) }
    var idlePrimary: Color { color(0xd4d4d4, 0x545557) }

    /// The reasoning/timeline violet. DSH mixes its business blue with the
    /// error secondary for assistant records; Muster has no assistant-message
    /// record, so the same mix labels reasoning instead.
    var reasoning: Color { color(0x9474bc, 0xaa8abd) }
    var reasoningTertiary: Color { color(0xf1ecf8, 0x342d44) }

    /// 22% business blue over the ledger surface, as DSH's turn rail.
    var turnRail: Color { color(0xd5e0fa, 0x364254) }

    /// A translucent row wash for a selected timeline range.
    var selectionWash: Color { businessPrimary.opacity(0.12) }
}

/// Geometry and type shared by the session header, view tabs and trace page.
enum SessionChromeMetrics {
    static let headerMinHeight: CGFloat = 76
    static let headerTopPadding: CGFloat = 10
    static let headerLeadingPadding: CGFloat = 20
    static let headerTrailingPadding: CGFloat = 28
    static let titleRowMinHeight: CGFloat = 30
    static let titleFontSize: CGFloat = 14
    static let titleLineHeight: CGFloat = 20
    static let crumbGap: CGFloat = 4
    static let titleClusterGap: CGFloat = 10
    static let headerActionsGap: CGFloat = 8
    static let headerUtilitiesGap: CGFloat = 8
    static let headerUtilitiesMargin: CGFloat = 20

    static let chipFontSize: CGFloat = 12
    static let chipHeight: CGFloat = 22
    static let chipHorizontalPadding: CGFloat = 7
    static let chipGap: CGFloat = 2
    static let chipCornerRadius: CGFloat = 8
    static let chipIconSize: CGFloat = 13

    static let tabsTopGap: CGFloat = 10
    static let tabsLeadingInset: CGFloat = 8
    static let tabGap: CGFloat = 32
    static let tabFontSize: CGFloat = 13
    static let tabLineHeight: CGFloat = 16
    static let tabBottomInset: CGFloat = 9
    static let tabUnderlineHeight: CGFloat = 2
    static let tabUnderlineRadius: CGFloat = 1
    static let dividerWidth: CGFloat = 0.5
    static let tabMoreSize: CGFloat = 22

    // The compact (iOS) header keeps DSH's tab look at touch size: about 40pt
    // tall tabs, under the native navigation bar.
    static let compactHeaderTopPadding: CGFloat = 2
    static let compactHeaderPadding: CGFloat = 16
    static let compactTabFontSize: CGFloat = 15
    static let compactTabLineHeight: CGFloat = 20
    static let compactTabTopInset: CGFloat = 10
    static let compactTabGap: CGFloat = 24

    static let traceToolbarHeight: CGFloat = 34
    static let traceToolbarControlHeight: CGFloat = 20
    static let traceToolbarFontSize: CGFloat = 12
    static let traceSearchWidth: CGFloat = 164
    static let traceSearchHeight: CGFloat = 22
    static let traceTimelineHeight: CGFloat = 50
    static let traceTimelineLabelWidth: CGFloat = 44
    static let traceTimelineLaneHeight: CGFloat = 14
    static let traceTimelineBarHeight: CGFloat = 8
    static let traceRowHeight: CGFloat = 30
    static let traceEventColumnWidth: CGFloat = 84
    static let traceKindTagHeight: CGFloat = 19
    static let traceKindTagFontSize: CGFloat = 10
    static let traceKindTagCornerRadius: CGFloat = 4
    static let traceInspectorWidth: CGFloat = 340
    static let traceContentFontSize: CGFloat = 12
    static let traceDetailFontSize: CGFloat = 12
    static let traceHeaderHeight: CGFloat = 30
}
#endif

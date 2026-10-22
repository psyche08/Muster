// The app icon drawn as vectors, so it stays sharp at any size on both
// platforms. Same geometry as Design/AppIcon.svg, in its 1024-unit square.
#if canImport(SwiftUI)
import SwiftUI

/// Muster's icon: three arms taper inward to a clay core on an ink tile.
struct MusterMark: View {
    var size: CGFloat = 88

    var body: some View {
        Canvas { context, canvas in
            let scale = canvas.width / 1024
            let tile = RoundedRectangle(cornerRadius: 229 * scale, style: .continuous)
            context.fill(tile.path(in: CGRect(origin: .zero, size: canvas)), with: .color(Self.ink))
            context.translateBy(x: canvas.width / 2, y: canvas.height / 2)
            context.scaleBy(x: scale, y: scale)
            // Each piece is filled on its own: one path would let opposite
            // windings cancel where a blade and its end caps overlap.
            for piece in Self.arms {
                context.fill(piece, with: .color(Self.parchment))
            }
            context.fill(Path(ellipseIn: CGRect(x: -76, y: -76, width: 152, height: 152)), with: .color(Self.clay))
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private static let ink = Color(red: 0x2A / 255.0, green: 0x28 / 255.0, blue: 0x25 / 255.0)
    private static let parchment = Color(red: 0xED / 255.0, green: 0xE6 / 255.0, blue: 0xDA / 255.0)
    private static let clay = Color(red: 0xB5 / 255.0, green: 0x77 / 255.0, blue: 0x5D / 255.0)

    private static let arms: [Path] = (0..<3).flatMap { i in
        arm(theta: (-90 + 120 * Double(i)) * .pi / 180)
    }

    /// One arm around the origin: a blade along a logarithmic spiral from
    /// radius 350 to 148 over 160°, widening from 34 to 110, with round ends.
    private static func arm(theta: Double) -> [Path] {
        let n = 80
        let spine = (0...n).map { k -> (x: Double, y: Double, width: Double) in
            let t = Double(k) / Double(n)
            let r = 350 * pow(148.0 / 350, t)
            let a = theta + 160 * .pi / 180 * t
            return (r * cos(a), r * sin(a), 34 + 76 * t)
        }
        var left: [CGPoint] = [], right: [CGPoint] = []
        for k in 0...n {
            let p = spine[k], before = spine[max(k - 1, 0)], after = spine[min(k + 1, n)]
            let dx = after.x - before.x, dy = after.y - before.y
            let length = max(hypot(dx, dy), 1)
            let nx = -dy / length * p.width / 2, ny = dx / length * p.width / 2
            left.append(CGPoint(x: p.x + nx, y: p.y + ny))
            right.append(CGPoint(x: p.x - nx, y: p.y - ny))
        }
        var blade = Path()
        blade.addLines(left + right.reversed())
        blade.closeSubpath()
        let ends = [spine[0], spine[n]].map { p in
            Path(ellipseIn: CGRect(x: p.x - p.width / 2, y: p.y - p.width / 2, width: p.width, height: p.width))
        }
        return [blade] + ends
    }
}
#endif

// Render the existing vector mark into the macOS icon slots; share iOS colors.
// Run: swift scripts/generate-mac-assets.swift
import AppKit

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let fm = FileManager.default
let assets = root.appendingPathComponent("Apps/macOS/Assets.xcassets")
let icon = assets.appendingPathComponent("AppIcon.appiconset")
try fm.createDirectory(at: icon, withIntermediateDirectories: true)
let info: [String: Any] = ["author": "xcode", "version": 1]
let source = root.appendingPathComponent("Design/AppIcon.svg")
guard let mark = NSImage(contentsOf: source) else { fatalError("Cannot read the vector icon") }
var entries: [[String: String]] = []
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = points * scale
        let name = "icon_\(points)x\(points)@\(scale)x.png"
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                                           bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
                                           isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
              let context = NSGraphicsContext(bitmapImageRep: bitmap) else { fatalError("Cannot create icon canvas") }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        context.imageInterpolation = .high
        let side = CGFloat(pixels)
        let tile = CGRect(x: side * 0.08, y: side * 0.08, width: side * 0.84, height: side * 0.84)
        NSBezierPath(roundedRect: tile, xRadius: side * 0.185, yRadius: side * 0.185).addClip()
        mark.draw(in: tile, from: .zero, operation: .copy, fraction: 1)
        context.flushGraphics()
        NSGraphicsContext.restoreGraphicsState()
        guard let png = bitmap.representation(using: .png, properties: [:]) else { fatalError("Cannot encode icon") }
        try png.write(to: icon.appendingPathComponent(name))
        entries.append(["filename": name, "idiom": "mac", "size": "\(points)x\(points)", "scale": "\(scale)x"])
    }
}
try JSONSerialization.data(withJSONObject: ["images": entries, "info": info], options: [.prettyPrinted, .sortedKeys])
    .write(to: icon.appendingPathComponent("Contents.json"))
try JSONSerialization.data(withJSONObject: ["info": info], options: [.prettyPrinted, .sortedKeys])
    .write(to: assets.appendingPathComponent("Contents.json"))
let accent = assets.appendingPathComponent("AccentColor.colorset")
try fm.createDirectory(at: accent, withIntermediateDirectories: true)
try Data(contentsOf: root.appendingPathComponent("Apps/iOS/Assets.xcassets/AccentColor.colorset/Contents.json"))
    .write(to: accent.appendingPathComponent("Contents.json"))
print("Generated macOS icon slots and matching light/dark accent colors")

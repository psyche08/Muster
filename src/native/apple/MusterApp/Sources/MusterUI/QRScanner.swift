// The camera on iPhone (VisionKit), and the QR image of an invite.
#if canImport(SwiftUI)
import MusterCore
import SwiftUI
#if canImport(CoreImage)
import CoreImage.CIFilterBuiltins
#endif
#if os(iOS) && canImport(VisionKit)
import VisionKit

struct QRScannerView: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    static var isAvailable: Bool {
        DataScannerViewController.isSupported && DataScannerViewController.isAvailable
    }

    func makeCoordinator() -> Coordinator { Coordinator(onCode: onCode) }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}

    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) {
        controller.stopScanning()
    }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onCode: (String) -> Void
        private var delivered = false
        init(onCode: @escaping (String) -> Void) { self.onCode = onCode }

        func dataScanner(_ scanner: DataScannerViewController, didAdd added: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !delivered else { return }
            for item in added {
                if case .barcode(let code) = item, let payload = code.payloadStringValue, payload.hasPrefix("muster1:") {
                    delivered = true
                    scanner.stopScanning()
                    onCode(payload)
                    return
                }
            }
        }
    }
}
#endif

/// The invite as a QR image, big enough for a phone camera across a desk.
struct QRCodeImage: View {
    let text: String

    var body: some View {
        #if canImport(CoreImage)
        if let image = Self.render(text) {
            Image(decorative: image, scale: 1)
                .interpolation(.none)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: 320)
                .accessibilityLabel("Invite QR code")
        } else {
            Text("QR unavailable")
        }
        #else
        Text(text).font(.system(.footnote, design: .monospaced))
        #endif
    }

    #if canImport(CoreImage)
    private static func render(_ text: String) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 8, y: 8))
        return CIContext().createCGImage(scaled, from: scaled.extent)
    }
    #endif
}
#endif

#if os(macOS) || os(iOS)
import AVFoundation
import Combine

/// The layer consumes the ordered Moonlight decode units even while its tab is hidden.
public final class RemoteVideo: ObservableObject {
    public struct Surface {
        public let session: UInt64
        public let layer: AVSampleBufferDisplayLayer
        public let size: CGSize
    }
    @Published public private(set) var surface: Surface?
    private let lock = NSLock()
    private var session: UInt64?
    private var format: CMVideoFormatDescription?
    private var layer: AVSampleBufferDisplayLayer?

    public init() {}

    public func submit(session id: UInt64, bytes: Data) {
        lock.lock()
        defer { lock.unlock() }
        if bytes.isEmpty {
            if session == id {
                layer?.sampleBufferRenderer.flush()
                layer = nil
                session = nil
                format = nil
                DispatchQueue.main.async { [weak self] in
                    if self?.surface?.session == id { self?.surface = nil }
                }
            }
            return
        }
        if session != id {
            session = id
            format = nil
            layer = AVSampleBufferDisplayLayer()
        }
        guard let layer else { return }
        var payload = Data()
        var sps: Data?
        var pps: Data?
        bytes.withUnsafeBytes { (buffer: UnsafeRawBufferPointer) in
            let raw = buffer.bindMemory(to: UInt8.self)
            var start: Int?
            var zeroes = 0
            func appendNAL(_ end: Int) {
                guard let start, end > start, let base = raw.baseAddress else { return }
                let data = UnsafeBufferPointer(start: base + start, count: end - start)
                switch raw[start] & 31 {
                case 7: sps = Data(buffer: data)
                case 8: pps = Data(buffer: data)
                default:
                    var length = UInt32(data.count).bigEndian
                    withUnsafeBytes(of: &length) { payload.append(contentsOf: $0) }
                    payload.append(data.baseAddress!, count: data.count)
                }
            }
            for i in raw.indices {
                if raw[i] == 0 { zeroes += 1; continue }
                if raw[i] == 1 && zeroes >= 2 {
                    appendNAL(i-zeroes)
                    start = i+1
                }
                zeroes = 0
            }
            appendNAL(raw.count-zeroes)
        }
        if let sps, let pps {
            var next: CMVideoFormatDescription?
            let status = sps.withUnsafeBytes { a in pps.withUnsafeBytes { b in
                CMVideoFormatDescriptionCreateFromH264ParameterSets(allocator: nil, parameterSetCount: 2,
                    parameterSetPointers: [a.bindMemory(to: UInt8.self).baseAddress!, b.bindMemory(to: UInt8.self).baseAddress!],
                    parameterSetSizes: [sps.count, pps.count], nalUnitHeaderLength: 4, formatDescriptionOut: &next)
            }}
            guard status == noErr, let next else { return }
            let d = CMVideoFormatDescriptionGetDimensions(next)
            guard (1...2560).contains(d.width), (1...2560).contains(d.height) else { return }
            if format != next {
                format = next
                let value = Surface(session: id, layer: layer, size: CGSize(width: Int(d.width), height: Int(d.height)))
                DispatchQueue.main.async { [weak self] in self?.surface = value }
            }
            if layer.sampleBufferRenderer.requiresFlushToResumeDecoding { layer.sampleBufferRenderer.flush() }
        }
        guard let format, !payload.isEmpty else { return }
        var block: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(allocator: nil, memoryBlock: nil, blockLength: payload.count,
            blockAllocator: nil, customBlockSource: nil, offsetToData: 0, dataLength: payload.count,
            flags: 0, blockBufferOut: &block) == noErr, let block else { return }
        let result = payload.withUnsafeBytes {
            CMBlockBufferReplaceDataBytes(with: $0.baseAddress!, blockBuffer: block, offsetIntoDestination: 0, dataLength: $0.count)
        }
        var sample: CMSampleBuffer?
        var length = payload.count
        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: CMClockGetTime(CMClockGetHostTimeClock()), decodeTimeStamp: .invalid)
        guard result == noErr, CMSampleBufferCreateReady(allocator: nil, dataBuffer: block, formatDescription: format,
            sampleCount: 1, sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleSizeEntryCount: 1,
            sampleSizeArray: &length, sampleBufferOut: &sample) == noErr, let sample else { return }
        if let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: true) {
            let values = unsafeBitCast(CFArrayGetValueAtIndex(attachments, 0), to: CFMutableDictionary.self)
            CFDictionarySetValue(values, Unmanaged.passUnretained(kCMSampleAttachmentKey_DisplayImmediately).toOpaque(),
                                 Unmanaged.passUnretained(kCFBooleanTrue).toOpaque())
        }
        layer.sampleBufferRenderer.enqueue(sample)
    }
}
#endif

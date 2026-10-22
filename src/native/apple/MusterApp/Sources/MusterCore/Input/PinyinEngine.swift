// Pinyin to Chinese for the in-app keyboard: splits typed letters into
// syllables (full ones, a trailing partial one, or bare initials as an
// abbreviation), looks words up in the bundled dictionary and composes a
// sentence when no single word covers everything. Pure text in, text out:
// no platform calls, so the tests run it on Linux as well. The same engine
// as the HarmonyOS shell's PinyinEngine.ts, with the same dictionary.
import Foundation

public struct PinyinEntry {
    public let syllables: [String]
    public let word: String
    public let freq: Double
}

/// One piece of the typed letters: a whole syllable, or a prefix of one.
public struct PinyinSegment: Equatable {
    public var text: String
    public var full: Bool
    /// Offsets into the composing letters (and apostrophes).
    public var start: Int
    public var end: Int
}

public struct PinyinCandidate: Equatable {
    public let word: String
    /// How many typed characters (letters and apostrophes) the word replaces.
    public let consumed: Int
}

private let initials: Set<String> = ["b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x",
                                     "zh", "ch", "sh", "r", "z", "c", "s", "y", "w"]
private let maxCandidates = 80

public final class PinyinDict {
    public private(set) var syllables: Set<String> = []
    private var prefixes: Set<String> = []
    /// First letter of each syllable → entries, highest frequency first.
    private var bySignature: [String: [PinyinEntry]] = [:]

    /// Lines of "syl'syl<TAB>word<TAB>frequency"; "#" starts a comment.
    public static func parse(_ text: String) -> PinyinDict {
        let dict = PinyinDict()
        for line in text.split(separator: "\n", omittingEmptySubsequences: true) {
            if line.hasPrefix("#") { continue }
            let parts = line.split(separator: "\t", omittingEmptySubsequences: false)
            if parts.count < 3 { continue }
            let syllables = parts[0].split(separator: "'", omittingEmptySubsequences: false).map(String.init)
            let word = String(parts[1])
            let freq = Double(parts[2].trimmingCharacters(in: .whitespaces)) ?? 1
            if syllables.isEmpty || syllables.count != word.count { continue }
            for syllable in syllables where !dict.syllables.contains(syllable) {
                dict.syllables.insert(syllable)
                for i in 1...syllable.count { dict.prefixes.insert(String(syllable.prefix(i))) }
            }
            let signature = String(syllables.map { $0.first ?? " " })
            dict.bySignature[signature, default: []].append(PinyinEntry(syllables: syllables, word: word, freq: freq == 0 ? 1 : freq))
        }
        for key in dict.bySignature.keys {
            // Stable: equal frequencies keep the dictionary's order.
            dict.bySignature[key] = dict.bySignature[key]!.enumerated()
                .sorted { $0.element.freq > $1.element.freq || ($0.element.freq == $1.element.freq && $0.offset < $1.offset) }
                .map(\.element)
        }
        return dict
    }

    public static func load(from url: URL) throws -> PinyinDict {
        parse(try String(contentsOf: url, encoding: .utf8))
    }

    /// The dictionary this package carries (pinyin.txt, from AOSP PinyinIME).
    public static func bundled() throws -> PinyinDict {
        guard let url = Bundle.module.url(forResource: "pinyin", withExtension: "txt") else {
            throw CocoaError(.fileNoSuchFile)
        }
        return try load(from: url)
    }

    public func isPrefix(_ text: String) -> Bool { prefixes.contains(text) }

    /// Entries matching the segments: equal syllables for full ones, prefixes for partial ones.
    public func match(_ segments: [PinyinSegment]) -> [PinyinEntry] {
        let signature = String(segments.map { $0.text.first ?? " " })
        guard let bucket = bySignature[signature] else { return [] }
        return bucket.filter { entry in
            segments.enumerated().allSatisfy { i, segment in
                segment.full ? entry.syllables[i] == segment.text : entry.syllables[i].hasPrefix(segment.text)
            }
        }
    }
}

private struct Step {
    var cost = 0.0
    var segments: [PinyinSegment] = []
}

/// Splits lowercase letters into syllables. An apostrophe forces a break.
/// Prefers whole syllables, then a partial final one, then bare initials
/// (abbreviations such as "zg" for zhong'guo). Nil when the letters cannot
/// be read as pinyin.
public func pinyinSegments(_ dict: PinyinDict, _ letters: String) -> [PinyinSegment]? {
    var out: [PinyinSegment] = []
    var offset = 0
    for chunk in letters.split(separator: "'", omittingEmptySubsequences: false) {
        if !chunk.isEmpty {
            guard let parts = segmentChunk(dict, Array(chunk), offset) else { return nil }
            out.append(contentsOf: parts)
        }
        offset += chunk.count + 1
    }
    return out.isEmpty ? nil : out
}

private func segmentChunk(_ dict: PinyinDict, _ chunk: [Character], _ offset: Int) -> [PinyinSegment]? {
    var best = [Step?](repeating: nil, count: chunk.count + 1)
    best[0] = Step()
    for i in 0..<chunk.count {
        guard let from = best[i] else { continue }
        for j in (i + 1)...min(chunk.count, i + 6) {
            let piece = String(chunk[i..<j])
            let cost: Double
            let full: Bool
            if dict.syllables.contains(piece) {
                cost = 1; full = true
            } else if j == chunk.count && dict.isPrefix(piece) {
                cost = 1.5; full = false
            } else if initials.contains(piece) {
                cost = 3; full = false
            } else {
                continue
            }
            let total = from.cost + cost
            // Ties keep the earlier, longer first syllable ("xian", not "xi'an").
            if best[j] == nil || total < best[j]!.cost {
                best[j] = Step(cost: total, segments: from.segments + [PinyinSegment(text: piece, full: full, start: offset + i, end: offset + j)])
            }
        }
    }
    return best[chunk.count]?.segments
}

/// The typing state of one Chinese input: letters being composed, and words already picked from them.
public final class PinyinComposer {
    private let dict: PinyinDict
    /// Letters (and apostrophes) not yet turned into words.
    public var composing = ""
    /// Words picked from the start of the letters, committed once nothing is left.
    public private(set) var picked = ""
    private var learned: [String: Int] = [:]

    public init(dict: PinyinDict) { self.dict = dict }

    public var active: Bool { !composing.isEmpty || !picked.isEmpty }

    /// The composing letters with syllable breaks, as the candidate bar shows them.
    public func display() -> String {
        let letters = pinyinSegments(dict, composing).map { $0.map(\.text).joined(separator: "'") } ?? composing
        return picked + letters
    }

    public func type(_ letter: String) { composing += letter }

    /// Removes the last letter, or the last picked word when no letters are left.
    public func backspace() {
        if !composing.isEmpty { composing.removeLast() } else if !picked.isEmpty { picked.removeLast() }
    }

    public func clear() { composing = ""; picked = "" }

    private func score(_ entry: PinyinEntry) -> Double { entry.freq * Double(1 + (learned[entry.word] ?? 0)) }

    private func ranked(_ entries: [PinyinEntry]) -> [PinyinEntry] {
        entries.enumerated().sorted { a, b in
            let sa = score(a.element), sb = score(b.element)
            return sa > sb || (sa == sb && a.offset < b.offset)
        }.map(\.element)
    }

    /// Words for the composing letters: whole matches first, then a composed sentence, then shorter words.
    public func candidates() -> [PinyinCandidate] {
        guard let segments = pinyinSegments(dict, composing) else { return [] }
        var out: [PinyinCandidate] = []
        var seen: Set<String> = []
        func add(_ word: String, _ count: Int) {
            if seen.contains(word) || out.count >= maxCandidates { return }
            seen.insert(word)
            out.append(PinyinCandidate(word: word, consumed: consumedBy(segments, count)))
        }
        let whole = ranked(dict.match(segments))
        let sentence = segments.count > 1 ? self.sentence(segments) : ""
        if whole.isEmpty && !sentence.isEmpty { add(sentence, segments.count) }
        // An abbreviation's best guesses come first only when they are common words.
        for entry in whole.prefix(8) { add(entry.word, segments.count) }
        if !whole.isEmpty && !sentence.isEmpty { add(sentence, segments.count) }
        for entry in whole.dropFirst(8) { add(entry.word, segments.count) }
        if segments.count > 1 {
            for count in stride(from: segments.count - 1, through: 1, by: -1) {
                for entry in ranked(dict.match(Array(segments.prefix(count)))) { add(entry.word, count) }
            }
        }
        return out
    }

    /// Characters of the composing letters covered by the first `count` segments (and a following apostrophe).
    private func consumedBy(_ segments: [PinyinSegment], _ count: Int) -> Int {
        var end = segments[count - 1].end
        let letters = Array(composing)
        while end < letters.count && letters[end] == "'" { end += 1 }
        return end
    }

    /// Greedy longest-word cover of all segments, or "" when some segment has no character at all.
    private func sentence(_ segments: [PinyinSegment]) -> String {
        var text = ""
        var i = 0
        while i < segments.count {
            var chosen = ""
            var length = 0
            var k = min(4, segments.count - i)
            while k >= 1 && chosen.isEmpty {
                let best = ranked(dict.match(Array(segments[i..<(i + k)])))
                if let first = best.first { chosen = first.word; length = k }
                k -= 1
            }
            if chosen.isEmpty { return "" }
            text += chosen
            i += length
        }
        return text
    }

    /// Picks a candidate. Returns the text to commit when the composition is
    /// finished, or nil while letters remain to be turned into words.
    public func pick(_ candidate: PinyinCandidate) -> String? {
        learned[candidate.word, default: 0] += 1
        picked += candidate.word
        composing = String(composing.dropFirst(candidate.consumed))
        if !composing.replacingOccurrences(of: "'", with: "").isEmpty { return nil }
        let text = picked
        clear()
        return text
    }

    /// Commits whatever is pending: picked words plus the letters as typed.
    public func commitRaw() -> String {
        let text = picked + composing.replacingOccurrences(of: "'", with: "")
        clear()
        return text
    }

    /// Commits the first candidate (space), or the raw letters when there is none.
    public func commitFirst() -> String {
        while !composing.isEmpty {
            guard let first = candidates().first else { return commitRaw() }
            if let done = pick(first) { return done }
        }
        return commitRaw()
    }
}

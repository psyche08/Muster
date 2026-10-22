// The keyboard's pinyin engine against the bundled dictionary (the same
// checks as the HarmonyOS shell's tests/pinyin.test.cjs).
import XCTest
@testable import MusterCore

final class PinyinEngineTests: XCTestCase {
    static let dict = try! PinyinDict.bundled()
    var dict: PinyinDict { Self.dict }

    private func split(_ letters: String) -> [String] {
        (pinyinSegments(dict, letters) ?? []).map { ($0.full ? "" : "~") + $0.text }
    }

    private func words(_ letters: String) -> [String] {
        let composer = PinyinComposer(dict: dict)
        composer.composing = letters
        return composer.candidates().map(\.word)
    }

    private func find(_ composer: PinyinComposer, _ word: String) throws -> PinyinCandidate {
        try XCTUnwrap(composer.candidates().first { $0.word == word }, word)
    }

    func testTheDictionaryLoads() {
        XCTAssertTrue(dict.syllables.contains("zhuang"))
        XCTAssertGreaterThan(dict.syllables.count, 380)
    }

    func testLettersSplitIntoWholeSyllablesPreferringLongerOnes() {
        XCTAssertEqual(split("nihao"), ["ni", "hao"])
        XCTAssertEqual(split("zhongguoren"), ["zhong", "guo", "ren"])
        XCTAssertEqual(split("xian"), ["xian"])
        XCTAssertEqual(split("xi'an"), ["xi", "an"])
    }

    func testATrailingPartialSyllableAndBareInitialsArePrefixes() {
        XCTAssertEqual(split("nih"), ["ni", "~h"])
        XCTAssertEqual(split("zhongg"), ["zhong", "~g"])
        XCTAssertEqual(split("zg"), ["~z", "~g"])
        XCTAssertNil(pinyinSegments(dict, "vvv"))
    }

    func testWholeWordsComeFirstByFrequency() {
        XCTAssertEqual(words("nihao").first, "你好")
        XCTAssertEqual(words("zhongguo").first, "中国")
        XCTAssertEqual(words("shurufa").first, "输入法")
        XCTAssertEqual(words("de").first, "的")
    }

    func testPartialSyllablesAndAbbreviationsFindWords() {
        XCTAssertEqual(words("nih").first, "你好")
        XCTAssertTrue(words("zg").prefix(5).contains("中国"))
        XCTAssertTrue(words("jianp").contains("键盘"))
    }

    func testASentenceIsComposedWhenNoSingleWordCoversTheLetters() {
        let composer = PinyinComposer(dict: dict)
        composer.composing = "woxihuanzhegejianpan"
        XCTAssertEqual(composer.candidates().first?.word, "我喜欢这个键盘")
    }

    func testShorterWordsFollowAndConsumeOnlyTheirOwnLetters() throws {
        let composer = PinyinComposer(dict: dict)
        composer.composing = "nihaoshijie"
        let ni = try find(composer, "你好")
        XCTAssertEqual(ni.consumed, 5)
        XCTAssertNil(composer.pick(ni))
        XCTAssertEqual(composer.composing, "shijie")
        XCTAssertEqual(composer.display(), "你好shi'jie")
        XCTAssertEqual(composer.pick(try find(composer, "世界")), "你好世界")
        XCTAssertFalse(composer.active)
    }

    func testApostrophesAreConsumedWithTheWordBeforeThem() throws {
        let composer = PinyinComposer(dict: dict)
        composer.composing = "xi'an"
        let xi = try find(composer, "西")
        XCTAssertEqual(xi.consumed, 3)
        _ = composer.pick(xi)
        XCTAssertEqual(composer.composing, "an")
    }

    func testSpaceCommitsTheBestReadingEnterCommitsTheLetters() {
        let composer = PinyinComposer(dict: dict)
        composer.composing = "nihao"
        XCTAssertEqual(composer.commitFirst(), "你好")
        composer.composing = "hello"
        XCTAssertEqual(composer.commitRaw(), "hello")
        composer.composing = "ni'hao"
        XCTAssertEqual(composer.commitRaw(), "nihao")
    }

    func testBackspaceRemovesLettersThenPickedWords() throws {
        let composer = PinyinComposer(dict: dict)
        composer.composing = "nihaoshi"
        _ = composer.pick(try find(composer, "你好"))
        composer.backspace(); composer.backspace(); composer.backspace()
        XCTAssertEqual(composer.composing, "")
        XCTAssertEqual(composer.picked, "你好")
        composer.backspace()
        XCTAssertEqual(composer.picked, "你")
        composer.backspace()
        XCTAssertFalse(composer.active)
    }

    func testPickingAWordRaisesItNextTime() throws {
        let composer = PinyinComposer(dict: dict)
        composer.composing = "shi"
        let rare = composer.candidates()[6].word
        _ = composer.pick(try find(composer, rare))
        for _ in 0..<40 {
            composer.composing = "shi"
            _ = composer.pick(try find(composer, rare))
        }
        composer.composing = "shi"
        XCTAssertLessThan(try XCTUnwrap(composer.candidates().map(\.word).firstIndex(of: rare)), 6)
    }
}

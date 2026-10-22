package dev.linsheng.muster.app

import dev.linsheng.muster.app.input.PinyinCandidate
import dev.linsheng.muster.app.input.PinyinComposer
import dev.linsheng.muster.app.input.PinyinDict
import dev.linsheng.muster.app.input.pinyinSegments
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** The keyboard's pinyin engine against the bundled dictionary (the same checks as the HarmonyOS shell's tests). */
class PinyinEngineTest {
    companion object {
        val dict: PinyinDict = PinyinDict.load(File("src/main/assets/pinyin.txt").inputStream())
    }

    private fun split(letters: String) = (pinyinSegments(dict, letters) ?: emptyList()).map { (if (it.full) "" else "~") + it.text }
    private fun words(letters: String) = PinyinComposer(dict).apply { composing = letters }.candidates().map { it.word }
    private fun PinyinComposer.find(word: String): PinyinCandidate = candidates().first { it.word == word }

    @Test fun theDictionaryLoads() {
        assertTrue("zhuang" in dict.syllables)
        assertTrue(dict.syllables.size > 380)
    }

    @Test fun lettersSplitIntoWholeSyllablesPreferringLongerOnes() {
        assertEquals(listOf("ni", "hao"), split("nihao"))
        assertEquals(listOf("zhong", "guo", "ren"), split("zhongguoren"))
        assertEquals(listOf("xian"), split("xian"))
        assertEquals(listOf("xi", "an"), split("xi'an"))
    }

    @Test fun aTrailingPartialSyllableAndBareInitialsArePrefixes() {
        assertEquals(listOf("ni", "~h"), split("nih"))
        assertEquals(listOf("zhong", "~g"), split("zhongg"))
        assertEquals(listOf("~z", "~g"), split("zg"))
        assertNull(pinyinSegments(dict, "vvv"))
    }

    @Test fun wholeWordsComeFirstByFrequency() {
        assertEquals("你好", words("nihao")[0])
        assertEquals("中国", words("zhongguo")[0])
        assertEquals("输入法", words("shurufa")[0])
        assertEquals("的", words("de")[0])
    }

    @Test fun partialSyllablesAndAbbreviationsFindWords() {
        assertEquals("你好", words("nih")[0])
        assertTrue("中国" in words("zg").take(5))
        assertTrue("键盘" in words("jianp"))
    }

    @Test fun aSentenceIsComposedWhenNoSingleWordCoversTheLetters() {
        val c = PinyinComposer(dict).apply { composing = "woxihuanzhegejianpan" }
        assertEquals("我喜欢这个键盘", c.candidates()[0].word)
    }

    @Test fun shorterWordsFollowAndConsumeOnlyTheirOwnLetters() {
        val c = PinyinComposer(dict).apply { composing = "nihaoshijie" }
        val ni = c.find("你好")
        assertEquals(5, ni.consumed)
        assertNull(c.pick(ni))
        assertEquals("shijie", c.composing)
        assertEquals("你好shi'jie", c.display())
        assertEquals("你好世界", c.pick(c.find("世界")))
        assertFalse(c.active)
    }

    @Test fun apostrophesAreConsumedWithTheWordBeforeThem() {
        val c = PinyinComposer(dict).apply { composing = "xi'an" }
        val xi = c.find("西")
        assertEquals(3, xi.consumed)
        c.pick(xi)
        assertEquals("an", c.composing)
    }

    @Test fun spaceCommitsTheBestReadingEnterCommitsTheLetters() {
        val c = PinyinComposer(dict).apply { composing = "nihao" }
        assertEquals("你好", c.commitFirst())
        c.composing = "hello"
        assertEquals("hello", c.commitRaw())
        c.composing = "ni'hao"
        assertEquals("nihao", c.commitRaw())
    }

    @Test fun backspaceRemovesLettersThenPickedWords() {
        val c = PinyinComposer(dict).apply { composing = "nihaoshi" }
        c.pick(c.find("你好"))
        c.backspace(); c.backspace(); c.backspace()
        assertEquals("", c.composing)
        assertEquals("你好", c.picked)
        c.backspace()
        assertEquals("你", c.picked)
        c.backspace()
        assertFalse(c.active)
    }

    @Test fun pickingAWordRaisesItNextTime() {
        val c = PinyinComposer(dict).apply { composing = "shi" }
        val rare = c.candidates()[6].word
        c.pick(c.find(rare))
        repeat(40) { c.composing = "shi"; c.pick(c.find(rare)) }
        c.composing = "shi"
        assertTrue(c.candidates().map { it.word }.indexOf(rare) < 6)
    }
}

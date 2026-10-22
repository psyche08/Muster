package dev.linsheng.muster.app

import dev.linsheng.muster.app.input.ChinesePunctuation
import dev.linsheng.muster.app.input.MAC_FUNCTION_ROW
import dev.linsheng.muster.app.input.MAC_ROWS
import dev.linsheng.muster.app.input.MAC_ROW_UNITS
import dev.linsheng.muster.app.input.MacKey
import dev.linsheng.muster.app.input.MacKeyKind
import dev.linsheng.muster.app.input.TextEdits
import org.junit.Assert.assertEquals
import org.junit.Test

/** The keyboard's MacBook layout and its caret arithmetic (the same checks as the HarmonyOS shell's tests). */
class KeyboardLayoutTest {
    private fun units(row: List<MacKey>) = row.sumOf { it.weight }
    private fun ids(row: List<MacKey>) = row.joinToString(" ") { it.id }

    @Test fun everyRowSpansAMacBookRowSoTheColumnsLineUp() {
        for (row in listOf(MAC_FUNCTION_ROW) + MAC_ROWS) assertEquals(ids(row), MAC_ROW_UNITS, units(row), 1e-9)
    }

    @Test fun theRowsHoldTheMacBookKeysInOrder() {
        assertEquals("` 1 2 3 4 5 6 7 8 9 0 - = delete", ids(MAC_ROWS[0]))
        assertEquals("tab q w e r t y u i o p [ ] \\", ids(MAC_ROWS[1]))
        assertEquals("caps a s d f g h j k l ; ' return", ids(MAC_ROWS[2]))
        assertEquals("shift z x c v b n m , . / shift", ids(MAC_ROWS[3]))
        assertEquals("fn control option command space command option arrows", ids(MAC_ROWS[4]))
        assertEquals("escape f1 f2 f3 f4 f5 f6 f7 f8 f9 f10 f11 f12", ids(MAC_FUNCTION_ROW))
    }

    @Test fun symbolKeysCarryTheirShiftedCharacter() {
        assertEquals("~!@#$%^&*()_+", MAC_ROWS[0].filter { it.kind == MacKeyKind.SYMBOL }.joinToString("") { it.top })
        assertEquals("?", MAC_ROWS[3].first { it.id == "/" }.top)
    }

    @Test fun chinesePunctuationFollowsMacOSPinyinQuotesAlternate() {
        val zh = ChinesePunctuation()
        assertEquals(listOf("，", "。", "、", "￥", "……", "《", "【", "1", "a"),
            listOf(",", ".", "\\", "$", "^", "<", "[", "1", "a").map { zh.map(it) })
        assertEquals(listOf("“", "”", "‘", "’"), listOf("\"", "\"", "'", "'").map { zh.map(it) })
    }

    @Test fun arrowsMoveByCharacterWithoutSplittingSurrogatePairs() {
        val text = "a😀b"
        assertEquals(3, TextEdits.charRight(text, 1))
        assertEquals(1, TextEdits.charLeft(text, 3))
        assertEquals(0, TextEdits.charLeft(text, 0))
        assertEquals(4, TextEdits.charRight(text, 4))
    }

    @Test fun upAndDownKeepTheColumnClampedToShorterLines() {
        val text = "hello\nhi\nworld"
        assertEquals(8, TextEdits.lineDown(text, 4))
        assertEquals(11, TextEdits.lineDown(text, 8))
        assertEquals(8, TextEdits.lineUp(text, 11))
        assertEquals(8, TextEdits.lineUp(text, 13))
        assertEquals(2, TextEdits.lineUp(text, 8))
        assertEquals(0, TextEdits.lineUp(text, 2))
        assertEquals(text.length, TextEdits.lineDown(text, 12))
    }

    @Test fun lineStartAndEndAreTheTextLines() {
        val text = "\nab\ncd"
        assertEquals(0, TextEdits.lineStart(text, 0))
        assertEquals(1, TextEdits.lineStart(text, 3))
        assertEquals(3, TextEdits.lineEnd(text, 1))
        assertEquals(6, TextEdits.lineEnd(text, 5))
    }

    @Test fun wordsSkipSeparatorsThenAWordChinesePunctuationIncluded() {
        val text = "git push, origin；你好 world"
        assertEquals(text.length - 5, TextEdits.wordLeft(text, text.length))
        assertEquals(10, TextEdits.wordLeft(text, 16))
        assertEquals(16, TextEdits.wordRight(text, 8))
        assertEquals(3, TextEdits.wordRight(text, 0))
        assertEquals(0, TextEdits.wordLeft(text, 0))
    }
}

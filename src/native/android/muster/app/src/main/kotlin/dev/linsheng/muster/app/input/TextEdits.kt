package dev.linsheng.muster.app.input

// Caret arithmetic for the keyboard's arrow, word and line keys, on plain
// text: lines are the text's own lines (not soft wraps), and offsets are
// UTF-16 indices (what TextFieldValue's selection uses) that never split a
// surrogate pair. No platform calls, so the JUnit tests run it on the host.
object TextEdits {
    private fun isLowSurrogate(value: String, index: Int): Boolean =
        index in value.indices && Character.isLowSurrogate(value[index])

    fun charLeft(value: String, pos: Int): Int {
        if (pos <= 0) return 0
        return if (pos >= 2 && isLowSurrogate(value, pos - 1)) pos - 2 else pos - 1
    }

    fun charRight(value: String, pos: Int): Int {
        if (pos >= value.length) return value.length
        return if (pos + 1 < value.length && isLowSurrogate(value, pos + 1)) pos + 2 else pos + 1
    }

    fun lineStart(value: String, pos: Int): Int = if (pos <= 0) 0 else value.lastIndexOf('\n', pos - 1) + 1

    fun lineEnd(value: String, pos: Int): Int {
        val index = value.indexOf('\n', maxOf(pos, 0))
        return if (index < 0) value.length else index
    }

    /** The same column on the line above, or the start of the text from the first line. */
    fun lineUp(value: String, pos: Int): Int {
        val start = lineStart(value, pos)
        if (start == 0) return 0
        return minOf(lineStart(value, start - 1) + pos - start, start - 1)
    }

    /** The same column on the line below, or the end of the text from the last line. */
    fun lineDown(value: String, pos: Int): Int {
        val end = lineEnd(value, pos)
        if (end == value.length) return value.length
        return minOf(end + 1 + pos - lineStart(value, pos), lineEnd(value, end + 1))
    }

    private val SEPARATORS: Set<Char> = " \t\n\r.,;:!?'\"`~@#$%^&*()[]{}<>/\\|+=-，。；：！？、（）【】《》「」“”‘’…—·".toSet()

    private fun isWord(value: String, index: Int): Boolean = index in value.indices && value[index] !in SEPARATORS

    /** Back over separators, then to the start of the word before them. */
    fun wordLeft(value: String, pos: Int): Int {
        var index = minOf(pos, value.length)
        while (index > 0 && !isWord(value, index - 1)) index--
        while (index > 0 && isWord(value, index - 1)) index--
        return index
    }

    /** On over separators, then to the end of the word after them. */
    fun wordRight(value: String, pos: Int): Int {
        var index = maxOf(pos, 0)
        while (index < value.length && !isWord(value, index)) index++
        while (index < value.length && isWord(value, index)) index++
        return index
    }
}

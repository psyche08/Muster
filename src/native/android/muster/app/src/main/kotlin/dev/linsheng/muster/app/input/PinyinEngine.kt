package dev.linsheng.muster.app.input

import java.io.InputStream

// Pinyin to Chinese for the in-app keyboard: splits typed letters into
// syllables (full ones, a trailing partial one, or bare initials as an
// abbreviation), looks words up in the bundled dictionary and composes a
// sentence when no single word covers everything. Pure text in, text out:
// no platform calls, so the JUnit tests run it on the host. The same engine
// as the HarmonyOS shell's PinyinEngine.ts, with the same dictionary.

class PinyinEntry(val syllables: List<String>, val word: String, val freq: Double)

/** One piece of the typed letters: a whole syllable, or a prefix of one. */
data class PinyinSegment(val text: String, val full: Boolean, val start: Int, val end: Int)

/** A word and how many typed characters (letters and apostrophes) it replaces. */
data class PinyinCandidate(val word: String, val consumed: Int)

private val INITIALS = setOf("b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x",
    "zh", "ch", "sh", "r", "z", "c", "s", "y", "w")
private const val MAX_CANDIDATES = 80

class PinyinDict private constructor() {
    val syllables = HashSet<String>()
    private val prefixes = HashSet<String>()
    /** First letter of each syllable → entries, highest frequency first. */
    private val bySignature = HashMap<String, MutableList<PinyinEntry>>()

    fun isPrefix(text: String): Boolean = text in prefixes

    /** Entries matching the segments: equal syllables for full ones, prefixes for partial ones. */
    fun match(segments: List<PinyinSegment>): List<PinyinEntry> {
        val bucket = bySignature[segments.joinToString("") { it.text.take(1) }] ?: return emptyList()
        return bucket.filter { entry ->
            segments.withIndex().all { (i, segment) ->
                if (segment.full) entry.syllables[i] == segment.text else entry.syllables[i].startsWith(segment.text)
            }
        }
    }

    companion object {
        /** Lines of "syl'syl<TAB>word<TAB>frequency"; "#" starts a comment. */
        fun parse(text: String): PinyinDict {
            val dict = PinyinDict()
            for (line in text.lineSequence()) {
                if (line.isEmpty() || line.startsWith("#")) continue
                val parts = line.split('\t')
                if (parts.size < 3) continue
                val syllables = parts[0].split('\'')
                val word = parts[1]
                val freq = parts[2].trim().toDoubleOrNull()?.takeIf { it != 0.0 } ?: 1.0
                if (syllables.isEmpty() || syllables.size != word.codePointCount(0, word.length)) continue
                for (syllable in syllables) {
                    if (dict.syllables.add(syllable)) {
                        for (i in 1..syllable.length) dict.prefixes.add(syllable.substring(0, i))
                    }
                }
                val signature = syllables.joinToString("") { it.take(1) }
                dict.bySignature.getOrPut(signature) { ArrayList() }.add(PinyinEntry(syllables, word, freq))
            }
            // sortedByDescending is stable: equal frequencies keep the dictionary's order.
            for (entry in dict.bySignature.entries) entry.setValue(entry.value.sortedByDescending { it.freq }.toMutableList())
            return dict
        }

        fun load(stream: InputStream): PinyinDict = stream.bufferedReader(Charsets.UTF_8).use { parse(it.readText()) }
    }
}

private class Step(val cost: Double = 0.0, val segments: List<PinyinSegment> = emptyList())

/**
 * Splits lowercase letters into syllables. An apostrophe forces a break.
 * Prefers whole syllables, then a partial final one, then bare initials
 * (abbreviations such as "zg" for zhong'guo). Null when the letters cannot
 * be read as pinyin.
 */
fun pinyinSegments(dict: PinyinDict, letters: String): List<PinyinSegment>? {
    val out = ArrayList<PinyinSegment>()
    var offset = 0
    for (chunk in letters.split('\'')) {
        if (chunk.isNotEmpty()) {
            val parts = segmentChunk(dict, chunk, offset) ?: return null
            out.addAll(parts)
        }
        offset += chunk.length + 1
    }
    return out.ifEmpty { null }
}

private fun segmentChunk(dict: PinyinDict, chunk: String, offset: Int): List<PinyinSegment>? {
    val best = arrayOfNulls<Step>(chunk.length + 1)
    best[0] = Step()
    for (i in chunk.indices) {
        val from = best[i] ?: continue
        for (j in i + 1..minOf(chunk.length, i + 6)) {
            val piece = chunk.substring(i, j)
            val cost: Double
            val full: Boolean
            when {
                piece in dict.syllables -> { cost = 1.0; full = true }
                j == chunk.length && dict.isPrefix(piece) -> { cost = 1.5; full = false }
                piece in INITIALS -> { cost = 3.0; full = false }
                else -> continue
            }
            val total = from.cost + cost
            val current = best[j]
            // Ties keep the earlier, longer first syllable ("xian", not "xi'an").
            if (current == null || total < current.cost) {
                best[j] = Step(total, from.segments + PinyinSegment(piece, full, offset + i, offset + j))
            }
        }
    }
    return best[chunk.length]?.segments
}

/** The typing state of one Chinese input: letters being composed, and words already picked from them. */
class PinyinComposer(private val dict: PinyinDict) {
    /** Letters (and apostrophes) not yet turned into words. */
    var composing = ""
    /** Words picked from the start of the letters, committed once nothing is left. */
    var picked = ""
        private set
    private val learned = HashMap<String, Int>()

    val active: Boolean get() = composing.isNotEmpty() || picked.isNotEmpty()

    /** The composing letters with syllable breaks, as the candidate bar shows them. */
    fun display(): String {
        val letters = pinyinSegments(dict, composing)?.joinToString("'") { it.text } ?: composing
        return picked + letters
    }

    fun type(letter: String) { composing += letter }

    /** Removes the last letter, or the last picked word when no letters are left. */
    fun backspace() {
        if (composing.isNotEmpty()) {
            composing = composing.substring(0, composing.length - 1)
        } else if (picked.isNotEmpty()) {
            picked = picked.substring(0, picked.offsetByCodePoints(picked.length, -1))
        }
    }

    fun clear() { composing = ""; picked = "" }

    private fun score(entry: PinyinEntry): Double = entry.freq * (1 + (learned[entry.word] ?: 0))

    private fun ranked(entries: List<PinyinEntry>): List<PinyinEntry> = entries.sortedByDescending { score(it) }

    /** Words for the composing letters: whole matches first, then a composed sentence, then shorter words. */
    fun candidates(): List<PinyinCandidate> {
        val segments = pinyinSegments(dict, composing) ?: return emptyList()
        val out = ArrayList<PinyinCandidate>()
        val seen = HashSet<String>()
        fun add(word: String, count: Int) {
            if (word in seen || out.size >= MAX_CANDIDATES) return
            seen.add(word)
            out.add(PinyinCandidate(word, consumedBy(segments, count)))
        }
        val whole = ranked(dict.match(segments))
        val sentence = if (segments.size > 1) sentence(segments) else ""
        if (whole.isEmpty() && sentence.isNotEmpty()) add(sentence, segments.size)
        // An abbreviation's best guesses come first only when they are common words.
        for (entry in whole.take(8)) add(entry.word, segments.size)
        if (whole.isNotEmpty() && sentence.isNotEmpty()) add(sentence, segments.size)
        for (entry in whole.drop(8)) add(entry.word, segments.size)
        for (count in segments.size - 1 downTo 1) {
            for (entry in ranked(dict.match(segments.subList(0, count)))) add(entry.word, count)
        }
        return out
    }

    /** Characters of the composing letters covered by the first `count` segments (and a following apostrophe). */
    private fun consumedBy(segments: List<PinyinSegment>, count: Int): Int {
        var end = segments[count - 1].end
        while (end < composing.length && composing[end] == '\'') end++
        return end
    }

    /** Greedy longest-word cover of all segments, or "" when some segment has no character at all. */
    private fun sentence(segments: List<PinyinSegment>): String {
        val text = StringBuilder()
        var i = 0
        while (i < segments.size) {
            var chosen = ""
            var length = 0
            var k = minOf(4, segments.size - i)
            while (k >= 1 && chosen.isEmpty()) {
                val best = ranked(dict.match(segments.subList(i, i + k)))
                if (best.isNotEmpty()) { chosen = best[0].word; length = k }
                k--
            }
            if (chosen.isEmpty()) return ""
            text.append(chosen)
            i += length
        }
        return text.toString()
    }

    /**
     * Picks a candidate. Returns the text to commit when the composition is
     * finished, or null while letters remain to be turned into words.
     */
    fun pick(candidate: PinyinCandidate): String? {
        learned[candidate.word] = (learned[candidate.word] ?: 0) + 1
        picked += candidate.word
        composing = composing.substring(minOf(candidate.consumed, composing.length))
        if (composing.replace("'", "").isNotEmpty()) return null
        val text = picked
        clear()
        return text
    }

    /** Commits whatever is pending: picked words plus the letters as typed. */
    fun commitRaw(): String {
        val text = picked + composing.replace("'", "")
        clear()
        return text
    }

    /** Commits the first candidate (space), or the raw letters when there is none. */
    fun commitFirst(): String {
        while (composing.isNotEmpty()) {
            val first = candidates().firstOrNull() ?: return commitRaw()
            val done = pick(first)
            if (done != null) return done
        }
        return commitRaw()
    }
}

package dev.linsheng.muster.app

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import dev.linsheng.muster.core.MusterCore

// A small Markdown renderer for agent replies: headings, paragraphs, list
// items, quotes, fenced code, and inline code, bold and links. It draws text
// only: images stay links, and nothing is fetched (rendering must not add
// network traffic outside Shared Rust). Web links open in the browser on a
// tap; other links go to the core, which resolves files in the session.

/** paragraph | heading | item | quote | code | rule */
class MdBlock(val kind: String, val text: String = "", val level: Int = 0)

/** text | code | bold | link */
class MdSpan(val kind: String, val text: String, val url: String = "")

private val FENCE = Regex("""^\s*(```|~~~)""")
private val HEADING = Regex("""^(#{1,6})\s+(.*)$""")
private val ITEM = Regex("""^(\s*)([-*+]|\d+[.)])\s+(.*)$""")
private val QUOTE = Regex("""^>\s?(.*)$""")
private val RULE = Regex("""^\s*([-*_])(\s*\1){2,}\s*$""")
private val INLINE = Regex("""(`[^`]+`)|(\*\*[^*]+\*\*)|(!?\[[^\]]*\]\([^)\s]+\))""")
private val LINK = Regex("""^!?\[([^\]]*)\]\(([^)\s]+)\)$""")

fun parseMarkdown(text: String): List<MdBlock> {
    val blocks = ArrayList<MdBlock>()
    val lines = text.replace("\r\n", "\n").split('\n')
    val paragraph = ArrayList<String>()
    fun flush() {
        if (paragraph.isNotEmpty()) {
            blocks.add(MdBlock("paragraph", paragraph.joinToString("\n")))
            paragraph.clear()
        }
    }
    var i = 0
    while (i < lines.size) {
        val line = lines[i]
        if (FENCE.containsMatchIn(line)) {
            flush()
            val code = ArrayList<String>()
            i++
            while (i < lines.size && !FENCE.containsMatchIn(lines[i])) { code.add(lines[i]); i++ }
            blocks.add(MdBlock("code", code.joinToString("\n")))
            i++
            continue
        }
        val heading = HEADING.find(line)
        val item = ITEM.find(line)
        val quote = QUOTE.find(line)
        when {
            line.isBlank() -> flush()
            RULE.containsMatchIn(line) -> { flush(); blocks.add(MdBlock("rule")) }
            heading != null -> { flush(); blocks.add(MdBlock("heading", heading.groupValues[2], heading.groupValues[1].length)) }
            item != null -> {
                flush()
                val marker = item.groupValues[2]
                val bullet = if (marker == "-" || marker == "*" || marker == "+") "•" else marker
                blocks.add(MdBlock("item", "$bullet ${item.groupValues[3]}", item.groupValues[1].length / 2))
            }
            quote != null -> { flush(); blocks.add(MdBlock("quote", quote.groupValues[1])) }
            else -> paragraph.add(line)
        }
        i++
    }
    flush()
    return blocks
}

fun parseInline(text: String): List<MdSpan> {
    val spans = ArrayList<MdSpan>()
    var last = 0
    for (match in INLINE.findAll(text)) {
        if (match.range.first > last) spans.add(MdSpan("text", text.substring(last, match.range.first)))
        val token = match.value
        spans.add(
            when {
                token.startsWith("`") -> MdSpan("code", token.substring(1, token.length - 1))
                token.startsWith("**") -> MdSpan("bold", token.substring(2, token.length - 2))
                else -> {
                    val link = LINK.find(token)
                    val label = if (link != null && link.groupValues[1].isNotEmpty()) link.groupValues[1]
                        else if (token.startsWith("!")) "Image" else token
                    MdSpan("link", label, link?.groupValues?.get(2) ?: "")
                }
            }
        )
        last = match.range.last + 1
    }
    if (last < text.length || spans.isEmpty()) spans.add(MdSpan("text", text.substring(last)))
    return spans
}

@Composable
fun ChatMarkdown(text: String, core: MusterCore?, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val open: (String) -> Unit = { url ->
        if (url.startsWith("http://") || url.startsWith("https://") || url.startsWith("mailto:")) {
            runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
        } else {
            core?.openFileLink(url)
        }
    }
    val blocks = remember(text) { parseMarkdown(text) }
    val body = MaterialTheme.typography.bodyLarge
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        blocks.forEach { block ->
            when (block.kind) {
                "code" -> Box(
                    Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp))
                        .horizontalScroll(rememberScrollState()).padding(12.dp),
                ) {
                    Text(block.text, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
                "rule" -> HorizontalDivider()
                "quote" -> Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
                    Box(Modifier.width(3.dp).fillMaxHeight().background(MaterialTheme.colorScheme.outlineVariant))
                    Inline(block.text, body, Modifier.padding(start = 12.dp).alpha(0.75f), open = open)
                }
                "heading" -> Inline(
                    block.text,
                    when (block.level) { 1 -> MaterialTheme.typography.titleLarge; 2 -> MaterialTheme.typography.titleMedium; else -> MaterialTheme.typography.titleSmall },
                    bold = true, open = open,
                )
                "item" -> Inline(block.text, body, Modifier.padding(start = (8 + block.level * 16).dp), open = open)
                else -> Inline(block.text, body, open = open)
            }
        }
    }
}

@Composable
private fun Inline(text: String, style: TextStyle, modifier: Modifier = Modifier, bold: Boolean = false, open: (String) -> Unit) {
    val link = MaterialTheme.colorScheme.primary
    val codeBackground = MaterialTheme.colorScheme.surfaceVariant
    val annotated = remember(text, link, codeBackground) {
        buildAnnotatedString {
            parseInline(text).forEach { span ->
                when (span.kind) {
                    "code" -> withStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = codeBackground)) { append(span.text) }
                    "bold" -> withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(span.text) }
                    "link" -> withLink(
                        LinkAnnotation.Clickable(span.url, TextLinkStyles(SpanStyle(color = link, textDecoration = TextDecoration.Underline))) {
                            open(span.url)
                        },
                    ) { append(span.text) }
                    else -> append(span.text)
                }
            }
        }
    }
    Text(annotated, modifier.fillMaxWidth(), style = if (bold) style.copy(fontWeight = FontWeight.Bold) else style)
}

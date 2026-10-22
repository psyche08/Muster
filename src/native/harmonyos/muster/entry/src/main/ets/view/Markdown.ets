// A small Markdown renderer for agent replies: headings, paragraphs, list
// items, quotes, fenced code, and inline code, bold and links. It draws text
// only: images stay links, and nothing is fetched (rendering must not add
// network traffic outside Shared Rust). Web links open in the browser on a
// tap; other links go to the core, which resolves files in the session.
import { common } from '@kit.AbilityKit';
import { MusterCore } from '../core/MusterCore';
import { MdBlock, MdSpan, parseInline, parseMarkdown } from './MarkdownParse';

@ComponentV2
export struct ChatMarkdown {
  @Param @Require text: string;
  @Param core: MusterCore | null = null;

  private open(url: string): void {
    if (url.startsWith('http://') || url.startsWith('https://') || url.startsWith('mailto:')) {
      const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
      context.openLink(url).catch((): void => {});
    } else {
      this.core?.openFileLink(url);
    }
  }

  build() {
    Column({ space: 10 }) {
      ForEach(parseMarkdown(this.text), (block: MdBlock): void => {
        if (block.kind === 'code') {
          Scroll() {
            Text(block.text).fontSize(13).fontFamily('monospace').copyOption(CopyOptions.LocalDevice)
          }
          .scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).align(Alignment.Start)
          .width('100%').padding(12).borderRadius(12).backgroundColor($r('app.color.code'))
          .border({ width: 1, color: $r('app.color.border') })
        } else if (block.kind === 'rule') {
          Divider()
        } else if (block.kind === 'quote') {
          Row() {
            this.inline(block.text, 16, FontWeight.Normal)
          }
          .width('100%').padding({ left: 12 }).opacity(0.75)
          .border({ width: { left: 3 }, color: { left: $r('sys.color.comp_divider') } })
        } else if (block.kind === 'heading') {
          this.inline(block.text, block.level <= 1 ? 21 : (block.level === 2 ? 19 : 17), FontWeight.Bold)
        } else if (block.kind === 'item') {
          Row() { this.inline(block.text, 16, FontWeight.Normal) }.padding({ left: 8 + block.level * 16 })
        } else {
          this.inline(block.text, 16, FontWeight.Normal)
        }
      }, (block: MdBlock, index: number): string => `${index}-${block.kind}-${block.text.length}`)
    }
    .alignItems(HorizontalAlign.Start).width('100%')
  }

  @Builder
  inline(text: string, size: number, weight: FontWeight) {
    Text() {
      ForEach(parseInline(text), (span: MdSpan): void => {
        if (span.kind === 'code') {
          Span(span.text).fontFamily('monospace').fontSize(size - 2)
            .textBackgroundStyle({ color: $r('sys.color.comp_background_tertiary'), radius: 4 })
        } else if (span.kind === 'bold') {
          Span(span.text).fontWeight(FontWeight.Bold)
        } else if (span.kind === 'link') {
          Span(span.text).fontColor($r('app.color.link')).decoration({ type: TextDecorationType.Underline })
            .onClick((): void => { this.open(span.url); })
        } else {
          Span(span.text)
        }
      }, (span: MdSpan, index: number): string => `${index}-${span.kind}-${span.text}`)
    }
    .fontSize(size).fontWeight(weight).lineHeight(size * 1.45).copyOption(CopyOptions.LocalDevice).width('100%')
  }
}

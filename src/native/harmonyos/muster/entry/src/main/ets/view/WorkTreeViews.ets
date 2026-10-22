// The session's working tree: a listing, one file, the git log or a diff.
import { util } from '@kit.ArkTS';
import { MusterCore, ViewModel, uint64 } from '../core/MusterCore';
import { muster as pb } from '../proto/muster';
import { bytesText } from './Workspace';

@ComponentV2
export struct FilesView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;

  private parent(path: string): string {
    const slash = path.lastIndexOf('/');
    return slash < 0 ? '' : path.substring(0, slash);
  }

  build() {
    List() {
      if (this.vm.files?.loading) { ListItem() { LoadingProgress().width(28).height(28).margin(16) } }
      if ((this.vm.files?.path ?? '') !== '') {
        ListItem() { Text('..').padding(16) }.onClick((): void => { this.core.openFiles(this.parent(this.vm.files?.path ?? '')); })
      }
      ForEach(this.vm.files?.entries ?? [], (entry: pb.v1.IFileEntry): void => {
        ListItem() {
          Row({ space: 10 }) {
            Text(entry.dir ? '📁' : '📄')
            Text(entry.name ?? '').layoutWeight(1)
            if (!entry.dir) { Text(bytesText(uint64(entry.size))).fontSize(12).opacity(0.7) }
          }
          .width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
        }
        .onClick((): void => {
          const base = this.vm.files?.path ?? '';
          const path = base === '' ? (entry.name ?? '') : `${base}/${entry.name}`;
          if (entry.dir) this.core.openFiles(path); else this.core.openFile(path);
        })
      }, (entry: pb.v1.IFileEntry): string => entry.name ?? '')
    }
    .width('100%').height('100%')
  }
}

@ComponentV2
export struct FileView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;

  private file(): pb.app.IFileView { return this.vm.file ?? pb.app.FileView.create(); }

  /** The text, or null for binary content. */
  private text(): string | null {
    try {
      return new util.TextDecoder('utf-8', { fatal: true }).decodeToString(this.file().content ?? new Uint8Array(0));
    } catch (e) {
      return null;
    }
  }

  build() {
    Column() {
      Row({ space: 8 }) {
        Column({ space: 2 }) {
          Text(this.file().isDiff ? 'Recorded turn changes' : 'Current file contents').fontSize(12).opacity(0.7)
          if (this.file().truncated) { Text('Preview truncated').fontSize(12).fontColor('#FB8C00') }
        }
        .alignItems(HorizontalAlign.Start).layoutWeight(1)
        if (this.file().isDiff && (this.file().currentPath ?? '') !== '') {
          Button('Current file').buttonStyle(ButtonStyleMode.TEXTUAL).fontSize(14)
            .onClick((): void => { this.core.openFile(this.file().currentPath ?? ''); })
        }
      }
      .width('100%').padding({ left: 12, right: 4, top: 6, bottom: 6 }).backgroundColor($r('sys.color.comp_background_tertiary'))
      if (this.file().loading) {
        LoadingProgress().width(28).height(28).margin(16)
      } else if ((this.file().error ?? '') !== '') {
        Text(`Preview unavailable: ${this.file().error}`).fontSize(14).opacity(0.7).padding(16)
      } else if (this.text() === null) {
        Text(`Binary file: ${bytesText((this.file().content ?? new Uint8Array(0)).length)} of binary data`).fontSize(14).opacity(0.7).padding(16)
      } else {
        SourcePreview({ text: this.text() ?? '', diff: this.file().isDiff === true }).layoutWeight(1)
      }
    }
    .width('100%').height('100%')
  }
}

class SourceLine {
  index: number;
  text: string;
  constructor(index: number, text: string) { this.index = index; this.text = text; }
}

/** Source with line numbers, or a diff with added, removed and hunk lines coloured. */
@ComponentV2
export struct SourcePreview {
  @Param @Require text: string;
  @Param diff: boolean = false;

  private lines(): SourceLine[] {
    return this.text.split('\n').map((line: string, index: number): SourceLine => new SourceLine(index, line));
  }

  private color(line: string): ResourceColor {
    if (!this.diff) return $r('sys.color.font_primary');
    if (line.startsWith('+') && !line.startsWith('+++')) return '#2E7D32';
    if (line.startsWith('-') && !line.startsWith('---')) return '#C62828';
    if (line.startsWith('@@')) return '#1E88E5';
    return $r('sys.color.font_primary');
  }

  private fill(line: string): ResourceColor {
    if (!this.diff) return Color.Transparent;
    if (line.startsWith('+') && !line.startsWith('+++')) return '#142E7D32';
    if (line.startsWith('-') && !line.startsWith('---')) return '#14C62828';
    if (line.startsWith('@@')) return '#141E88E5';
    return Color.Transparent;
  }

  build() {
    Scroll() {
      List() {
        ForEach(this.lines(), (line: SourceLine): void => {
          ListItem() {
            Row({ space: 12 }) {
              if (!this.diff) {
                Text(`${line.index + 1}`).fontSize(12).fontFamily('monospace').opacity(0.4).textAlign(TextAlign.End)
                  .constraintSize({ minWidth: 32 })
              }
              Text(line.text === '' ? ' ' : line.text).fontSize(12).fontFamily('monospace').fontColor(this.color(line.text))
                .maxLines(1).copyOption(CopyOptions.LocalDevice)
            }
            .padding({ left: 12, right: 12, top: 1, bottom: 1 }).backgroundColor(this.fill(line.text))
            .constraintSize({ minWidth: '100%' })
          }
        }, (line: SourceLine): string => `${line.index}`)
      }
      .width('auto').padding({ top: 8, bottom: 8 })
    }
    .scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).width('100%').height('100%').align(Alignment.TopStart)
  }
}

@ComponentV2
export struct GitView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;

  build() {
    Column() {
      if (this.vm.git?.loading) {
        LoadingProgress().width(28).height(28).margin(16)
      } else if ((this.vm.git?.diff ?? '') !== '') {
        SourcePreview({ text: this.vm.git?.diff ?? '', diff: true })
      } else {
        List() {
          ForEach(this.vm.git?.commits ?? [], (commit: pb.v1.IGitCommit): void => {
            ListItem() {
              Column({ space: 2 }) {
                Text(commit.subject ?? '').fontWeight(FontWeight.Medium).maxLines(2)
                Text(`${(commit.hash ?? '').substring(0, 8)} · ${commit.author}`).fontSize(12).opacity(0.7)
              }
              .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
            }
            .onClick((): void => { this.core.openGit(commit.hash ?? ''); })
          }, (commit: pb.v1.IGitCommit): string => commit.hash ?? '')
        }
        .width('100%').height('100%')
      }
    }
    .width('100%').height('100%')
  }
}

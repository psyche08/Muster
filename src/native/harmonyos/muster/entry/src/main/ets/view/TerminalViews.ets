// Terminals: the list, and one session drawn on a canvas as a grid of
// styled cells. The shell draws rows and the cursor and forwards keys.
import { pasteboard } from '@kit.BasicServicesKit';
import { MusterTextInput } from '../input/KeyboardFields';
import { KeyboardTarget, MusterKeyboard, focusField, keyboardPrefs } from '../input/Keyboard';
import { WorkspaceLayout } from '../display/WorkspaceLayout';
import { MusterCore, ViewModel, TerminalKeys, StyleRun, palette256 } from '../core/MusterCore';
import { muster as pb } from '../proto/muster';
import { DeviceTabs, ErrorLine } from './Workspace';

const TERMINAL_INPUT = 'muster-terminal-input';

class KeyButton {
  label: string;
  key: string;
  constructor(label: string, key: string) { this.label = label; this.key = key; }
}

const keyBar: KeyButton[] = [
  new KeyButton('esc', TerminalKeys.ESCAPE), new KeyButton('tab', TerminalKeys.TAB),
  new KeyButton('^C', 'ctrl-c'), new KeyButton('^D', 'ctrl-d'), new KeyButton('^Z', 'ctrl-z'), new KeyButton('^L', 'ctrl-l'),
  new KeyButton('←', TerminalKeys.LEFT), new KeyButton('↓', TerminalKeys.DOWN), new KeyButton('↑', TerminalKeys.UP), new KeyButton('→', TerminalKeys.RIGHT),
  new KeyButton('home', TerminalKeys.HOME), new KeyButton('end', TerminalKeys.END),
  new KeyButton('pgup', TerminalKeys.PAGE_UP), new KeyButton('pgdn', TerminalKeys.PAGE_DOWN),
];

@ComponentV2
export struct TerminalsView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local ssh: boolean = false;
  @Local refreshing: boolean = false;

  build() {
    Column() {
      Row({ space: 8 }) {
        DeviceTabs({ vm: this.vm, selected: this.vm.terminals?.deviceId ?? '',
          select: (device: string): void => { this.core.openTerminals(device); },
          details: (device: string): void => { this.core.selectDevice(device); } })
          .layoutWeight(1)
        if (this.vm.terminals?.canCreate) {
          Button({ type: ButtonType.Circle }) {
            SymbolGlyph($r('sys.symbol.plus')).fontSize(18).fontColor([$r('sys.color.font_primary')])
          }
          .width(36).height(36).backgroundColor(Color.Transparent).accessibilityText('New window')
          .onClick((): void => { this.core.terminalCreate(); })
        }
        if (this.vm.terminals?.sshAvailable) {
          Button('SSH').buttonStyle(ButtonStyleMode.TEXTUAL).onClick((): void => { this.ssh = true; })
        }
      }
      .width('100%').padding({ right: 8 })
      Refresh({ refreshing: $$this.refreshing }) {
        List() {
          ForEach(this.vm.terminals?.errors ?? [], (error: string): void => {
            ListItem() { ErrorLine(error) }
          }, (error: string): string => `error-${error}`)
          if ((this.vm.terminals?.sessions ?? []).length === 0) {
            ListItem() {
              if (this.vm.terminals?.loading) {
                LoadingProgress().width(32).height(32).margin(24)
              } else {
                Text('Terminal sessions on the selected devices show up here.').padding(24).opacity(0.7)
              }
            }.width('100%')
          }
          ForEach(this.vm.terminals?.sessions ?? [], (session: pb.v1.ITerminalSession): void => {
            ListItem() {
              Column({ space: 2 }) {
                Text(session.title || (session.id ?? '')).fontSize(16).fontWeight(FontWeight.Medium).maxLines(1)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })
                Text(`${session.deviceName} · ${session.source} · ${session.cols}×${session.rows}`).fontSize(12).opacity(0.7)
              }
              .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
            }
            .backgroundColor(this.vm.screen === pb.app.Screen.TERMINAL && this.vm.terminal?.session === session.id &&
              this.vm.terminal?.deviceId === session.deviceId ? $r('app.color.accent_soft') : Color.Transparent)
            .onClick((): void => { this.core.openTerminal(session.id ?? '', session.deviceId ?? ''); })
          }, (session: pb.v1.ITerminalSession): string => `${session.deviceId}:${session.id}`)
        }
        .width('100%').height('100%').divider({ strokeWidth: 0.5, color: $r('sys.color.comp_divider'), startMargin: 16, endMargin: 16 })
      }
      .layoutWeight(1)
      .onRefreshing((): void => {
        this.core.openTerminals(this.vm.terminals?.deviceId ?? '');
        this.refreshing = false;
      })
    }
    .width('100%').height('100%')
    .bindSheet($$this.ssh, this.sshSheet(), { height: 300, title: { title: 'SSH' } })
  }

  @Builder
  sshSheet() {
    SshSheet({ core: this.core, close: (): void => { this.ssh = false; } })
  }
}

/** In-app SSH from this phone to the selected device, opened as a terminal. */
@ComponentV2
export struct SshSheet {
  @Param @Require core: MusterCore;
  @Param @Require close: () => void;
  @Local user: string = '';
  @Local port: string = '22';

  build() {
    Column({ space: 12 }) {
      MusterTextInput({ text: this.user, placeholder: 'User', onChange: (v: string): void => { this.user = v; } })
      MusterTextInput({ text: this.port, placeholder: 'Port', onChange: (v: string): void => { this.port = v; } })
      Text("Signs in with this phone's own key: put its public key line (Settings) in the target's authorized_keys.").fontSize(12).opacity(0.7)
      Button('Connect').width('100%').enabled(this.user.trim() !== '')
        .onClick((): void => { this.core.openSsh(Number(this.port) || 22, this.user.trim(), 80, 24); this.close(); })
    }
    .padding(16)
  }
}

@ComponentV2
export struct TerminalView {
  @Param embedded: boolean = false;
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Param layout: WorkspaceLayout = new WorkspaceLayout();
  @Local input: string = '';
  private settings: RenderingContextSettings = new RenderingContextSettings(true);
  private context: CanvasRenderingContext2D = new CanvasRenderingContext2D(this.settings);
  private palette: string[] = palette256();
  private cellWidth: number = 8;
  private cellHeight: number = 16;
  private readonly fontSize: number = 13;
  /** The font size drawn with: smaller for a fixed grid that would not fit. */
  private drawSize: number = 13;
  private canvasWidth: number = 0;
  private reportedCols: number = 0;
  private reportedRows: number = 0;

  private inputController: TextInputController = new TextInputController();

  @Monitor('vm')
  redraw(): void { this.draw(); }

  private terminalTarget(): KeyboardTarget {
    return {
      insert: (text: string): void => { this.core.terminalText(text); },
      backspace: (): void => { this.core.terminalKey(TerminalKeys.BACKSPACE); },
      enter: (): void => { this.core.terminalKey(TerminalKeys.ENTER); },
      hide: (): void => { this.inputController.stopEditing(); },
      key: (name: string): void => { this.core.terminalKey(name); },
    };
  }

  @Builder
  keyboard() {
    MusterKeyboard({ target: this.terminalTarget(), enterLabel: '回车', compact: this.layout.flex })
  }

  // Top and bottom: the keyboard is up from the start, below the crease.
  @Monitor('layout')
  stackedChanged(): void { if (this.layout.flex !== this.wasStacked) this.focusIfStacked(); }

  private wasStacked: boolean = false;

  private focusIfStacked(): void {
    this.wasStacked = this.layout.flex;
    if (!this.layout.flex || keyboardPrefs.terminalSystemKeyboard) return;
    focusField(this.getUIContext(), TERMINAL_INPUT);
  }

  aboutToAppear(): void { this.focusIfStacked(); }

  /** Sends the difference between the field's old and new text. */
  private typed(value: string): void {
    const previous = this.input;
    let common = 0;
    while (common < previous.length && common < value.length && previous[common] === value[common]) common++;
    for (let i = common; i < previous.length; i++) this.core.terminalKey(TerminalKeys.BACKSPACE);
    if (value.length > common) this.core.terminalText(value.substring(common));
    this.input = value;
  }

  /** Cell metrics at `size`; a fixed grid (someone's iTerm2 window) shrinks to fit its columns. */
  private measure(size: number = this.fontSize): void {
    this.context.font = `${size}vp monospace`;
    const ratio = this.context.measureText('M').width / size;
    const terminal = this.vm.terminal;
    const cols = terminal?.cols ?? 0;
    this.drawSize = terminal?.resizable === false && cols > 0 && this.canvasWidth > 0 ?
      Math.max(5, Math.min(size, this.canvasWidth / (cols * ratio))) : size;
    this.cellWidth = ratio * this.drawSize;
    this.cellHeight = Math.ceil(this.drawSize * 1.25);
  }

  private resize(width: number, height: number): void {
    this.canvasWidth = width;
    this.measure();
    // Only our own grids (PTY, SSH) take the viewport's size; a live window keeps its own.
    if (this.vm.terminal?.resizable === false) return;
    const cols = Math.max(20, Math.floor(width / this.cellWidth));
    const rows = Math.max(5, Math.floor(height / this.cellHeight));
    if (cols !== this.reportedCols || rows !== this.reportedRows) {
      this.reportedCols = cols;
      this.reportedRows = rows;
      this.core.terminalResize(cols, rows);
    }
  }

  private draw(): void {
    const ctx = this.context;
    const terminal = this.vm.terminal;
    if (terminal === null || terminal === undefined) return;
    this.measure();
    ctx.fillStyle = '#000000';
    ctx.fillRect(0, 0, ctx.width, ctx.height);
    ctx.font = `${this.drawSize}vp monospace`;
    ctx.textBaseline = 'top';
    const cols = Math.max(terminal.cols ?? 1, 1);
    for (const line of terminal.lines ?? []) {
      const top = (line.index ?? 0) * this.cellHeight;
      const text = line.text ?? '';
      const runs = StyleRun.decode(line.styles ?? new Uint8Array(0));
      if (runs.length === 0) {
        ctx.fillStyle = '#ffffff';
        ctx.fillText(text, 0, top);
        continue;
      }
      let col = 0;
      for (const run of runs) {
        if (col >= text.length) break;
        const end = Math.min(text.length, col + run.length, cols);
        let fg = StyleRun.css(run.fg, this.palette) ?? '#ffffff';
        let bg = StyleRun.css(run.bg, this.palette);
        if (run.inverse) { const swap = fg; fg = bg ?? '#000000'; bg = swap; }
        const x = col * this.cellWidth;
        if (bg !== null) {
          ctx.fillStyle = bg;
          ctx.fillRect(x, top, (end - col) * this.cellWidth, this.cellHeight);
        }
        ctx.globalAlpha = run.dim ? 0.6 : 1;
        ctx.fillStyle = fg;
        ctx.font = `${run.bold ? 'bold ' : ''}${run.italic ? 'italic ' : ''}${this.drawSize}vp monospace`;
        ctx.fillText(text.substring(col, end), x, top);
        if (run.underline) ctx.fillRect(x, top + this.cellHeight - 2, (end - col) * this.cellWidth, 1);
        ctx.globalAlpha = 1;
        col = end;
      }
    }
    if (!terminal.ended) {
      ctx.fillStyle = 'rgba(255,255,255,0.6)';
      ctx.fillRect((terminal.cursorX ?? 0) * this.cellWidth, (terminal.cursorY ?? 0) * this.cellHeight, this.cellWidth, this.cellHeight);
    }
  }

  build() {
    Column() {
      Canvas(this.context)
        .height(this.layout.flex ? this.layout.readingHeight : undefined)
        .flexShrink(0).layoutWeight(this.layout.flex ? 0 : 1).width('100%').backgroundColor('#000000')
        .onReady((): void => { this.draw(); })
        .onAreaChange((oldArea: Area, newArea: Area): void => {
          this.resize(Number(newArea.width) - 8, Number(newArea.height) - 8);
          this.draw();
        })
      Blank().height(this.layout.hingeHeight).flexShrink(0)
      Scroll() {
        Column() {
          Row({ space: 8 }) {
            // The system's paste control: one tap types the clipboard, with no
            // read prompt (a programmatic read asks the user every time). Rust
            // turns the text into keys while the session accepts them.
            if (this.vm.terminal?.canPaste === true) {
              PasteButton({ icon: PasteIconStyle.LINES, text: PasteDescription.PASTE, buttonType: ButtonType.Capsule })
                .fontSize(12).fontColor('#EEEEEE').iconSize(14).iconColor('#EEEEEE').backgroundColor('#33FFFFFF').height(28)
                .onClick((event: ClickEvent, result: PasteButtonOnClickResult): void => {
                  if (result !== PasteButtonOnClickResult.SUCCESS) return;
                  pasteboard.getSystemPasteboard().getData()
                    .then((data: pasteboard.PasteData): void => {
                      const text = data.getPrimaryText() ?? '';
                      if (text !== '') this.core.terminalPaste(text);
                    })
                    .catch((): void => {});
                })
            }
            Blank()
            if (!this.embedded) {
            Button('Close').buttonStyle(ButtonStyleMode.TEXTUAL).fontColor('#64B5F6')
              .onClick((): void => { this.core.closeTerminal(); })
            }
          }
          .width('100%').padding({ left: 8, right: 8 })
          // The in-app keyboard carries these keys itself; the bar stays for the system keyboard.
          if (keyboardPrefs.terminalSystemKeyboard) {
          Scroll() {
            Row({ space: 4 }) {
              ForEach(keyBar, (button: KeyButton): void => {
                // The terminal chrome is always dark: light text, whatever the system theme.
                Button(button.label).fontSize(12).fontFamily('monospace').backgroundColor('#33FFFFFF')
                  .fontColor('#EEEEEE').padding({ left: 10, right: 10 })
                  .onClick((): void => { this.core.terminalKey(button.key); this.input = ''; })
              }, (button: KeyButton): string => button.key)
            }
            .padding(4)
          }
          .scrollable(ScrollDirection.Horizontal).width('100%')
          }
          // The field holds the line typed since the last Enter: each change
          // sends what was added and a backspace per character removed;
          // Enter goes as a key and starts a new line.
          if (keyboardPrefs.terminalSystemKeyboard) {
            TextInput({ placeholder: 'type here', text: this.input, controller: this.inputController })
              .width('100%').enterKeyType(EnterKeyType.Send).enableKeyboardOnFocus(true)
              .backgroundColor('#26FFFFFF').fontColor('#FFFFFF').placeholderColor('#99FFFFFF').caretColor('#FFFFFF')
              .fontFamily('monospace').margin({ top: 4 })
              .onChange((value: string): void => { this.typed(value); })
              .onSubmit((): void => { this.core.terminalKey(TerminalKeys.ENTER); this.input = ''; })
          } else {
            // The in-app keyboard types straight into the terminal; the field only takes focus.
            TextInput({ placeholder: 'tap to type', text: '', controller: this.inputController })
              .width('100%').enableKeyboardOnFocus(true).id(TERMINAL_INPUT)
              .backgroundColor('#26FFFFFF').fontColor('#FFFFFF').placeholderColor('#99FFFFFF').caretColor('#FFFFFF')
              .fontFamily('monospace').margin({ top: 4 })
              .customKeyboard(this.keyboard(), { supportAvoidance: true })
          }
          if (this.vm.terminal?.ended || (this.vm.terminal?.status ?? '') !== '') {
            Text(this.vm.terminal?.ended ? `Ended · ${this.vm.terminal?.status}` : (this.vm.terminal?.status ?? ''))
              .fontSize(12).fontColor(this.vm.terminal?.ended ? '#FFB74D' : '#BBBBBB').width('100%').padding({ left: 8, top: 4, bottom: 4 })
          }
        }
        .width('100%').padding({ left: 8, right: 8 })
      }
      .width('100%').layoutWeight(this.layout.flex ? 1 : 0)
      .constraintSize({ maxHeight: this.layout.flex ? '100%' : '45%' }).align(Alignment.Top)
    }
    .width('100%').height('100%').backgroundColor('#000000')
  }
}

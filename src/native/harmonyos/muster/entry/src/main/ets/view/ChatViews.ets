// The conversation with one agent session or desktop chat. The core builds
// the timeline (messages, grouped tool activity, turn summaries) and owns
// every send and answer; this file draws them and collects the user's input.
// Options shown for an interaction are exactly what the provider offered
// (SECURITY I7).
import { MusterTextArea } from '../input/KeyboardFields';
import { common } from '@kit.AbilityKit';
import { fileIo, picker } from '@kit.CoreFileKit';
import { photoAccessHelper } from '@kit.MediaLibraryKit';
import { util } from '@kit.ArkTS';
import { WorkspaceLayout } from '../display/WorkspaceLayout';
import { MusterCore, ViewModel, uint64 } from '../core/MusterCore';
import { muster as pb } from '../proto/muster';
import { ErrorLine, bytesText, capitalized } from './Workspace';
import { TerminalView } from './TerminalViews';
import { ChatMarkdown } from './Markdown';

/** "Mon DD HH:MM:SS" in local time. */
function stamp(unix: number): string {
  const d = new Date(unix * 1000);
  const months: string[] = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const two = (n: number): string => n.toString().padStart(2, '0');
  return `${months[d.getMonth()]} ${d.getDate()} ${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
}

function grouped(n: number): string {
  return Math.round(n).toString().replace(new RegExp('\\B(?=(\\d{3})+(?!\\d))', 'g'), ',');
}

/** The subtitle under a conversation's title: provider and device. */
export function chatSubtitle(chat: pb.app.IChatView): string {
  return [chat.providerLabel ?? '', chat.session?.deviceName ?? ''].filter((s: string): boolean => s !== '').join(' · ');
}

@ComponentV2
export struct ChatView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Param layout: WorkspaceLayout = new WorkspaceLayout();
  @Local atLatest: boolean = true;
  @Local contentTab: string = 'Conversation';
  @Local activityTab: string = 'Tracing';
  @Local reviewingCode: boolean = false;
  private scroller: Scroller = new Scroller();
  private shownCount: number = 0;
  private shownSession: string = '';

  private chat(): pb.app.IChatView { return this.vm.chat ?? pb.app.ChatView.create(); }

  /** The cards under the transcript: the provider's questions and approvals, then queued messages. */
  private cards(): pb.v1.IPendingInteraction[] {
    return (this.chat().interactions ?? []).concat(this.chat().queue?.messages ?? []);
  }

  private sessionKey(): string {
    const session = this.chat().session ?? pb.v1.AgentSession.create();
    // A desktop chat keeps its draft when the core moves it to another Mac
    // (every Mac shows the same conversation).
    return session.mode === 'chat' ? `chat:${session.id}` : `${this.chat().deviceId}:${session.id}`;
  }

  // Follow new messages and cards while the reader is at the latest one.
  @Monitor('vm')
  followLatest(): void {
    this.reviewingCode = this.chat().codePreview != null;
    const key = this.sessionKey();
    const count = (this.chat().timeline ?? []).length + this.cards().length;
    if (key !== this.shownSession) {
      this.shownSession = key;
      this.contentTab = 'Conversation';
      this.atLatest = true;
    }
    if (count !== this.shownCount) {
      this.shownCount = count;
      if (this.atLatest) setTimeout((): void => { this.scroller.scrollEdge(Edge.Bottom); }, 0);
    }
  }

  aboutToAppear(): void {
    this.shownSession = this.sessionKey();
    this.shownCount = (this.chat().timeline ?? []).length + this.cards().length;
  }

  build() {
    Column() {
      if ((this.chat().session?.parentSessionId ?? '') !== '') {
        Button('Parent session').buttonStyle(ButtonStyleMode.TEXTUAL)
          .onClick((): void => { this.core.openAgent(this.chat().session?.parentSessionId ?? '', this.chat().deviceId ?? ''); })
      }
      Text(chatSubtitle(this.chat())).fontSize(12).opacity(0.65).padding({ top: 8, bottom: 8 }).width('100%').textAlign(TextAlign.Start)
      Row() {
          ForEach(['Conversation', 'Activity', 'Terminal'], (tab: string): void => {
            Button(tab === 'Conversation' ? 'Chat' : tab).buttonStyle(ButtonStyleMode.TEXTUAL).layoutWeight(1)
              .backgroundColor(this.contentTab === tab ? $r('sys.color.comp_background_secondary') : Color.Transparent).borderRadius(12)
              .fontWeight(this.contentTab === tab ? FontWeight.Bold : FontWeight.Normal)
              .enabled(tab !== 'Terminal' || this.chat().terminal?.available === true)
              .onClick((): void => {
                if (tab !== this.contentTab) {
                  if (tab === 'Terminal') this.core.sessionTerminal(pb.app.SessionTerminalAction.Action.START);
                  else if (this.contentTab === 'Terminal') this.core.sessionTerminal(pb.app.SessionTerminalAction.Action.DETACH);
                  this.contentTab = tab;
                }
              })
          }, (tab: string): string => tab)
      }.width('100%').padding(6).backgroundColor($r('sys.color.comp_background_tertiary')).borderRadius(16)
      if (this.contentTab === 'Conversation') {
      Stack({ alignContent: Alignment.Bottom }) {
        List({ space: 14, scroller: this.scroller }) {
          if (this.chat().loading && (this.chat().messageCount ?? 0) === 0) {
            ListItem() {
              Row({ space: 8 }) {
                LoadingProgress().width(22).height(22)
                Text('Loading conversation…').fontSize(14).opacity(0.7)
              }
            }
          }
          if ((this.chat().loadError ?? '') !== '') {
            ListItem() { ErrorLine(this.chat().loadError ?? '') }
          }
          if (!this.chat().loading && (this.chat().messageCount ?? 0) === 0 && (this.chat().loadError ?? '') === '') {
            ListItem() { Text(this.chat().session?.transcriptNotice || 'No messages yet').fontSize(14).opacity(0.7) }
          }
          if (this.chat().hasEarlier) {
            ListItem() {
              Button('Load earlier').buttonStyle(ButtonStyleMode.TEXTUAL)
                .onClick((): void => { this.atLatest = false; this.core.agentLoadEarlier(); })
            }.width('100%')
          }
          ForEach(this.chat().timeline ?? [], (block: pb.app.IChatBlock): void => {
            ListItem() { this.blockView(block) }
          }, (block: pb.app.IChatBlock): string => block.id ?? '')
          if (!this.layout.flex) {
            ForEach(this.cards(), (interaction: pb.v1.IPendingInteraction): void => {
              ListItem() { this.interactionCard(interaction) }
            }, (interaction: pb.v1.IPendingInteraction): string => `interaction-${interaction.id}`)
          }
        }
        .width('100%').height('100%').padding({ left: 16, right: 16, top: 12, bottom: 12 })
        .edgeEffect(EdgeEffect.Spring)
        .onAppear((): void => { this.scroller.scrollEdge(Edge.Bottom); })
        .onDidScroll((): void => { this.atLatest = this.scroller.isAtEnd(); })
        if (!this.atLatest) {
          Button({ type: ButtonType.Circle }) {
            SymbolGlyph($r('sys.symbol.arrow_up')).fontSize(18).rotate({ angle: 180 })
              .fontColor([$r('sys.color.font_primary')])
          }
          .width(38).height(38).margin({ bottom: 8 }).backgroundColor($r('sys.color.comp_background_tertiary'))
          .accessibilityText('Jump to latest')
          .onClick((): void => { this.atLatest = true; this.scroller.scrollEdge(Edge.Bottom); })
        }
      }
      .height(this.layout.flex ? this.layout.readingHeight : undefined)
      .flexShrink(0).layoutWeight(this.layout.flex ? 0 : 1).width('100%').clip(true)
      Blank().height(this.layout.hingeHeight).flexShrink(0)
      Scroll() {
        Column({ space: 8 }) {
          if ((this.chat().lockedUseStatus ?? '') !== '') {
            Row({ space: 8 }) {
              LoadingProgress().width(18).height(18)
              Text(this.chat().lockedUseStatus ?? '').fontSize(12).layoutWeight(1)
              if (this.chat().canCancelLockedUse) {
                Button('Cancel').buttonStyle(ButtonStyleMode.TEXTUAL).fontSize(13)
                  .onClick((): void => { this.core.cancelLockedUse(); })
              }
            }
            .width('100%').padding({ left: 8, right: 8 })
          }
          // One composer per conversation: a new key starts an empty draft.
          if (this.chat().session?.readOnly === true) {
            Text('Subagent · Read only').fontSize(12).opacity(0.6).padding(12)
          } else {
            ForEach([this.sessionKey()], (key: string): void => {
              // Top and bottom: the keyboard is up from the start, below the crease.
              ChatComposer({ core: this.core, chat: this.chat(), stacked: this.layout.flex })
            }, (key: string): string => key)
          }
          if (this.layout.flex) {
            ForEach(this.cards(), (interaction: pb.v1.IPendingInteraction): void => {
              this.interactionCard(interaction)
            }, (interaction: pb.v1.IPendingInteraction): string => `interaction-${interaction.id}`)
          }
        }
        .width('100%').padding({ left: 12, right: 12, top: 8, bottom: 8 })
      }
      .width('100%').layoutWeight(this.layout.flex ? 1 : 0)
      .constraintSize({ maxHeight: this.layout.flex ? '100%' : '45%' })
      .align(Alignment.Top).edgeEffect(EdgeEffect.Spring)
      } else if (this.contentTab === 'Terminal') {
        this.runHistory()
        Text(this.chat().terminal?.cwd ?? '').fontSize(12).padding(8)
        Row({ space: 12 }) {
          Button('Start').enabled(!this.chat().terminal?.busy && !this.chat().terminal?.attached && this.chat().terminal?.available === true)
            .onClick((): void => { this.core.sessionTerminal(pb.app.SessionTerminalAction.Action.START); })
          Button('End').enabled(this.chat().terminal?.canEnd === true)
            .onClick((): void => { this.core.sessionTerminal(pb.app.SessionTerminalAction.Action.END); })
          Button('Restart').enabled(this.chat().terminal?.canRestart === true)
            .onClick((): void => { this.core.sessionTerminal(pb.app.SessionTerminalAction.Action.RESTART); })
        }
        if (this.chat().terminal?.busy) { Text('Starting terminal…') }
        if (this.chat().terminal?.error) { Text(this.chat().terminal?.error ?? '').fontColor(Color.Red) }
        if (this.chat().terminal?.attached || this.vm.terminal?.ended) {
          TerminalView({ core: this.core, vm: this.vm, layout: this.layout, embedded: true })
        } else if (!this.chat().terminal?.busy) { Text('Terminal stopped') }
      } else {
        Row() {
          ForEach(['Tracing', 'Subagents', 'Sessions'], (tab: string): void => {
            Button(tab).buttonStyle(ButtonStyleMode.TEXTUAL).layoutWeight(1)
              .fontWeight(this.activityTab === tab ? FontWeight.Bold : FontWeight.Normal)
              .onClick((): void => { this.activityTab = tab; })
          })
        }.width('100%')
        List({ space: 12 }) {
          if (this.activityTab === 'Tracing') {
            ListItem() { Text('Execution trace · durations are approximate').fontSize(14) }
            if (this.chat().hasEarlier) {
              ListItem() { Button('Load earlier events').onClick((): void => { this.core.agentLoadEarlier(); }) }
            }
            if ((this.chat().trace ?? []).length === 0) { ListItem() { Text('No execution events yet') } }
            ForEach(this.chat().trace ?? [], (span: pb.app.ITraceSpan): void => {
              ListItem() { TraceEvent({ span: span }) }
            }, (span: pb.app.ITraceSpan): string => span.id ?? '')
          } else {
            if ((this.activityTab === 'Subagents' ? this.chat().session?.subagents ?? [] : this.chat().session?.subsessions ?? []).length === 0) {
              ListItem() { Text(`No ${this.activityTab.toLowerCase()}`) }
            }
            ForEach(this.activityTab === 'Subagents' ? this.chat().session?.subagents ?? [] : this.chat().session?.subsessions ?? [], (child: pb.v1.IAgentSession): void => {
              ListItem() {
                Button(`${child.title || child.id} · ${child.state} · ${child.relationship}`)
                  .buttonStyle(ButtonStyleMode.TEXTUAL)
                  .onClick((): void => { this.core.openAgent(child.id ?? '', this.chat().deviceId ?? ''); })
              }
            }, (child: pb.v1.IAgentSession): string => child.id ?? '')
          }
        }.width('100%').layoutWeight(1).padding(16)
      }
    }
    .width('100%').height('100%')
    .bindSheet($$this.reviewingCode, this.codeReview(), { height: '85%', title: { title: 'Review command' }, onDisappear: (): void => { this.core.codeAction(pb.app.CodeAction.Action.DISMISS); } })
  }

  @Builder
  codeReview() {
    Scroll() {
      Column({ space: 20 }) {
        Text(this.chat().codePreview?.deviceName ?? '').fontSize(18).fontWeight(FontWeight.Medium)
        Text(this.chat().codePreview?.cwd ?? '').fontSize(12).fontFamily('monospace').copyOption(CopyOptions.LocalDevice)
        Text(this.chat().codePreview?.code ?? '').fontSize(14).fontFamily('monospace').copyOption(CopyOptions.LocalDevice)
        Text('The exit code and output will be sent back to this conversation when the command finishes.').fontSize(14).opacity(0.7)
        Button('Run & return result').width('100%').onClick((): void => {
          this.core.codeAction(pb.app.CodeAction.Action.RUN, this.chat().codePreview?.id ?? '');
          this.contentTab = 'Terminal';
        })
        Button('Cancel').buttonStyle(ButtonStyleMode.TEXTUAL).onClick((): void => { this.core.codeAction(pb.app.CodeAction.Action.DISMISS); })
      }.width('100%').alignItems(HorizontalAlign.Start).padding(20)
    }.width('100%')
  }

  @Builder
  runHistory() {
    if (this.chat().codeError) { Text(this.chat().codeError ?? '').fontSize(12).fontColor(Color.Red).padding(8) }
    if ((this.chat().codeRuns ?? []).length > 0) {
      Scroll() {
        Row({ space: 12 }) {
          ForEach(this.chat().codeRuns ?? [], (run: pb.app.ICodeRunView): void => {
            CodeRunCard({ item: run, core: this.core })
          }, (run: pb.app.ICodeRunView): string => run.run?.id ?? '')
        }
      }.scrollable(ScrollDirection.Horizontal).width('100%').padding(12)
    }
  }

  @Builder
  blockView(block: pb.app.IChatBlock) {
    if (block.kind === 'working') {
      WorkingGroup({ block: block, core: this.core })
    } else if (block.kind === 'activity') {
      ActivityGroup({ block: block })
    } else if (block.kind === 'compact_context') {
      CompactContext({ message: block.message ?? pb.v1.AgentMessage.create(), core: this.core })
    } else if (block.kind === 'summary') {
      Column() {
        TurnChanges({ message: block.message ?? pb.v1.AgentMessage.create(),
          open: (path: string): void => { this.core.openChange(block.message?.id ?? '', path); } })
        TurnFooter({ message: block.message ?? pb.v1.AgentMessage.create() })
      }
    } else {
      MessageBubble({ message: block.message ?? pb.v1.AgentMessage.create(), parts: block.parts ?? [], core: this.core })
    }
  }

  @Builder
  interactionCard(interaction: pb.v1.IPendingInteraction) {
    InteractionCard({
      interaction: interaction,
      submitting: (this.chat().respondingInteraction ?? '') !== '',
      error: this.chat().responseError ?? '',
      outcomeUnknown: this.chat().responseUnknown === true,
      respond: (option: string, answers: pb.v1.IQuestionAnswer[]): void => {
        this.core.agentRespond(interaction.id ?? '', option, '', answers);
      },
    })
  }
}

class PickedAttachment {
  id: string = util.generateRandomUUID();
  uri: string = '';
  value: pb.v1.IAgentAttachment = pb.v1.AgentAttachment.create();
  image: boolean = false;
}

const mediaTypes: Map<string, string> = new Map<string, string>([
  ['jpg', 'image/jpeg'], ['jpeg', 'image/jpeg'], ['png', 'image/png'], ['gif', 'image/gif'], ['webp', 'image/webp'],
  ['heic', 'image/heic'], ['heif', 'image/heif'], ['bmp', 'image/bmp'], ['pdf', 'application/pdf'],
  ['txt', 'text/plain'], ['md', 'text/markdown'], ['json', 'application/json'], ['csv', 'text/csv'],
  ['log', 'text/plain'], ['yaml', 'application/yaml'], ['yml', 'application/yaml'], ['zip', 'application/zip'],
]);

function fileName(uri: string): string {
  const last = uri.split('/').pop() ?? 'file';
  try { return decodeURIComponent(last); } catch (e) { return last; }
}

function mediaType(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot < 0 ? 'application/octet-stream' : (mediaTypes.get(name.substring(dot + 1).toLowerCase()) ?? 'application/octet-stream');
}

/**
 * The draft and its attachments. Sending hands both to the core with a
 * draft id; the draft is cleared only when the core reports that id as sent,
 * so a failed send keeps what the user wrote.
 */
@ComponentV2
export struct ChatComposer {
  @Param @Require core: MusterCore;
  @Param @Require chat: pb.app.IChatView;
  /** Top-and-bottom layout: focus the field and use the compact keyboard. */
  @Param stacked: boolean = false;
  @Local text: string = '';
  /** Changes with every keystroke; the core clears a draft by its exact revision. */
  @Local draftRevision: string = '';
  @Local attachments: PickedAttachment[] = [];
  @Local importing: boolean = false;
  @Local error: string = '';
  @Local preview: string = '';
  @Local previewing: boolean = false;
  private submittedDraft: string = '';
  private submittedRevision: string = '';
  private registration: string = '';

  // The draft the core issued for this conversation, and the snapshot it
  // reads with every intent sent while this composer is on screen. The key
  // is the core's, never a path the shell made up.
  aboutToAppear(): void {
    this.text = this.chat.draft?.text ?? '';
    this.draftRevision = this.chat.draft?.revision || util.generateRandomUUID();
    const key = this.chat.draft?.key ?? '';
    this.registration = this.core.composerDraft.register((): pb.app.IChatDraft =>
      ({ key: key, text: this.text, revision: this.draftRevision }));
  }

  aboutToDisappear(): void {
    this.core.composerDraft.unregister(this.registration);
    this.registration = '';
  }

  @Monitor('chat')
  chatChanged(): void {
    // Older snapshots cannot overwrite newer keystrokes: only this exact
    // revision, which the core clears when a send finishes after re-entry.
    const draft = this.chat.draft;
    if (draft && (draft.revision ?? '') === this.draftRevision && (draft.text ?? '') !== this.text) {
      this.text = draft.text ?? '';
    }
    const sent = this.chat.sentDraftId ?? '';
    if (sent !== '' && sent === this.submittedDraft) {
      if (this.draftRevision === this.submittedRevision) {
        this.text = '';
        this.attachments = [];
        this.error = '';
      }
      this.submittedDraft = '';
    }
  }

  private typed(value: string): void {
    this.text = value;
    this.draftRevision = util.generateRandomUUID();
  }

  private busy(): boolean { return this.chat.sending === true || this.importing; }
  private empty(): boolean { return this.text.trim() === '' && this.attachments.length === 0; }
  /** Rust sets the limit to zero where attachments are not offered (desktop chats). */
  private acceptsAttachments(): boolean { return (this.chat.attachmentCountLimit ?? 0) > 0; }

  /** `releasing`: the unconfirmed send's token, once the user has checked the Mac. */
  private send(releasing: string = ''): void {
    if (this.empty() || this.busy()) return;
    this.submittedDraft = util.generateRandomUUID();
    this.submittedRevision = this.draftRevision;
    this.core.agentSend(this.text, this.attachments.map((a: PickedAttachment): pb.v1.IAgentAttachment => a.value),
      this.submittedDraft, releasing);
  }

  /** Reads a picked file within the core's advertised limits. */
  private add(uri: string, image: boolean): void {
    const countLimit = this.chat.attachmentCountLimit ?? 0;
    const bytesLimit = uint64(this.chat.attachmentBytesLimit);
    const used = this.attachments.reduce((sum: number, a: PickedAttachment): number => sum + (a.value.data?.length ?? 0), 0);
    const file = fileIo.openSync(uri, fileIo.OpenMode.READ_ONLY);
    try {
      const size = fileIo.statSync(file.fd).size;
      if (this.attachments.length >= countLimit || used + size > bytesLimit) {
        throw new Error(`Up to ${countLimit} files, ${bytesText(bytesLimit)} total.`);
      }
      const buffer = new ArrayBuffer(size);
      const read = fileIo.readSync(file.fd, buffer);
      const picked = new PickedAttachment();
      const name = fileName(uri);
      picked.uri = uri;
      picked.image = image || mediaType(name).startsWith('image/');
      picked.value = pb.v1.AgentAttachment.create({ name: name, mediaType: mediaType(name), data: new Uint8Array(buffer, 0, read) });
      this.attachments = this.attachments.concat([picked]);
      this.error = '';
    } finally {
      fileIo.closeSync(file);
    }
  }

  private async pickPhotos(): Promise<void> {
    const options = new photoAccessHelper.PhotoSelectOptions();
    options.MIMEType = photoAccessHelper.PhotoViewMIMETypes.IMAGE_TYPE;
    options.maxSelectNumber = Math.max(1, (this.chat.attachmentCountLimit ?? 1) - this.attachments.length);
    const result = await new photoAccessHelper.PhotoViewPicker().select(options);
    this.addAll(result.photoUris, true);
  }

  private async pickFiles(): Promise<void> {
    const options = new picker.DocumentSelectOptions();
    options.maxSelectNumber = Math.max(1, (this.chat.attachmentCountLimit ?? 1) - this.attachments.length);
    const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
    const uris = await new picker.DocumentViewPicker(context).select(options);
    this.addAll(uris, false);
  }

  private addAll(uris: string[], images: boolean): void {
    this.importing = true;
    try {
      for (const uri of uris) {
        try { this.add(uri, images); } catch (e) { this.error = (e as Error).message; }
      }
    } finally {
      this.importing = false;
    }
  }

  private attachMenu(): MenuElement[] {
    return [
      { value: 'Photo library', action: (): void => {
        this.pickPhotos().catch((e: Error): void => { this.error = e.message; });
      } },
      { value: 'Add files', action: (): void => {
        this.pickFiles().catch((e: Error): void => { this.error = e.message; });
      } },
    ];
  }

  build() {
    Column({ space: 6 }) {
      if (this.chat.unconfirmed) {
        this.unconfirmedCard(this.chat.unconfirmed ?? pb.app.UnconfirmedSend.create())
      }
      if (this.text === '' && (this.chat.composerSuggestion ?? '') !== '') {
        Button() {
          Row({ space: 12 }) {
            Text(this.chat.composerSuggestion ?? '').layoutWeight(1).textAlign(TextAlign.Start)
            Text('Use').fontWeight(FontWeight.Medium)
          }.width('100%').padding(12)
        }
        .width('100%').type(ButtonType.Normal).borderRadius(16).enabled(!this.busy())
        .backgroundColor($r('app.color.composer')).fontColor($r('sys.color.font_primary'))
        .onClick((): void => { this.typed(this.chat.composerSuggestion ?? ''); })
      }
      Column({ space: 8 }) {
        if (this.attachments.length > 0) {
          Scroll() {
            Row({ space: 8 }) {
              ForEach(this.attachments, (item: PickedAttachment): void => { this.attachment(item) },
                (item: PickedAttachment): string => item.id)
            }
            .padding(2)
          }
          .scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).width('100%').align(Alignment.Start)
        }
        MusterTextArea({ text: this.text, placeholder: this.chat.composerPlaceholder || 'Follow up', transparent: true, editable: !this.busy(),
          autoFocus: this.stacked, compactKeyboard: this.stacked,
          onChange: (v: string): void => { this.typed(v); } })
          .width('100%').padding({ left: 4, right: 4, top: 6, bottom: 6 })
        Row({ space: 8 }) {
          if (this.acceptsAttachments()) {
            Button({ type: ButtonType.Circle }) {
              SymbolGlyph($r('sys.symbol.plus')).fontSize(20).fontColor([$r('sys.color.font_primary')])
            }
            .width(44).height(44).backgroundColor(Color.Transparent).enabled(!this.busy())
            .accessibilityText('Add attachment')
            .bindMenu(this.attachMenu())
          }
          Blank().layoutWeight(1)
          if (this.busy()) { LoadingProgress().width(20).height(20) }
          if (this.chat.canInterrupt === true && this.empty() && this.chat.sending !== true) {
            Button({ type: ButtonType.Circle }) {
              SymbolGlyph($r('sys.symbol.stop_circle_fill')).fontSize(30).fontColor([$r('app.color.action')])
            }
            .width(44).height(44).backgroundColor(Color.Transparent).accessibilityText('Stop')
            .onClick((): void => { this.core.agentInterrupt(); })
          } else {
            Button({ type: ButtonType.Circle }) {
              SymbolGlyph($r('sys.symbol.arrow_up_circle_fill')).fontSize(32).fontColor([$r('sys.color.font_primary')])
            }
            .width(44).height(44).backgroundColor(Color.Transparent).accessibilityText('Send message')
            .enabled(!this.empty() && !this.busy()).opacity(!this.empty() && !this.busy() ? 1 : 0.35)
            .onClick((): void => { this.send(); })
          }
        }
        .width('100%').alignItems(VerticalAlign.Bottom)
      }
      .width('100%').padding(8).borderRadius(24)
      .backgroundColor($r('app.color.composer')).border({ width: 1, color: $r('app.color.border') })
      if (this.error !== '') {
        Text(this.error).fontSize(12).fontColor('#C62828').copyOption(CopyOptions.LocalDevice).padding({ left: 8 })
      }
    }
    .width('100%')
    .bindSheet($$this.previewing, this.previewSheet(), { height: '80%' })
  }

  @Builder
  attachment(item: PickedAttachment) {
    Row({ space: 8 }) {
      if (item.image) {
        Image(item.uri).width(56).height(56).objectFit(ImageFit.Cover).borderRadius(8)
          .onClick((): void => { this.preview = item.uri; this.previewing = true; })
      } else {
        SymbolGlyph($r('sys.symbol.doc')).fontSize(28).fontColor([$r('sys.color.font_secondary')]).width(40)
      }
      Column({ space: 2 }) {
        Text(item.value.name ?? '').fontSize(12).maxLines(1).textOverflow({ overflow: TextOverflow.Ellipsis })
          .constraintSize({ maxWidth: 140 })
        Text(bytesText(item.value.data?.length ?? 0)).fontSize(11).opacity(0.6)
      }
      .alignItems(HorizontalAlign.Start)
      Button({ type: ButtonType.Circle }) {
        SymbolGlyph($r('sys.symbol.xmark')).fontSize(14).fontColor([$r('sys.color.font_secondary')])
      }
      .width(28).height(28).backgroundColor(Color.Transparent).enabled(!this.busy())
      .accessibilityText(`Remove ${item.value.name}`)
      .onClick((): void => { this.attachments = this.attachments.filter((a: PickedAttachment): boolean => a.id !== item.id); })
    }
    .padding(6).borderRadius(12).backgroundColor($r('sys.color.comp_background_primary'))
  }

  @Builder
  previewSheet() {
    Image(this.preview).objectFit(ImageFit.Contain).width('100%').height('100%').padding(16)
  }

  /**
   * The Mac refused the last send: an earlier one has no receipt from the
   * desktop app. Once the user has looked at the conversation on the Mac
   * and it is not there, this draft goes again and releases it.
   */
  @Builder
  unconfirmedCard(refusal: pb.app.IUnconfirmedSend) {
    Column({ space: 6 }) {
      Text('上一条消息还没有在 Mac 上确认送达').fontSize(14).fontWeight(FontWeight.Medium)
      Text('请先在 Mac 上查看这个对话。那条消息确实没有出现，再继续发送。').fontSize(12)
      Text(refusal.message ?? '').fontSize(11).opacity(0.7).copyOption(CopyOptions.LocalDevice)
      Button('确认未送达，继续发送').fontSize(13).height(32).backgroundColor('#FB8C00').fontColor(Color.White)
        .enabled(!this.empty() && !this.busy())
        .onClick((): void => { this.send(refusal.token ?? ''); })
    }
    .alignItems(HorizontalAlign.Start).width('100%').padding(12).borderRadius(14).backgroundColor('#1FFB8C00')
  }
}

/** Draft selections are user input only; Shared Rust validates and submits them. */
@ComponentV2
export struct InteractionCard {
  @Param @Require interaction: pb.v1.IPendingInteraction;
  @Param submitting: boolean = false;
  @Param error: string = '';
  @Param outcomeUnknown: boolean = false;
  @Param @Require respond: (option: string, answers: pb.v1.IQuestionAnswer[]) => void;
  @Local expanded: boolean = false;
  @Local selections: Map<string, string[]> = new Map<string, string[]>();
  @Local custom: string[] = [];
  @Local answers: Map<string, string> = new Map<string, string>();
  private shown: string = '';

  aboutToAppear(): void { this.shown = this.interaction.id ?? ''; }

  @Monitor('interaction')
  reset(): void {
    if ((this.interaction.id ?? '') === this.shown) return;
    this.shown = this.interaction.id ?? '';
    this.selections = new Map<string, string[]>();
    this.custom = [];
    this.answers = new Map<string, string>();
  }

  private selected(question: string, option: string): boolean {
    return (this.selections.get(question) ?? []).indexOf(option) >= 0;
  }

  private toggle(question: pb.v1.IInteractionQuestion, option: string): void {
    const id = question.id ?? '';
    const current = this.selections.get(id) ?? [];
    const next = new Map<string, string[]>(this.selections);
    if (question.multiSelect) {
      next.set(id, current.indexOf(option) >= 0 ? current.filter((o: string): boolean => o !== option) : current.concat([option]));
    } else {
      next.set(id, [option]);
      this.custom = this.custom.filter((q: string): boolean => q !== id);
    }
    this.selections = next;
  }

  private toggleOther(question: pb.v1.IInteractionQuestion): void {
    const id = question.id ?? '';
    if (this.custom.indexOf(id) >= 0) {
      this.custom = this.custom.filter((q: string): boolean => q !== id);
    } else {
      this.custom = this.custom.concat([id]);
      if (!question.multiSelect) {
        const next = new Map<string, string[]>(this.selections);
        next.set(id, []);
        this.selections = next;
      }
    }
  }

  private writes(question: pb.v1.IInteractionQuestion): boolean {
    return this.custom.indexOf(question.id ?? '') >= 0 || (question.options ?? []).length === 0;
  }

  private submit(): void {
    const answers: pb.v1.IQuestionAnswer[] = (this.interaction.questions ?? []).map(
      (question: pb.v1.IInteractionQuestion): pb.v1.IQuestionAnswer => {
        const id = question.id ?? '';
        return {
          questionId: id,
          optionIds: (question.options ?? []).filter((o: pb.v1.IQuestionOption): boolean => this.selected(id, o.id ?? ''))
            .map((o: pb.v1.IQuestionOption): string => o.id ?? ''),
          text: this.writes(question) ? (this.answers.get(id) ?? '') : '',
        };
      });
    this.respond('', answers);
  }

  build() {
    if (this.interaction.optional) {
      Column({ space: 10 }) {
        Row({ space: 6 }) {
          Text('可选回答').fontSize(14).opacity(0.7)
          SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(12).rotate({ angle: this.expanded ? -90 : 180 })
            .fontColor([$r('sys.color.font_secondary')])
        }
        .onClick((): void => { this.expanded = !this.expanded; })
        if (this.expanded) { this.card() }
      }
      .alignItems(HorizontalAlign.Start).width('100%')
    } else {
      this.card()
    }
  }

  @Builder
  card() {
    Column({ space: 14 }) {
      Text(this.interaction.optional ? '可选回答' :
        (this.interaction.kind === 'question' ? 'Your input is needed' : 'Approval needed'))
        .fontSize(16).fontWeight(FontWeight.Medium)
      if ((this.interaction.questions ?? []).length === 0) {
        Text(this.interaction.prompt ?? '').copyOption(CopyOptions.LocalDevice)
        Column({ space: 8 }) {
          ForEach(this.interaction.options ?? [], (option: string): void => {
            Button(option).buttonStyle(ButtonStyleMode.NORMAL).onClick((): void => { this.respond(option, []); })
          }, (option: string): string => option)
        }
        .alignItems(HorizontalAlign.Start)
      } else {
        ForEach(this.interaction.questions ?? [], (question: pb.v1.IInteractionQuestion, index: number): void => {
          this.questionView(question, index)
        }, (question: pb.v1.IInteractionQuestion): string => question.id ?? '')
        Button() {
          Row({ space: 8 }) {
            if (this.submitting) { LoadingProgress().width(18).height(18).color(Color.White) }
            Text(this.submitting ? 'Submitting…' : 'Submit answers').fontColor(Color.White)
          }
        }
        .width('100%').height(44).onClick((): void => { this.submit(); })
      }
      if (this.error !== '') {
        Text(this.error).fontSize(13).fontColor('#C62828')
      }
      if (this.outcomeUnknown) {
        Text('Check the answer in the desktop app before trying again.').fontSize(12).opacity(0.7)
      }
    }
    .alignItems(HorizontalAlign.Start).width('100%').padding(16).borderRadius(18)
    .backgroundColor($r('sys.color.comp_background_tertiary'))
    .enabled(!this.submitting && !this.outcomeUnknown)
  }

  @Builder
  questionView(question: pb.v1.IInteractionQuestion, index: number) {
    Column({ space: 8 }) {
      if (index > 0) { Divider() }
      Row({ space: 6 }) {
        Text(`${index + 1} / ${(this.interaction.questions ?? []).length}`).fontSize(12).opacity(0.7)
        if ((question.header ?? '') !== '') { Text(question.header ?? '').fontSize(12).opacity(0.7) }
        Blank()
        Text(question.multiSelect ? 'Select all that apply' : 'Select one').fontSize(12).opacity(0.7)
      }
      .width('100%')
      Text(question.prompt ?? '').fontSize(15).fontWeight(FontWeight.Medium).copyOption(CopyOptions.LocalDevice)
      ForEach(question.options ?? [], (option: pb.v1.IQuestionOption): void => {
        this.choice(question.multiSelect === true, this.selected(question.id ?? '', option.id ?? ''),
          option.label ?? '', option.description ?? '', (): void => { this.toggle(question, option.id ?? ''); })
      }, (option: pb.v1.IQuestionOption): string => option.id ?? '')
      if (question.allowText) {
        if ((question.options ?? []).length > 0) {
          this.choice(question.multiSelect === true, this.custom.indexOf(question.id ?? '') >= 0,
            'Other', '输入自己的回答', (): void => { this.toggleOther(question); })
        }
        if (this.writes(question)) {
          MusterTextArea({ text: this.answers.get(question.id ?? '') ?? '', placeholder: 'Your answer', minHeight: 60,
            onChange: (v: string): void => {
              const next = new Map<string, string>(this.answers);
              next.set(question.id ?? '', v);
              this.answers = next;
            } })
            .accessibilityText(question.prompt ?? '')
        }
      }
    }
    .alignItems(HorizontalAlign.Start).width('100%')
  }

  @Builder
  choice(multi: boolean, selected: boolean, label: string, detail: string, toggle: () => void) {
    Row({ space: 10 }) {
      if (multi) {
        Checkbox().select(selected).hitTestBehavior(HitTestMode.None)
      } else {
        Radio({ value: label, group: `${this.interaction.id}-${label}` }).checked(selected).hitTestBehavior(HitTestMode.None)
      }
      Column({ space: 4 }) {
        Text(label).fontSize(15).fontWeight(selected ? FontWeight.Medium : FontWeight.Normal)
        if (detail !== '') { Text(detail).fontSize(13).opacity(0.7) }
      }
      .alignItems(HorizontalAlign.Start).layoutWeight(1)
    }
    .width('100%').padding(12).borderRadius(10)
    .border({ width: 1, color: selected ? $r('app.color.accent') : $r('sys.color.comp_divider') })
    .backgroundColor(selected ? $r('app.color.accent_faint') : $r('sys.color.comp_background_primary'))
    .accessibilitySelected(selected)
    .onClick((): void => { toggle(); })
  }
}

/** A turn's working phase: commentary and tool groups, folded once the turn completes. */
@ComponentV2
export struct WorkingGroup {
  @Param @Require block: pb.app.IChatBlock;
  @Param core: MusterCore | null = null;
  /** The reader's choice; until then the core's default_expanded applies. */
  @Local toggled: boolean | null = null;

  private expanded(): boolean { return this.toggled ?? (this.block.defaultExpanded === true); }

  build() {
    Column({ space: 12 }) {
      Row({ space: 8 }) {
        if (this.block.completed) {
          SymbolGlyph($r('sys.symbol.checkmark')).fontSize(14).fontColor([$r('sys.color.font_secondary')])
        } else {
          LoadingProgress().width(16).height(16)
        }
        Text(this.block.title ?? '').fontSize(14).opacity(0.7).maxLines(1).layoutWeight(1)
        if ((this.block.children?.length ?? 0) > 0) {
          SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(12).rotate({ angle: this.expanded() ? -90 : 180 })
            .fontColor([$r('sys.color.font_secondary')])
        }
      }
      .width('100%').padding({ top: 4, bottom: 4 })
      .accessibilityText((this.block.children?.length ?? 0) > 0
        ? `${this.block.title}, ${this.expanded() ? 'expanded' : 'collapsed'}` : (this.block.title ?? ''))
      .onClick((): void => {
        if ((this.block.children?.length ?? 0) > 0) { this.toggled = !this.expanded(); }
      })
      if (this.expanded()) {
        ForEach(this.block.children ?? [], (child: pb.app.IChatBlock): void => {
          if (child.kind === 'activity') {
            ActivityGroup({ block: child })
          } else {
            MessageBubble({ message: child.message ?? pb.v1.AgentMessage.create(), parts: child.parts ?? [], core: this.core })
          }
        }, (child: pb.app.IChatBlock): string => child.id ?? '')
      }
    }
    .alignItems(HorizontalAlign.Start).width('100%')
  }
}

@ComponentV2
export struct ActivityGroup {
  @Param @Require block: pb.app.IChatBlock;
  @Local expanded: boolean = false;

  build() {
    Column({ space: 8 }) {
      Row({ space: 8 }) {
        Text('⌘').fontSize(14).opacity(0.6).width(20)
        Text(this.block.title || 'Tool activity').fontSize(14).opacity(0.7).maxLines(1).layoutWeight(1)
        SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(12).rotate({ angle: this.expanded ? -90 : 180 })
          .fontColor([$r('sys.color.font_secondary')])
      }
      .width('100%').padding({ top: 4, bottom: 4 })
      .accessibilityText(`${this.block.title || 'Tool activity'}, ${this.expanded ? 'expanded' : 'collapsed'}`)
      .onClick((): void => { this.expanded = !this.expanded; })
      // Shared Rust gives every activity group its rows (calls paired with output).
      if (this.expanded) {
        ForEach(this.block.activityRows ?? [], (row: pb.app.IChatActivityRow): void => {
          ToolActivityRow({ row: row })
        }, (row: pb.app.IChatActivityRow): string => row.id ?? '')
      }
    }
    .alignItems(HorizontalAlign.Start).width('100%')
  }
}

@ComponentV2
export struct ToolActivityRow {
  @Param @Require row: pb.app.IChatActivityRow;
  @Local expanded: boolean = false;

  private symbol(): string {
    if (this.row.category === 'read') return '📖';
    if (this.row.category === 'search') return '🔍';
    return '›_';
  }

  build() {
    Column({ space: 8 }) {
      Row({ space: 8 }) {
        Text(this.symbol()).fontSize(12).opacity(0.6).width(20)
        Text(this.row.title ?? '').fontSize(14).opacity(0.7).maxLines(1).textOverflow({ overflow: TextOverflow.Ellipsis })
          .layoutWeight(1)
        SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(12).rotate({ angle: this.expanded ? -90 : 180 })
          .fontColor([$r('sys.color.font_secondary')])
      }
      .width('100%').padding({ top: 3, bottom: 3 })
      .onClick((): void => { this.expanded = !this.expanded; })
      if (this.expanded) {
        Column({ space: 10 }) {
          Text(this.row.category === 'shell' ? 'Shell' : (this.row.toolName ?? '')).fontSize(13).opacity(0.7)
          Scroll() {
            Text(this.row.detail ?? '').fontSize(12).fontFamily('monospace').copyOption(CopyOptions.LocalDevice).width('100%')
          }
          .constraintSize({ maxHeight: 280 }).align(Alignment.TopStart)
          if ((this.row.output ?? '') !== '') {
            Text('Output').fontSize(13).opacity(0.7)
            Scroll() {
              Text(this.row.output ?? '').fontSize(12).fontFamily('monospace').copyOption(CopyOptions.LocalDevice).width('100%')
            }
            .constraintSize({ maxHeight: 280 }).align(Alignment.TopStart)
          }
          Text(this.row.isResult ? 'Output received' : 'Arguments').fontSize(11).opacity(0.6).width('100%').textAlign(TextAlign.End)
        }
        .alignItems(HorizontalAlign.Start).width('100%').padding(14).borderRadius(14)
        .backgroundColor($r('sys.color.comp_background_tertiary'))
      }
    }
    .alignItems(HorizontalAlign.Start).width('100%')
  }
}

@ComponentV2
export struct TurnChanges {
  @Param @Require message: pb.v1.IAgentMessage;
  @Param @Require open: (path: string) => void;
  @Local expanded: boolean = true;

  private changes(): pb.v1.ITurnFileChange[] { return this.message.usage?.changes ?? []; }

  build() {
    if (this.changes().length > 0 || this.message.usage?.changesIncomplete) {
      Column({ space: 4 }) {
        Row({ space: 8 }) {
          Text(`${this.changes().length} changed files`).fontSize(14).fontWeight(FontWeight.Medium).layoutWeight(1)
          SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(12).rotate({ angle: this.expanded ? -90 : 180 })
            .fontColor([$r('sys.color.font_secondary')])
        }
        .width('100%')
        .onClick((): void => { this.expanded = !this.expanded; })
        if (this.expanded) {
          if (this.message.usage?.changesIncomplete) {
            Text('Some recorded changes could not be displayed').fontSize(12).opacity(0.7)
          }
          ForEach(this.changes(), (change: pb.v1.ITurnFileChange): void => {
            Row({ space: 8 }) {
              Column({ space: 2 }) {
                Text(change.openPath || (change.path ?? '')).fontSize(12).fontFamily('monospace').maxLines(2)
                  .textOverflow({ overflow: TextOverflow.Ellipsis })
                if ((change.previousPath ?? '') !== '') {
                  Text(`From ${change.previousPath}`).fontSize(10).opacity(0.6)
                }
              }
              .alignItems(HorizontalAlign.Start).layoutWeight(1)
              if (change.countsKnown) {
                Text(`+${change.additions}`).fontSize(12).fontFamily('monospace').fontColor('#43A047')
                Text(`−${change.deletions}`).fontSize(12).fontFamily('monospace').fontColor('#C62828')
              } else {
                Text(capitalized(change.kind ?? '')).fontSize(12).opacity(0.7)
              }
            }
            .width('100%').padding({ top: 10, bottom: 10 })
            .onClick((): void => { this.open(change.path ?? ''); })
          }, (change: pb.v1.ITurnFileChange): string => change.path ?? '')
        }
      }
      .alignItems(HorizontalAlign.Start).width('100%').padding(12).margin({ top: 10 }).borderRadius(12)
      .backgroundColor('#0F808080').border({ width: 1, color: '#26808080' })
    }
  }
}

@ComponentV2
export struct TurnFooter {
  @Param @Require message: pb.v1.IAgentMessage;
  @Local details: boolean = false;

  private usage(): pb.v1.ITurnUsage { return this.message.usage ?? pb.v1.TurnUsage.create(); }
  private expiry(): pb.v1.ICacheExpiry | null { return this.usage().cacheExpiry ?? null; }

  /** Input, output and cache hit for the whole turn. */
  private tokens(): string {
    const usage = this.usage();
    const counts = `input ${grouped(uint64(usage.inputTokens))} · Output ${grouped(uint64(usage.outputTokens))}`;
    if (usage.tokensKnown) {
      return `Turn ${counts} · Cache hit ` +
        (usage.cacheHitKnown ? `${(usage.cacheHitPercent ?? 0).toFixed(1)}%` : 'unavailable');
    }
    if (uint64(usage.requestCount) > 0) return `Partial turn ${counts} · Cache hit unavailable`;
    return 'Token usage unavailable';
  }

  /** The core's TTL estimate and, to the minute, when it lapses. */
  private cache(): string {
    const expiry = this.expiry();
    if (expiry === null) return 'Cache TTL unavailable';
    if (uint64(expiry.fromUnix) <= 0) return expiry.label ?? '';
    return `${expiry.label ?? ''} · until ${this.ends(false)}`;
  }

  /** The estimated expiry (or range): the time alone on the turn's day, with the date otherwise. */
  private ends(seconds: boolean): string {
    const expiry = this.expiry() ?? pb.v1.CacheExpiry.create();
    const turnDay = (text: string): string => text.substring(0, text.lastIndexOf(' '));
    const clock = (unix: number): string => {
      const full = stamp(unix);
      const time = full.substring(full.lastIndexOf(' ') + 1);
      const shown = seconds ? time : time.substring(0, time.lastIndexOf(':'));
      return turnDay(full) === turnDay(stamp(uint64(this.message.tsUnix))) ? shown : `${turnDay(full)} ${shown}`;
    };
    const from = uint64(expiry.fromUnix);
    const until = uint64(expiry.untilUnix);
    return until > from ? `${clock(from)}–${clock(until)}` : clock(from);
  }

  private detailLines(): string[] {
    const usage = this.usage();
    const lines: string[] = [`${uint64(usage.requestCount)} requests · ` + (usage.tokensKnown
      ? `Read ${grouped(uint64(usage.cacheReadTokens))} · Write ${grouped(uint64(usage.cacheWriteTokens))} tokens`
      : 'Token breakdown unavailable')];
    if (uint64(this.expiry()?.fromUnix) > 0) lines.push(`Estimated expiry ${this.ends(true)}`);
    if (usage.firstRequestCacheHitKnown) lines.push(`First request hit ${(usage.firstRequestCacheHitPercent ?? 0).toFixed(1)}%`);
    const gap = uint64(this.expiry()?.observedGapSeconds);
    if (gap > 0) lines.push(`Observed gap before first request: ${grouped(gap)} seconds. Hits and misses do not prove a TTL.`);
    if (this.expiry() === null) {
      lines.push('No expiry data received');
    } else if ((this.expiry()?.detail ?? '') !== '') {
      lines.push(this.expiry()?.detail ?? '');
    }
    lines.push("Turn hit rate = cached reads ÷ all input across this turn's requests. Input includes cache reads and writes.");
    return lines;
  }

  private cost(): string {
    const usage = this.usage();
    if (!usage.costKnown) return 'Cost unavailable';
    return `${usage.costSource === 'recorded' ? 'Recorded' : 'Estimated'} $${(usage.costUsd ?? 0).toFixed(5)} USD`;
  }

  build() {
    Column({ space: 4 }) {
      Divider()
      Row() {
        Text(uint64(this.message.tsUnix) > 0 ? stamp(uint64(this.message.tsUnix)) : 'End time unavailable').fontSize(11)
        Blank()
        Text(this.usage().model ?? '').fontSize(11).maxLines(1)
      }
      .width('100%')
      Text(this.tokens()).fontSize(11).maxLines(1).minFontSize(9).maxFontSize(11)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
      // One line for the cache; tapping it shows the details.
      Text(`${this.details ? '▾' : '▸'} ${this.cache()}`).fontSize(11).maxLines(1).minFontSize(9).maxFontSize(11)
        .textOverflow({ overflow: TextOverflow.Ellipsis }).width('100%')
        .accessibilityDescription(this.details ? 'Hides cache details' : 'Shows cache details')
        .onClick((): void => { this.details = !this.details; })
      if (this.details) {
        ForEach(this.detailLines(), (line: string): void => { Text(line).fontSize(11).padding({ left: 12 }) },
          (line: string, index: number): string => `${index}-${line}`)
      }
      Text(this.cost()).fontSize(11)
    }
    .alignItems(HorizontalAlign.Start).width('100%').padding({ top: 8, bottom: 8 }).opacity(0.6)
  }
}

@ComponentV2
export struct CompactContext {
  @Param @Require message: pb.v1.IAgentMessage;
  @Param core: MusterCore | null = null;
  @Local expanded: boolean = false;

  build() {
    Column({ space: 8 }) {
      Row({ space: 6 }) {
        Text('Compact context').fontSize(14).opacity(0.7)
        SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(12).rotate({ angle: this.expanded ? -90 : 180 })
          .fontColor([$r('sys.color.font_secondary')])
      }
      .onClick((): void => { this.expanded = !this.expanded; })
      if (this.expanded) { ChatMarkdown({ text: this.message.text ?? '', core: this.core }) }
    }
    .alignItems(HorizontalAlign.Start).width('100%').padding({ top: 6, bottom: 6 })
  }
}

@ComponentV2
export struct MessageBubble {
  @Param parts: pb.app.IMessagePart[] = [];
  @Param @Require message: pb.v1.IAgentMessage;
  @Param core: MusterCore | null = null;

  private isUser(): boolean { return this.message.role === 'user'; }

  build() {
    Row() {
      if (this.isUser()) { Blank().constraintSize({ minWidth: 64 }) }
      Column({ space: 4 }) {
        if (this.parts.some((p: pb.app.IMessagePart): boolean => p.kind === 'code')) {
          ForEach(this.parts, (part: pb.app.IMessagePart): void => {
            if (part.kind === 'code') {
              Column({ space: 10 }) {
                Row() {
                  Text((part.language ?? 'CODE').toUpperCase()).fontSize(12).layoutWeight(1)
                  if (part.commandId) {
                    Button('Run').buttonStyle(ButtonStyleMode.TEXTUAL)
                      .onClick((): void => { this.core?.codeAction(pb.app.CodeAction.Action.PREVIEW, part.commandId ?? ''); })
                  }
                }.width('100%')
                Scroll() { Text(part.text ?? '').fontSize(13).fontFamily('monospace').copyOption(CopyOptions.LocalDevice) }
                  .scrollable(ScrollDirection.Horizontal).width('100%')
              }.padding(14).borderRadius(14).backgroundColor($r('sys.color.comp_background_tertiary')).width('100%')
            } else { ChatMarkdown({ text: part.text ?? '', core: this.core }) }
          })
        } else if (this.isUser()) {
          Text(this.message.text ?? '').fontSize(16).lineHeight(23).fontColor($r('app.color.user_text'))
            .copyOption(CopyOptions.LocalDevice)
        } else {
          ChatMarkdown({ text: this.message.text ?? '', core: this.core })
        }
      }
      .alignItems(HorizontalAlign.Start)
      .padding(this.isUser() ? { left: 16, right: 16, top: 10, bottom: 10 } : 0)
      .backgroundColor(this.isUser() ? $r('app.color.user_surface') : Color.Transparent)
      .borderRadius(this.isUser() ? 20 : 0)
      .layoutWeight(this.isUser() ? 0 : 1)
    }
    .width('100%').alignItems(VerticalAlign.Top)
    .padding({ top: 6, bottom: 6 })
  }
}


@ComponentV2
struct TraceEvent {
  @Param @Require span: pb.app.ITraceSpan;
  @Local expanded: boolean = false;
  build() {
    Column({ space: 6 }) {
      Button(`${this.span.title} · ${this.span.status}${this.span.durationKnown ? ` · ≈ ${uint64(this.span.durationSeconds)} s` : ''}`)
        .buttonStyle(ButtonStyleMode.TEXTUAL)
        .onClick((): void => { this.expanded = !this.expanded; })
      if (uint64(this.span.startedUnix) > 0) { Text(stamp(uint64(this.span.startedUnix))).fontSize(12).opacity(0.6) }
      if (this.expanded) { Text(this.span.detail ?? '').fontSize(13) }
    }.width('100%').alignItems(HorizontalAlign.Start)
  }
}

@ComponentV2
struct CodeRunCard {
  @Param @Require item: pb.app.ICodeRunView;
  @Param @Require core: MusterCore;
  @Local expanded: boolean = false;
  build() {
    Column({ space: 8 }) {
      Button(`${this.item.run?.state ?? ''} · ${this.item.delivery ?? ''}`)
        .buttonStyle(ButtonStyleMode.TEXTUAL).onClick((): void => { this.expanded = !this.expanded; })
      Text(this.item.run?.code ?? '').fontFamily('monospace').fontSize(12).maxLines(this.expanded ? 20 : 2)
      if (this.expanded) {
        if (this.item.run?.exitKnown) { Text(`Exit code ${this.item.run?.exitCode}`).fontSize(12) }
        if (this.item.run?.truncated) { Text('Output truncated').fontSize(12) }
        Scroll() { Text(this.item.run?.output ?? '').fontSize(12).fontFamily('monospace').copyOption(CopyOptions.LocalDevice) }.constraintSize({ maxHeight: 180 })
        if (this.item.error) { Text(this.item.error ?? '').fontSize(12).fontColor(Color.Red) }
        if (this.item.canReturn) { Button('Return result to conversation').onClick((): void => { this.core.codeAction(pb.app.CodeAction.Action.RETURN, this.item.run?.id ?? ''); }) }
      }
    }.width(260).padding(12).borderRadius(14).backgroundColor($r('sys.color.comp_background_tertiary')).alignItems(HorizontalAlign.Start)
  }
}

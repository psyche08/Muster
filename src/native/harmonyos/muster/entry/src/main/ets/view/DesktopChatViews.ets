// Desktop Chat mode: the Claude app's and the ChatGPT app's ordinary
// conversations on one Mac. Shared Rust reads and drives them through
// Accessibility; this screen shows what the core lists and forwards taps.
import { MusterCore, ViewModel } from '../core/MusterCore';
import { muster as pb } from '../proto/muster';
import { ErrorLine, SectionHeader } from './Workspace';

/**
 * The Chat tab: one app's conversations. There is no device list: every Mac
 * signed in to the account shows the same chats, so the core picks one that
 * can show them now and names it in the footer.
 */
@ComponentV2
export struct DesktopChatsView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local refreshing: boolean = false;

  private view(): pb.app.IDesktopChatsView { return this.vm.desktopChats ?? pb.app.DesktopChatsView.create(); }
  private provider(): string { return this.view().provider ?? ''; }
  /** The desktop apps with a Chat mode, in the core's order and as it names them. */
  private choices(): pb.app.IChatProvider[] { return this.view().choices ?? []; }
  private appName(): string {
    return this.choices().find((choice: pb.app.IChatProvider): boolean => choice.id === this.provider())?.label ?? '';
  }
  private busy(): boolean { return (this.view().opening ?? '') !== ''; }
  private picked(): boolean { return (this.view().deviceId ?? '') !== ''; }
  private catalog(): pb.v1.IAgentChatCatalog | null { return this.view().catalog ?? null; }

  private footer(): string {
    if (this.picked()) {
      return `Through ${this.view().deviceName}. Every Mac shows the same ${this.appName()} chats; another one is used when this one is busy.`;
    }
    return this.view().loading ? `Finding a Mac that can show ${this.appName()} chats…` : '';
  }

  build() {
    Refresh({ refreshing: $$this.refreshing }) {
      List() {
        ListItem() {
          Column({ space: 10 }) {
            Row() {
              ForEach(this.choices(), (choice: pb.app.IChatProvider): void => {
                Text(choice.label ?? '')
                  .fontSize(14).fontWeight(FontWeight.Medium).textAlign(TextAlign.Center).layoutWeight(1)
                  .padding({ top: 7, bottom: 7 }).borderRadius(14)
                  .fontColor(this.provider() === choice.id ? $r('sys.color.font_primary') : $r('sys.color.font_secondary'))
                  .backgroundColor(this.provider() === choice.id ? $r('sys.color.comp_background_primary') : Color.Transparent)
                  .accessibilitySelected(this.provider() === choice.id)
                  .onClick((): void => { if (this.provider() !== choice.id) this.core.openDesktopChats(choice.id ?? ''); })
              }, (choice: pb.app.IChatProvider): string => choice.id ?? '')
            }
            .width('100%').padding(2).borderRadius(16).backgroundColor($r('sys.color.comp_background_tertiary'))
            Button() {
              Row({ space: 6 }) {
                SymbolGlyph($r('sys.symbol.square_and_pencil')).fontSize(16).fontColor([$r('sys.color.font_on_primary')])
                Text('New chat').fontColor($r('sys.color.font_on_primary')).fontSize(15)
              }
            }
            .width('100%').height(40).enabled(!this.busy() && this.picked())
            .onClick((): void => { this.core.desktopChatOpen(''); })
            if (this.footer() !== '') {
              Text(this.footer()).fontSize(12).opacity(0.7)
            }
          }
          .alignItems(HorizontalAlign.Start).width('100%').padding(16)
        }
        if ((this.view().error ?? '') !== '') {
          ListItem() { ErrorLine(this.view().error ?? '') }
        }
        if (this.catalog() !== null) {
          if (this.view().loading) {
            ListItem() {
              Row({ space: 8 }) {
                LoadingProgress().width(20).height(20)
                Text('Loading all conversations…').fontSize(13).opacity(0.7)
              }
              .padding({ left: 16, top: 4, bottom: 4 })
            }
          }
          ListItem() { SectionHeader('Conversations') }
          if (this.catalog()?.chatMode !== true) {
            ListItem() {
              Column({ space: 8 }) {
                Text(`${this.appName()} is not in its Chat mode on ${this.view().deviceName}.`).fontSize(14).opacity(0.7)
                Button(this.view().opening === 'new' ? 'Switching…' : 'Switch to Chat mode')
                  .buttonStyle(ButtonStyleMode.NORMAL).enabled(!this.busy() && this.picked())
                  .onClick((): void => { this.core.desktopChatOpen('', true); })
              }
              .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 4, bottom: 8 })
            }
          } else if ((this.catalog()?.chats ?? []).length === 0) {
            ListItem() { Text('No conversations').fontSize(14).opacity(0.7).padding(16) }
          }
          ForEach(this.catalog()?.chats ?? [], (chat: pb.v1.IAgentChat): void => {
            ListItem() {
              Row({ space: 8 }) {
                Text(chat.title ?? '').fontSize(15).maxLines(2).textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1)
                if (this.view().opening === chat.id) { LoadingProgress().width(18).height(18) }
              }
              .width('100%').padding({ left: 16, right: 16, top: 12, bottom: 12 })
              .opacity(!this.busy() && this.picked() ? 1 : 0.5)
              .backgroundColor(this.vm.screen === pb.app.Screen.CHAT && this.vm.chat?.session?.id === chat.id ?
                $r('app.color.accent_soft') : Color.Transparent)
            }
            .onClick((): void => { if (!this.busy() && this.picked()) this.core.desktopChatOpen(chat.id ?? ''); })
          }, (chat: pb.v1.IAgentChat): string => chat.id ?? '')
        } else if (this.view().loading) {
          ListItem() { LoadingProgress().width(32).height(32).margin(24) }.width('100%')
        }
      }
      .width('100%').height('100%')
      .divider({ strokeWidth: 0.5, color: $r('sys.color.comp_divider'), startMargin: 16, endMargin: 16 })
    }
    .onRefreshing((): void => { this.core.refresh(); this.refreshing = false; })
  }
}

/**
 * A desktop chat's model and effort, as the app on the Mac offers them.
 * Opening reads them there (the Mac shows its picker briefly); a tap chooses
 * by name. What cannot be set through Accessibility is shown with the app's
 * reason.
 */
@ComponentV2
export struct ChatOptionsSheet {
  @Param @Require core: MusterCore;
  @Param @Require chat: pb.app.IChatView;

  aboutToAppear(): void { this.core.chatOptionsLoad(); }

  private options(): pb.v1.IAgentChatOptions { return this.chat.chatOptions ?? pb.v1.AgentChatOptions.create(); }

  build() {
    Column() {
      Row() {
        Blank()
        if (this.chat.chatOptionsBusy) {
          LoadingProgress().width(22).height(22)
        } else {
          Button({ type: ButtonType.Circle }) {
            SymbolGlyph($r('sys.symbol.arrow_clockwise')).fontSize(18).fontColor([$r('sys.color.font_primary')])
          }
          .width(36).height(36).backgroundColor(Color.Transparent).accessibilityText('Refresh')
          .onClick((): void => { this.core.chatOptionsLoad(); })
        }
      }
      .width('100%').padding({ left: 16, right: 16 })
      List() {
        if ((this.chat.chatOptionsError ?? '') !== '') {
          ListItem() { ErrorLine(this.chat.chatOptionsError ?? '') }
        }
        ListItem() { SectionHeader('Model') }
        if ((this.options().models ?? []).length === 0 && this.chat.chatOptionsBusy) {
          ListItem() { LoadingProgress().width(24).height(24).margin(12) }
        }
        ForEach(this.options().models ?? [], (model: pb.v1.IAgentChatOption): void => {
          ListItem() { this.option(model, (): void => { this.core.chatSetOptions(model.name ?? '', ''); }) }
        }, (model: pb.v1.IAgentChatOption): string => `model-${model.name}`)
        ListItem() { SectionHeader('Reasoning') }
        if ((this.options().efforts ?? []).length === 0) {
          ListItem() {
            Row() {
              Text('Effort').layoutWeight(1)
              Text(this.options().effort || '—').opacity(0.7)
            }
            .width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
          }
        }
        ForEach(this.options().efforts ?? [], (effort: pb.v1.IAgentChatOption): void => {
          ListItem() { this.option(effort, (): void => { this.core.chatSetOptions('', effort.name ?? ''); }) }
        }, (effort: pb.v1.IAgentChatOption): string => `effort-${effort.name}`)
        if ((this.options().note ?? '') !== '') {
          ListItem() { Text(this.options().note ?? '').fontSize(12).opacity(0.7).padding({ left: 16, right: 16, top: 4 }) }
        }
      }
      .layoutWeight(1).width('100%')
    }
    .width('100%').height('100%')
  }

  @Builder
  option(option: pb.v1.IAgentChatOption, choose: () => void) {
    Row() {
      Column({ space: 2 }) {
        Text(option.name ?? '').fontSize(15)
        if ((option.detail ?? '') !== '') { Text(option.detail ?? '').fontSize(12).opacity(0.7) }
      }
      .alignItems(HorizontalAlign.Start).layoutWeight(1)
      if (option.selected) {
        SymbolGlyph($r('sys.symbol.checkmark')).fontSize(18).fontColor([$r('app.color.accent')])
      }
    }
    .width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
    .opacity(this.chat.chatOptionsBusy && option.selected !== true ? 0.5 : 1)
    .onClick((): void => { if (!this.chat.chatOptionsBusy && option.selected !== true) choose(); })
  }
}

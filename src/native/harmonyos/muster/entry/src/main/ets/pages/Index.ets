// One component per screen the core names; the core owns the stack, so
// back is an intent, not a router call.
import { FlexPreference, FoldGeometry, WorkspaceArea, WorkspaceLayout, workspaceLayout } from '../display/WorkspaceLayout';
import { keyboardPrefs } from '../input/Keyboard';
import { MODEL_KEY, MusterModel } from '../core/MusterModel';
import { AppStorageV2 } from '@kit.ArkUI';
import { muster as pb } from '../proto/muster';
import { OnboardingView, HomeView, InviteView, JoinView, DeviceView, SettingsView } from '../view/NetworkViews';
import { ChatView, chatSubtitle } from '../view/ChatViews';
import { ChatOptionsSheet, DesktopChatsView } from '../view/DesktopChatViews';
import { WorkspaceSwitcher } from '../view/Workspace';
import { TerminalsView, TerminalView } from '../view/TerminalViews';
import { FilesView, FileView, GitView } from '../view/WorkTreeViews';
import { BrowserView } from '../view/BrowserViews';

@Entry
@ComponentV2
struct Index {
  @Local model: MusterModel = AppStorageV2.connect<MusterModel>(MusterModel, MODEL_KEY, (): MusterModel => new MusterModel())!;

  @Local workspace: WorkspaceArea = new WorkspaceArea();
  @Local chatOptions: boolean = false;
  @Local subagents: boolean = false;

  private layout(): WorkspaceLayout {
    // The in-app keyboard is not the system IME, so the OS does not report its height.
    const geometry = this.model.display.geometry;
    if (keyboardPrefs.keyboardHeight <= geometry.keyboardHeight) {
      return workspaceLayout(this.workspace, geometry, this.model.display.preference);
    }
    const withKeyboard = new FoldGeometry();
    withKeyboard.halfOpen = geometry.halfOpen;
    withKeyboard.creases = geometry.creases;
    withKeyboard.windowLeft = geometry.windowLeft;
    withKeyboard.windowTop = geometry.windowTop;
    withKeyboard.keyboardHeight = keyboardPrefs.keyboardHeight;
    return workspaceLayout(this.workspace, withKeyboard, this.model.display.preference);
  }

  private title(): string {
    const vm = this.model.vm;
    switch (vm.screen) {
      case pb.app.Screen.ONBOARDING: return 'Welcome';
      case pb.app.Screen.INVITE: return 'Invite a device';
      case pb.app.Screen.JOIN: return 'Joining';
      case pb.app.Screen.DEVICE: return vm.network?.members?.find((m): boolean => m.deviceId === vm.selectedDevice)?.name ?? 'Device';
      case pb.app.Screen.AGENTS: return 'Agents';
      case pb.app.Screen.CHAT: return vm.chat?.session?.title || 'Chat';
      case pb.app.Screen.TERMINALS: return 'Terminals';
      case pb.app.Screen.TERMINAL:
        return vm.terminal?.session ? `${vm.terminal.source} · ${vm.terminal.session.substring(0, 8)}` : 'Terminal';
      case pb.app.Screen.SETTINGS: return 'Settings';
      case pb.app.Screen.DESKTOP_CHATS: return 'Chat';
      case pb.app.Screen.FILES: return vm.files?.path || 'Files';
      case pb.app.Screen.FILE: return vm.file?.path ?? 'File';
      case pb.app.Screen.GIT: return vm.git?.diffCommit ? vm.git.diffCommit.substring(0, 8) : 'Git log';
      case pb.app.Screen.BROWSER: return 'Agent Halo Browser';
      default: return 'Muster';
    }
  }

  /** The tabs' own screens: the switcher replaces the title (the core offers no Back there). */
  private atRoot(): boolean {
    const screen = this.model.vm.screen;
    return screen === pb.app.Screen.AGENTS || screen === pb.app.Screen.DESKTOP_CHATS || screen === pb.app.Screen.TERMINALS;
  }

  /** What a side-by-side split shows beside the workspace's list. */
  private static readonly details: pb.app.Screen[] = [pb.app.Screen.CHAT, pb.app.Screen.FILES, pb.app.Screen.FILE,
    pb.app.Screen.GIT, pb.app.Screen.TERMINAL];

  /** The workspace's list on the left, the conversation or terminal on the right. */
  private split(): boolean {
    return this.layout().side && (this.atRoot() || Index.details.indexOf(this.model.vm.screen) >= 0);
  }

  private canGoBack(): boolean {
    // Beside the list a conversation or terminal needs no Back: another row replaces it.
    const screen = this.model.vm.screen;
    const replaced = this.split() && (screen === pb.app.Screen.CHAT || screen === pb.app.Screen.TERMINAL);
    return this.model.vm.canGoBack === true && !replaced && this.model.authenticating === '';
  }

  /** Where the layout button applies: a conversation, a terminal or a split workspace. */
  private showsLayout(): boolean {
    const screen = this.model.vm.screen;
    return this.split() || this.atRoot() || screen === pb.app.Screen.CHAT || screen === pb.app.Screen.TERMINAL;
  }

  private layoutMenu(): MenuElement[] {
    const current = this.model.display.preference;
    const item = (label: string, value: FlexPreference): MenuElement => ({
      value: `${current === value ? '✓ ' : '\u2003'}${label}`,
      action: (): void => { this.model.display.select(value); },
    });
    const items = [item('Automatic', FlexPreference.Automatic), item('Top and bottom', FlexPreference.Always),
      item('Side by side', FlexPreference.Side), item('Single pane', FlexPreference.Disabled)];
    if (current === FlexPreference.Always) {
      const flipped = this.model.display.flipped;
      items.push({ value: `${flipped ? '✓ ' : '\u2003'}Flip 180°`, action: (): void => { this.model.display.flip(!flipped); } });
    }
    return items;
  }

  // The system back gesture still closes a conversation or terminal shown
  // beside the list; only the title bar drops its Back button there.
  onBackPress(): boolean {
    const vm = this.model.vm;
    if (vm.canGoBack !== true || this.model.authenticating !== '' || this.model.core === null) return false;
    this.model.core.back();
    return true;
  }

  /** A conversation's actions, as the Apple shells offer them. */
  private chatMenu(): MenuElement[] {
    const chat = this.model.vm.chat;
    const items: MenuElement[] = [];
    if (chat?.canChooseModel) {
      items.push({ value: 'Model and effort', action: (): void => { this.chatOptions = true; } });
    }
    items.push({ value: `Subagents (${chat?.session?.subagents?.length ?? 0})`, action: (): void => { this.subagents = true; } });
    if (chat?.canBrowseFiles) {
      items.push({ value: 'Files', action: (): void => { this.model.core?.openFiles(); } });
      items.push({ value: 'Git', action: (): void => { this.model.core?.openGit(); } });
    }
    if (chat?.canInterrupt) {
      items.push({ value: 'Stop', action: (): void => { this.model.core?.agentInterrupt(); } });
    }
    return items;
  }

  build() {
    Column() {
      Row({ space: 4 }) {
        if (this.canGoBack()) {
          Button({ type: ButtonType.Circle }) {
            SymbolGlyph($r('sys.symbol.chevron_left')).fontSize(22).fontColor([$r('sys.color.font_primary')])
          }
          .width(40).height(40).backgroundColor(Color.Transparent).accessibilityText('Back')
          .onClick((): void => { this.model.core?.back(); })
        }
        if ((this.atRoot() || this.split()) && this.model.core !== null) {
          WorkspaceSwitcher({ core: this.model.core, vm: this.model.vm })
          Blank()
        } else if (this.model.vm.screen === pb.app.Screen.CHAT) {
          Column({ space: 1 }) {
            Text(this.title()).fontSize(16).fontWeight(FontWeight.Medium).maxLines(1)
              .textOverflow({ overflow: TextOverflow.Ellipsis })
            Text(chatSubtitle(this.model.vm.chat ?? pb.app.ChatView.create())).fontSize(12).opacity(0.6).maxLines(1)
          }
          .alignItems(HorizontalAlign.Start).layoutWeight(1)
        } else {
          Text(this.title()).fontSize(20).fontWeight(FontWeight.Medium).layoutWeight(1).maxLines(1)
            .textOverflow({ overflow: TextOverflow.Ellipsis })
        }
        if (this.model.vm.busy) { LoadingProgress().width(22).height(22) }
        if (this.showsLayout()) {
          Button({ type: ButtonType.Circle }) {
            SymbolGlyph($r('sys.symbol.foldablescreen')).fontSize(22).fontColor([$r('sys.color.font_primary')])
          }
          .width(40).height(40).backgroundColor(Color.Transparent).accessibilityText('Layout')
          .bindMenu(this.layoutMenu())
        }
        if (this.model.vm.screen === pb.app.Screen.CHAT && this.model.core !== null) {
          Button({ type: ButtonType.Circle }) {
            SymbolGlyph($r('sys.symbol.ellipsis_circle')).fontSize(22)
              .fontColor([$r('sys.color.font_primary')])
          }
          .width(40).height(40).backgroundColor(Color.Transparent).accessibilityText('Conversation actions')
          .bindMenu(this.chatMenu())
          .bindSheet($$this.chatOptions, this.chatOptionsSheet(), { height: '70%', title: { title: 'Model and effort' } })
        }
        if (this.atRoot()) {
          // Settings holds invites and join approvals; the dot says one waits.
          Button({ type: ButtonType.Circle }) {
            Stack({ alignContent: Alignment.TopEnd }) {
              SymbolGlyph($r('sys.symbol.gearshape')).fontSize(22).fontColor([$r('sys.color.font_primary')])
              if ((this.model.vm.pending ?? []).length > 0) {
                Row() {}.width(8).height(8).borderRadius(4).backgroundColor('#E53935')
              }
            }
          }
          .width(40).height(40).backgroundColor(Color.Transparent)
          .accessibilityText((this.model.vm.pending ?? []).length > 0 ? 'Settings, a device is waiting to join' : 'Settings')
          .onClick((): void => { this.model.core?.navigate(pb.app.Screen.SETTINGS); })
        }
      }
      .width('100%').height(52).padding({ left: 8, right: 8 }).alignItems(VerticalAlign.Center)

      Stack() {
        if (this.model.startError !== '') {
          Text(`Muster could not start: ${this.model.startError}`).padding(24)
        } else if (!this.model.started || this.model.core === null) {
          LoadingProgress().width(40).height(40)
        } else if (this.split()) {
          this.splitScreen()
        } else {
          this.screen()
        }
        if (this.model.authenticating !== '') {
          Column() {
            LoadingProgress().width(32).height(32)
            Text(this.model.authenticating).fontSize(14).margin({ top: 8 })
          }
          .padding(20).backgroundColor('#E6FFFFFF').borderRadius(14)
        }
        Column() {
          ForEach(this.model.vm.toasts ?? [], (toast: pb.app.IToast): void => {
            ToastView({ toast: toast, dismiss: (id: string): void => { this.model.core?.dismiss(id); } })
          }, (toast: pb.app.IToast): string => toast.id ?? '')
        }
        .width('100%').height('100%').padding(12).justifyContent(FlexAlign.End)
        .hitTestBehavior(HitTestMode.Transparent)
      }
      .layoutWeight(1).width('100%')
      .onAreaChange((_: Area, area: Area): void => {
        const bounds = new WorkspaceArea();
        bounds.left = Number(area.globalPosition.x);
        bounds.top = Number(area.globalPosition.y);
        bounds.width = Number(area.width);
        bounds.height = Number(area.height);
        this.workspace = bounds;
      })
    }
    .width('100%').height('100%')
    .bindSheet($$this.subagents, this.subagentSheet(), { height: '70%', title: { title: 'Subagents' } })
  }

  @Builder
  subagentSheet() {
    List({ space: 8 }) {
      if ((this.model.vm.chat?.session?.subagents?.length ?? 0) === 0) {
        ListItem() { Text('No subagents').padding(16) }
      }
      ForEach(this.model.vm.chat?.session?.subagents ?? [], (child: pb.v1.IAgentSession): void => {
        ListItem() {
          Button((child.title || child.id || '') + ' · ' + (child.state ?? 'unknown'))
            .buttonStyle(ButtonStyleMode.TEXTUAL)
            .onClick((): void => {
              this.subagents = false;
              this.model.core?.openAgent(child.id ?? '', this.model.vm.chat?.deviceId ?? '');
            })
        }
      }, (child: pb.v1.IAgentSession): string => child.id ?? '')
    }.width('100%').height('100%').padding(16)
  }

  @Builder
  chatOptionsSheet() {
    if (this.model.core !== null) {
      ChatOptionsSheet({ core: this.model.core, chat: this.model.vm.chat ?? pb.app.ChatView.create() })
    }
  }

  @Builder
  splitScreen() {
    Row() {
      Column() { this.listPane() }
        .width(this.layout().listWidth).height('100%')
      Row() { Divider().vertical(true).height('100%') }
        .width(this.layout().gapWidth).height('100%').justifyContent(FlexAlign.Center)
      Column() {
        if (this.atRoot()) {
          WorkspaceWelcome({ workspace: this.model.vm.workspace ?? pb.app.Screen.AGENTS })
        } else {
          this.screen()
        }
      }
      .layoutWeight(1).height('100%')
    }
    .width('100%').height('100%')
  }

  @Builder
  listPane() {
    if (this.model.core !== null) {
      if (this.model.vm.workspace === pb.app.Screen.TERMINALS) {
        TerminalsView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.workspace === pb.app.Screen.DESKTOP_CHATS) {
        DesktopChatsView({ core: this.model.core, vm: this.model.vm })
      } else {
        HomeView({ core: this.model.core, vm: this.model.vm })
      }
    }
  }

  @Builder
  screen() {
    if (this.model.core !== null) {
      if (this.model.vm.screen === pb.app.Screen.ONBOARDING) {
        OnboardingView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.INVITE) {
        InviteView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.JOIN) {
        JoinView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.DEVICE) {
        DeviceView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.AGENTS) {
        HomeView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.DESKTOP_CHATS) {
        DesktopChatsView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.CHAT) {
        ChatView({ core: this.model.core, vm: this.model.vm, layout: this.layout() })
      } else if (this.model.vm.screen === pb.app.Screen.TERMINALS) {
        TerminalsView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.TERMINAL) {
        TerminalView({ core: this.model.core, vm: this.model.vm, layout: this.layout() })
      } else if (this.model.vm.screen === pb.app.Screen.SETTINGS) {
        SettingsView({ core: this.model.core, vm: this.model.vm, model: this.model })
      } else if (this.model.vm.screen === pb.app.Screen.FILES) {
        FilesView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.FILE) {
        FileView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.GIT) {
        GitView({ core: this.model.core, vm: this.model.vm })
      } else if (this.model.vm.screen === pb.app.Screen.BROWSER) {
        BrowserView({ core: this.model.core, vm: this.model.vm })
      } else {
        Text('Unknown screen')
      }
    }
  }
}

/** A core toast; it dismisses itself like the Apple shells' (4 s, errors 8 s). */
@ComponentV2
struct ToastView {
  @Param @Require toast: pb.app.IToast;
  @Param @Require dismiss: (id: string) => void;
  private timer: number = -1;

  aboutToAppear(): void {
    this.timer = setTimeout((): void => { this.dismiss(this.toast.id ?? ''); }, this.toast.error ? 8000 : 4000);
  }

  aboutToDisappear(): void { clearTimeout(this.timer); }

  build() {
    Text(this.toast.text ?? '')
      .fontSize(14).fontColor(Color.White).padding(10).width('100%')
      .backgroundColor(this.toast.error ? '#E6C62828' : '#D9424242').borderRadius(10).margin({ top: 6 })
      .onClick((): void => { this.dismiss(this.toast.id ?? ''); })
  }
}

/** The right pane of a split workspace before anything is chosen. */
@ComponentV2
struct WorkspaceWelcome {
  @Param @Require workspace: pb.app.Screen;

  private text(): string {
    if (this.workspace === pb.app.Screen.TERMINALS) return 'Choose a terminal on the left to watch it and type into it.';
    if (this.workspace === pb.app.Screen.DESKTOP_CHATS) {
      return 'Choose a conversation on the left, or start a new one. It is shown in the desktop app on a Mac, and its replies are read from there.';
    }
    return 'Choose a Claude or Codex session on the left to follow it and send it messages.';
  }

  build() {
    Column({ space: 12 }) {
      SymbolGlyph($r('sys.symbol.foldablescreen')).fontSize(48).fontColor([$r('sys.color.font_secondary')])
      Text(this.text()).fontSize(15).opacity(0.7).textAlign(TextAlign.Center).constraintSize({ maxWidth: 420 })
    }
    .width('100%').height('100%').justifyContent(FlexAlign.Center).padding(24)
  }
}

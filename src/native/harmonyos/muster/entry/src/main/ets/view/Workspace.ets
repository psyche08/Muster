// The tabs above the lists: which workspace (Chat, Agents, Terminals) and,
// within Agents and Terminals, which device. The core owns both choices;
// these only render them and send the intent.
import { MusterCore, ViewModel } from '../core/MusterCore';
import { muster as pb } from '../proto/muster';

/** The tab the core says the screen belongs to (Home counts as Agents). */
function selectedWorkspace(vm: ViewModel): pb.app.Screen {
  const workspace = vm.workspace ?? pb.app.Screen.HOME;
  return workspace === pb.app.Screen.DESKTOP_CHATS || workspace === pb.app.Screen.TERMINALS
    ? workspace : pb.app.Screen.AGENTS;
}

class WorkspaceTab {
  label: string;
  screen: pb.app.Screen;
  constructor(label: string, screen: pb.app.Screen) { this.label = label; this.screen = screen; }
}

const workspaceTabs: WorkspaceTab[] = [
  new WorkspaceTab('Agents', pb.app.Screen.AGENTS),
  new WorkspaceTab('Terminals', pb.app.Screen.TERMINALS),
];

@ComponentV2
export struct WorkspaceSwitcher {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;

  private open(screen: pb.app.Screen): void {
    if (screen === pb.app.Screen.DESKTOP_CHATS) {
      this.core.openDesktopChats();
    } else if (screen === pb.app.Screen.TERMINALS) {
      this.core.openTerminals(this.vm.agents?.deviceId || '*');
    } else {
      this.core.openAgents(this.vm.terminals?.deviceId || '*');
    }
  }

  build() {
    Row() {
      ForEach(workspaceTabs, (tab: WorkspaceTab): void => {
        Text(tab.label)
          .fontSize(14).fontWeight(FontWeight.Medium).textAlign(TextAlign.Center)
          .padding({ left: 12, right: 12, top: 6, bottom: 6 }).borderRadius(14)
          .fontColor(selectedWorkspace(this.vm) === tab.screen ? $r('sys.color.font_primary') : $r('sys.color.font_secondary'))
          .backgroundColor(selectedWorkspace(this.vm) === tab.screen ? $r('sys.color.comp_background_primary') : Color.Transparent)
          .accessibilityText(tab.label)
          .accessibilitySelected(selectedWorkspace(this.vm) === tab.screen)
          .onClick((): void => { if (selectedWorkspace(this.vm) !== tab.screen) this.open(tab.screen); })
      }, (tab: WorkspaceTab): string => tab.label)
    }
    .padding(2).borderRadius(16).backgroundColor($r('sys.color.comp_background_tertiary'))
  }
}

/** All devices, then each device that offers services; a long press opens its details. */
@ComponentV2
export struct DeviceTabs {
  @Param @Require vm: ViewModel;
  @Param @Require selected: string;
  @Param @Require select: (device: string) => void;
  @Param @Require details: (device: string) => void;

  build() {
    Scroll() {
      Row({ space: 8 }) {
        this.tab('All devices', '*', true)
        ForEach(this.vm.serviceDevices ?? [], (member: pb.v1.IMemberView): void => {
          this.tab(member.name ?? '', member.deviceId ?? '', member.connected === true || member.isSelf === true)
        }, (member: pb.v1.IMemberView): string => member.deviceId ?? '')
      }
      .padding({ left: 12, right: 12, top: 8, bottom: 8 })
    }
    .scrollable(ScrollDirection.Horizontal).scrollBar(BarState.Off).align(Alignment.Start).width('100%').flexShrink(0)
  }

  @Builder
  tab(name: string, id: string, connected: boolean) {
    Row({ space: 6 }) {
      Text(name).fontSize(14).fontWeight(FontWeight.Medium).maxLines(1)
        .fontColor(this.selected === id ? Color.White : $r('sys.color.font_primary'))
      if (id !== '*') {
        Row().width(6).height(6).borderRadius(3)
          .backgroundColor(connected ? '#43A047' : (this.selected === id ? '#B3FFFFFF' : '#66999999'))
      }
    }
    .padding({ left: 12, right: 12, top: 7, bottom: 7 }).borderRadius(16)
    .backgroundColor(this.selected === id ? $r('app.color.accent') : $r('sys.color.comp_background_tertiary'))
    .accessibilitySelected(this.selected === id)
    .onClick((): void => { this.select(id); })
    .gesture(LongPressGesture().onAction((): void => { if (id !== '*') this.details(id); }))
  }
}

/** The first letter in upper case: "codex" → "Codex". */
export function capitalized(text: string): string {
  return text.length === 0 ? text : text.charAt(0).toUpperCase() + text.substring(1);
}

/** A byte count as "512 B", "1.5 KB" or "2.0 MB". */
export function bytesText(v: number): string {
  if (v < 1024) return `${v} B`;
  if (v < 1024 * 1024) return `${(v / 1024).toFixed(1)} KB`;
  return `${(v / 1024 / 1024).toFixed(1)} MB`;
}

/** A caption row inside a list, like a section header. */
@Builder
export function SectionHeader(title: string) {
  Text(title).fontSize(13).opacity(0.7).padding({ left: 16, right: 16, top: 12, bottom: 4 })
}

/** A transport or read error shown inline above a list. */
@Builder
export function ErrorLine(text: string) {
  Row({ space: 8 }) {
    SymbolGlyph($r('sys.symbol.exclamationmark_triangle')).fontSize(14).fontColor([$r('sys.color.font_secondary')])
    Text(text).fontSize(12).fontColor($r('sys.color.font_secondary')).layoutWeight(1)
  }
  .width('100%').padding({ left: 16, right: 16, top: 6, bottom: 6 })
}

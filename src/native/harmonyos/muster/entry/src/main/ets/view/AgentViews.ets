// Agent sessions: the list across devices and one session's row.
import { MusterCore, ViewModel, uint64 } from '../core/MusterCore';
import { DeviceTabs, ErrorLine, capitalized } from './Workspace';
import { muster as pb } from '../proto/muster';

@ComponentV2
export struct AgentsView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local refreshing: boolean = false;

  /** The session shown beside the list (a split workspace). */
  private shown(): string {
    return this.vm.screen === pb.app.Screen.CHAT ? `${this.vm.chat?.deviceId}:${this.vm.chat?.session?.id}` : '';
  }

  build() {
    Column() {
      DeviceTabs({ vm: this.vm, selected: this.vm.agents?.deviceId ?? '',
        select: (device: string): void => { this.core.openAgents(device); },
        details: (device: string): void => { this.core.selectDevice(device); } })
      Refresh({ refreshing: $$this.refreshing }) {
        List() {
          ForEach(this.vm.agents?.errors ?? [], (error: string): void => {
            ListItem() { ErrorLine(error) }
          }, (error: string): string => `error-${error}`)
          if ((this.vm.agents?.sessions ?? []).length === 0) {
            ListItem() {
              if (this.vm.agents?.loading) {
                LoadingProgress().width(32).height(32).margin(24)
              } else {
                Text('Codex and Claude sessions on the selected devices show up here.').padding(24).opacity(0.7)
              }
            }.width('100%')
          }
          ForEach(this.vm.agents?.sessions ?? [], (session: pb.v1.IAgentSession): void => {
            ListItem() { SessionRow({ session: session }) }
              .backgroundColor(this.shown() === `${session.deviceId}:${session.id}` ? $r('app.color.accent_soft') : Color.Transparent)
              .onClick((): void => { this.core.openAgent(session.id ?? '', session.deviceId ?? ''); })
          }, (session: pb.v1.IAgentSession): string => `${session.deviceId}:${session.id}`)
        }
        .width('100%').height('100%').divider({ strokeWidth: 0.5, color: $r('sys.color.comp_divider'), startMargin: 16, endMargin: 16 })
      }
      .layoutWeight(1)
      .onRefreshing((): void => {
        this.core.openAgents(this.vm.agents?.deviceId ?? '');
        this.refreshing = false;
      })
    }
    .width('100%').height('100%')
  }
}

@ComponentV2
export struct SessionRow {
  @Param @Require session: pb.v1.IAgentSession;

  build() {
    Column({ space: 3 }) {
      Row({ space: 8 }) {
        Text(capitalized(this.session.provider ?? ''))
          .fontSize(11).padding({ left: 6, right: 6, top: 2, bottom: 2 })
          .backgroundColor(this.session.provider === 'codex' ? '#261E88E5' : '#26FB8C00').borderRadius(8)
        Text(this.session.title || (this.session.id ?? '')).fontSize(16).fontWeight(FontWeight.Medium).maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1)
        Column({ space: 2 }) {
          StateBadge({ state: this.session.state ?? '' })
          if (this.session.processState === 'running' && this.session.state === 'unknown') {
            Text('process alive').fontSize(10).opacity(0.6)
          }
        }
        .alignItems(HorizontalAlign.End)
      }
      Row({ space: 6 }) {
        if (this.session.deviceName) { Text(this.session.deviceName).fontSize(12).fontWeight(FontWeight.Medium) }
        Text(this.session.cwd ?? '').fontSize(12).opacity(0.7).maxLines(1).textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1)
        Text(relativeTime(uint64(this.session.updatedUnix))).fontSize(12).opacity(0.7)
      }
    }
    .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
  }
}

@ComponentV2
export struct StateBadge {
  @Param @Require state: string;

  build() {
    if (this.state === 'running') {
      Text('● working').fontSize(11).fontColor('#43A047')
    } else if (this.state === 'waiting_approval') {
      Text('✋ needs you').fontSize(11).fontColor('#FB8C00')
    } else if (this.state === 'idle' || this.state === 'ended') {
      Text(this.state).fontSize(11).opacity(0.6)
    } else {
      Text('unknown').fontSize(11).opacity(0.6)
    }
  }
}

/** "now", "5 min. ago", "3 hr. ago", "2 days ago" or a date. */
export function relativeTime(unix: number): string {
  if (unix <= 0) return '';
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - unix);
  if (seconds < 60) return 'now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min. ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} hr. ago`;
  if (seconds < 7 * 86400) {
    const days = Math.floor(seconds / 86400);
    return days === 1 ? '1 day ago' : `${days} days ago`;
  }
  const d = new Date(unix * 1000);
  return `${d.getFullYear()}-${(d.getMonth() + 1).toString().padStart(2, '0')}-${d.getDate().toString().padStart(2, '0')}`;
}

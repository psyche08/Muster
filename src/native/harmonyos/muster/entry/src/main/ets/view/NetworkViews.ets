import { FlexPreference } from '../display/WorkspaceLayout';
// Onboarding, home, invite, join, device and settings: the network side.
import { MusterTextArea, MusterTextInput } from '../input/KeyboardFields';
import { copyText, keyboardPrefs } from '../input/Keyboard';
import { generateBarcode, scanCore } from '@kit.ScanKit';
import { image } from '@kit.ImageKit';
import { MusterCore, ViewModel, uint64 } from '../core/MusterCore';
import { MusterModel } from '../core/MusterModel';
import { AgentsView } from './AgentViews';
import { SshSheet } from './TerminalViews';
import { SectionHeader } from './Workspace';
import { muster as pb } from '../proto/muster';

/** Every scope an access view offers: each section's switch, then its items. */
function offeredItems(access: pb.app.IAccessView): pb.app.IAccessItem[] {
  const out: pb.app.IAccessItem[] = [];
  for (const section of access.sections ?? []) {
    if (section.toggle) out.push(section.toggle);
    for (const item of section.items ?? []) out.push(item);
  }
  return out;
}

@ComponentV2
export struct OnboardingView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local creating: boolean = false;
  @Local pasting: boolean = false;
  @Local editingRelay: boolean = false;
  @Local name: string = 'home';
  @Local invite: string = '';

  private relayLabel(): string {
    const relay = this.vm.self?.relay ?? '';
    return relay === '' ? 'Relay: not set' : `Relay: ${relay}`;
  }

  build() {
    Column({ space: 12 }) {
      Blank()
      Text('Muster').fontSize(34).fontWeight(FontWeight.Bold)
      Text(this.vm.self?.name ?? '').fontSize(17)
      Text(this.vm.self?.fingerprint ?? '').fontSize(12).fontFamily('monospace').opacity(0.7)
      Button('Scan an invite').width(260).margin({ top: 16 })
        .onClick((): void => { this.core.join('', this.vm.self?.name ?? '', this.vm.self?.kind ?? 'phone'); })
      Button('Create a network', { type: ButtonType.Capsule, stateEffect: true }).width(260).backgroundColor('#33777777')
        .fontColor($r('sys.color.font_primary'))
        .onClick((): void => { this.creating = true; })
        .bindSheet($$this.creating, this.createSheet(), { height: 240, title: { title: 'New network' } })
      Button('Paste an invite').buttonStyle(ButtonStyleMode.TEXTUAL).fontSize(14)
        .onClick((): void => { this.pasting = true; })
        .bindSheet($$this.pasting, this.pasteSheet(), { height: 320, title: { title: 'Join a network' } })
      // Creating a network and inviting need a relay; joining takes the invite's.
      Button(this.relayLabel()).buttonStyle(ButtonStyleMode.TEXTUAL).fontSize(13)
        .onClick((): void => { this.editingRelay = true; })
        .bindSheet($$this.editingRelay, this.relaySheet(), { height: 340, title: { title: 'Relay' } })
      Blank()
      if ((this.vm.join?.phase ?? 'idle') !== 'idle') {
        Text(this.vm.join?.message ?? '').fontSize(13).opacity(0.7)
      }
    }
    .width('100%').height('100%').padding(24)
  }

  @Builder
  createSheet() {
    Column({ space: 12 }) {
      MusterTextInput({ text: this.name, placeholder: 'Network name', onChange: (v: string): void => { this.name = v; } })
      Button('Create').width('100%').enabled(this.name.trim() !== '')
        .onClick((): void => { this.core.createNetwork(this.name.trim()); this.creating = false; })
    }
    .padding(16)
  }

  @Builder
  pasteSheet() {
    Column({ space: 12 }) {
      MusterTextArea({ text: this.invite, placeholder: 'muster1:…', monospace: true, minHeight: 120, maxHeight: 120,
        onChange: (v: string): void => { this.invite = v; } })
      Text("Paste the invite text shown on the admin's device.").fontSize(12).opacity(0.7)
      Button('Join').width('100%').enabled(this.invite.trim().startsWith('muster1:'))
        .onClick((): void => {
          this.core.join(this.invite.trim(), this.vm.self?.name ?? '', this.vm.self?.kind ?? 'phone');
          this.invite = '';
          this.pasting = false;
        })
    }
    .padding(16)
  }

  @Builder
  relaySheet() {
    RelayEditor({ core: this.core, vm: this.vm })
  }
}

/** The relay this device registers with, in Settings and on the welcome screen. */
@ComponentV2
export struct RelayEditor {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local relay: string = '';
  private shown: string = '';

  aboutToAppear(): void { this.shown = this.vm.self?.relay ?? ''; this.relay = this.shown; }

  // Every frame is a new ViewModel; only a changed relay replaces the draft.
  @Monitor('vm')
  relayChanged(): void {
    const relay = this.vm.self?.relay ?? '';
    if (relay !== this.shown) { this.shown = relay; this.relay = relay; }
  }

  build() {
    Column({ space: 8 }) {
      MusterTextInput({ text: this.relay, placeholder: 'relay.example.com:9443', monospace: true,
        onChange: (v: string): void => { this.relay = v; } })
      Row() {
        Text(this.vm.self?.state?.relay ?? '').fontSize(13).opacity(0.7).layoutWeight(1)
        Button('Use this relay').buttonStyle(ButtonStyleMode.TEXTUAL)
          .enabled(this.relay.trim() !== '' && this.relay.trim() !== (this.vm.self?.relay ?? ''))
          .onClick((): void => { this.core.setRelay(this.relay.trim()); })
      }
      .width('100%')
      Text('Where this device registers so members can reach it from anywhere. TCP/TLS and UDP/QUIC share host:port and are selected automatically. Both verify the server certificate. Joining a network switches to the relay in its invite.')
        .fontSize(12).opacity(0.7)
    }
    .alignItems(HorizontalAlign.Start).width('100%').padding(16)
  }
}

@ComponentV2
export struct HomeView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local deciding: pb.v1.IPendingJoin | null = null;
  @Local picking: boolean = false;

  build() {
    Column() {
      ForEach(this.vm.pending ?? [], (join: pb.v1.IPendingJoin): void => {
        PendingJoinRow({ join: join, approve: (): void => { this.deciding = join; this.picking = true; },
          reject: (): void => { this.core.decideJoin(join.deviceId ?? '', false, [], 'rejected'); } })
      }, (join: pb.v1.IPendingJoin): string => join.deviceId ?? '')
      AgentsView({ core: this.core, vm: this.vm }).layoutWeight(1)
    }
    .width('100%').height('100%')
    .bindSheet($$this.picking, this.scopeSheet(), { height: 560, title: { title: `Approve ${this.deciding?.name ?? ''}` } })
  }

  @Builder
  scopeSheet() {
    AccessEditor({ access: this.vm.joinAccess ?? pb.app.AccessView.create(), onSave: (scopes: string[]): void => {
      this.core.decideJoin(this.deciding?.deviceId ?? '', true, scopes);
      this.picking = false;
    } })
  }
}

/** A device asking to join, on Home and in Settings. */
@ComponentV2
export struct PendingJoinRow {
  @Param @Require join: pb.v1.IPendingJoin;
  @Param @Require approve: () => void;
  @Param @Require reject: () => void;

  build() {
    Row({ space: 8 }) {
      SymbolGlyph($r('sys.symbol.person_badge_plus')).fontSize(20).fontColor([$r('app.color.accent')])
      Column({ space: 2 }) {
        Text(`${this.join.name} wants to join`).fontSize(15).fontWeight(FontWeight.Medium).maxLines(1)
        Text(`${this.join.kind} · ${this.join.fingerprint}`).fontSize(11).fontFamily('monospace').opacity(0.7).maxLines(1)
      }
      .alignItems(HorizontalAlign.Start).layoutWeight(1)
      Button('Approve…').fontSize(13).height(32).onClick((): void => { this.approve(); })
      Button('Reject').fontSize(13).height(32).buttonStyle(ButtonStyleMode.TEXTUAL).fontColor('#C62828')
        .onClick((): void => { this.reject(); })
    }
    .width('100%').padding({ left: 16, right: 8, top: 8, bottom: 8 })
    .backgroundColor($r('sys.color.comp_background_tertiary'))
  }
}

/** A device's release version and source commit as the core reports them. */
function buildText(member: pb.v1.IMemberView): string {
  return [member.version ?? '', member.commit ?? ''].filter((part: string): boolean => part !== '').join(' · ');
}

@ComponentV2
export struct MemberRow {
  @Param @Require member: pb.v1.IMemberView;

  private subtitle(): string {
    return [this.member.vip ?? '', this.member.link ?? ''].filter((part: string): boolean => part !== '').join(' · ');
  }

  build() {
    Row({ space: 12 }) {
      Text(this.member.kind === 'phone' ? '📱' : this.member.kind === 'server' ? '🖥' : '💻').fontSize(22)
      Column({ space: 2 }) {
        Row({ space: 6 }) {
          Text(this.member.name ?? '').fontSize(17).fontWeight(FontWeight.Medium)
          if (this.member.isSelf) { Text('this device').fontSize(11).opacity(0.7) }
          if (this.member.role === 'admin') { Text('admin').fontSize(11).fontColor($r('app.color.accent')) }
        }
        Text(this.subtitle()).fontSize(13).opacity(0.7)
        if (buildText(this.member) !== '') {
          Text(buildText(this.member)).fontSize(11).fontFamily('monospace').opacity(0.6)
        }
      }
      .alignItems(HorizontalAlign.Start).layoutWeight(1)
      if (!this.member.isSelf) {
        Row() {}
          .width(8).height(8).borderRadius(4).backgroundColor(this.member.connected ? '#43A047' : '#66999999')
      }
    }
    .width('100%').padding({ left: 16, right: 16, top: 10, bottom: 10 })
  }
}

/**
 * Edits a member's access as the core groups it: each module's device-level
 * switch, its providers (disabled while the switch is off), then the rest.
 * Sends back every scope left on; the core validates and signs.
 */
@ComponentV2
export struct AccessEditor {
  @Param @Require access: pb.app.IAccessView;
  @Param memberRole: string = '';
  @Param @Require onSave: (scopes: string[], role: string, clearSavedPorts: boolean) => void;
  @Local role: string = '';
  private originalRole: string = '';
  private originalScopes: string[] = [];
  private originalAdvanced: boolean = false;
  private originallyHadSavedPorts: boolean = false;
  @Local clearSavedPorts: boolean = false;
  @Local protocol: string = 'tcp';
  @Local customScopes: string[] = [];
  @Local chatProvider: string = '';
  @Local agentProvider: string = '';
  @Local selected: string[] = [];
  @Local newPort: string = "";

  aboutToAppear(): void {
    this.originalRole = this.memberRole;
    this.role = this.originalRole;
    this.originalAdvanced = this.access.networkAdvanced === true;
    this.originallyHadSavedPorts = this.access.hasSavedPortRules === true;
    this.originalScopes = offeredItems(this.access)
      .filter((item): boolean => item.granted === true)
      .map((item): string => item.scope ?? '');
    this.selected = this.originalScopes.slice();
  }

  private accessChanged(): boolean {
    if ((this.access.hasSavedPortRules === true) !== this.originallyHadSavedPorts) return true;
    if (this.memberRole !== this.originalRole || (this.access.networkAdvanced === true) !== this.originalAdvanced) return true;
    const current = offeredItems(this.access).filter((item): boolean => item.granted === true);
    return current.length !== this.originalScopes.length ||
      current.some((item): boolean => this.originalScopes.indexOf(item.scope ?? '') < 0);
  }

  private isOn(scope: string): boolean {
    return this.selected.indexOf(scope) >= 0;
  }

  private addScope(scope: string): void {
    if (!offeredItems(this.access).some((item): boolean => item.scope === scope) && this.customScopes.indexOf(scope) < 0) {
      this.customScopes = this.customScopes.concat([scope]);
    }
    if (!this.isOn(scope)) this.selected = this.selected.concat([scope]);
  }

  private toggleOf(section: pb.app.IAccessSection): pb.app.IAccessItem {
    return section.toggle ?? pb.app.AccessItem.create();
  }

  build() {
    Scroll() {
      Column() {
        if (this.memberRole !== '') {
          Row() {
            Text('Network role').layoutWeight(1)
            Select([{ value: 'Member' }, { value: 'Administrator' }])
              .value(this.role === 'admin' ? 'Administrator' : 'Member')
              .selected(this.role === 'admin' ? 1 : 0)
              .onSelect((index: number): void => { this.role = index === 1 ? 'admin' : 'member'; })
          }.width('100%').padding({ bottom: 12 })
        }
        ForEach(this.access.sections ?? [], (section: pb.app.IAccessSection): void => {
          Column() {
            if (section.toggle) {
              this.item(this.toggleOf(section), true, false, true)
            } else {
              Text(section.title ?? '').fontSize(13).opacity(0.7).padding({ top: 12, bottom: 4 })
            }
            ForEach(section.items ?? [], (item: pb.app.IAccessItem): void => {
              this.item(item, item.requiresToggle !== true || this.isOn(this.toggleOf(section).scope ?? ''),
                item.requiresToggle === true, false)
            }, (item: pb.app.IAccessItem): string => item.scope ?? '')
            if (section.id === 'network' && this.memberRole !== '' &&
              this.access.networkAdvanced !== true && this.access.hasSavedPortRules) {
              Row() {
                Text('Clear saved port rules').layoutWeight(1)
                Toggle({ type: ToggleType.Switch, isOn: this.clearSavedPorts })
                  .onChange((on: boolean): void => { this.clearSavedPorts = on; })
              }.width('100%').padding({ top: 8, bottom: 8 })
            }
            if (section.id === 'network' && this.access.networkAdvanced === true) {
              this.customItems('net:', true)
              Row({ space: 8 }) {
                Select([{ value: 'TCP' }, { value: 'UDP' }, { value: 'TCP & UDP' }])
                  .selected(this.protocol === 'tcp' ? 0 : this.protocol === 'udp' ? 1 : 2)
                  .onSelect((index: number): void => { this.protocol = ['tcp', 'udp', '*'][index]; })
                MusterTextInput({ text: this.newPort, placeholder: 'Port or *', monospace: true,
                  onChange: (value: string): void => { this.newPort = value; } }).layoutWeight(1)
                Button('Add').enabled(this.newPort.trim() !== '').onClick((): void => {
                  this.addScope('net:' + this.protocol + '/' + this.newPort.trim());
                  this.newPort = '';
                })
              }
            }
            if (section.id === 'chats' || section.id === 'agents') {
              this.customItems(section.id + ':', this.isOn(this.toggleOf(section).scope ?? ''))
              Row({ space: 8 }) {
                MusterTextInput({ text: section.id === 'chats' ? this.chatProvider : this.agentProvider,
                  placeholder: 'Provider ID', onChange: (value: string): void => {
                    if (section.id === 'chats') this.chatProvider = value;
                    else this.agentProvider = value;
                  } }).layoutWeight(1)
                Button('Add').enabled((section.id === 'chats' ? this.chatProvider : this.agentProvider).trim() !== '')
                  .onClick((): void => {
                    const provider = (section.id === 'chats' ? this.chatProvider : this.agentProvider).trim();
                    this.addScope(section.id + ':' + provider);
                    if (section.id === 'chats') this.chatProvider = '';
                    else this.agentProvider = '';
                  })
              }.enabled(this.isOn(this.toggleOf(section).scope ?? ''))
            }
            if (section.footer) {
              Text(section.footer).fontSize(12).opacity(0.7).padding({ top: 4 })
            }
          }
          .alignItems(HorizontalAlign.Start).width('100%').padding({ bottom: 12 })
        }, (section: pb.app.IAccessSection): string => section.id ?? '')
        if (this.accessChanged()) {
          Text('Permissions changed. Reopen this editor to review the current settings.').fontSize(13)
        }
        Button('Save').width('100%').margin({ top: 12 }).enabled(!this.accessChanged())
          .onClick((): void => {
            this.onSave(offeredItems(this.access).map((item): string => item.scope ?? '')
              .concat(this.customScopes).filter((scope): boolean => this.isOn(scope)),
              this.role === this.originalRole ? '' : this.role, this.clearSavedPorts);
          })
      }
      .padding(16)
    }
    // Without a bounded height the sheet clips the list and Save is unreachable.
    .width('100%').height('100%').align(Alignment.Top).edgeEffect(EdgeEffect.Spring)
  }

  @Builder
  customItems(prefix: string, enabled: boolean) {
    ForEach(this.customScopes.filter((scope: string): boolean => scope.startsWith(prefix)), (scope: string): void => {
      this.item(pb.app.AccessItem.create({ scope: scope, label: scope }), enabled, false, false)
    }, (scope: string): string => scope)
  }

  @Builder
  item(item: pb.app.IAccessItem, enabled: boolean, indent: boolean, strong: boolean) {
    Row() {
      Text(item.label ?? '').layoutWeight(1).opacity(enabled ? 1 : 0.4)
        .fontWeight(strong ? FontWeight.Medium : FontWeight.Normal)
      Toggle({ type: ToggleType.Checkbox, isOn: this.isOn(item.scope ?? '') })
        .enabled(enabled)
        .onChange((on: boolean): void => {
          const scope = item.scope ?? '';
          this.selected = on ? this.selected.concat([scope]) : this.selected.filter((s): boolean => s !== scope);
        })
    }
    .width('100%').padding({ left: indent ? 24 : 0, top: 8, bottom: 8 })
  }
}

@ComponentV2
export struct InviteView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local qr: image.PixelMap | null = null;
  @Local rendered: string = '';
  @Local qrError: string = '';
  @Local secondsLeft: number = 0;
  private timer: number = -1;

  aboutToAppear(): void {
    this.timer = setInterval((): void => {
      this.secondsLeft = uint64(this.vm.invite?.expiresUnix) - Math.floor(Date.now() / 1000);
      this.render();
    }, 1000);
  }

  aboutToDisappear(): void { clearInterval(this.timer); }

  private render(): void {
    const text = this.vm.invite?.text ?? '';
    if (text === '' || text === this.rendered) return;
    this.rendered = text;
    const options: generateBarcode.CreateOptions = { scanType: scanCore.ScanType.QR_CODE, height: 640, width: 640, margin: 2 };
    generateBarcode.createBarcode(text, options)
      .then((pixelMap: image.PixelMap): void => { this.qr = pixelMap; this.qrError = ''; })
      .catch((e: Error): void => { this.qr = null; this.qrError = e.message || 'no QR code on this device'; });
  }

  build() {
    Column({ space: 16 }) {
      if ((this.vm.invite?.text ?? '') === '') {
        LoadingProgress().width(40).height(40)
      } else {
        if (this.qr !== null) {
          Image(this.qr).width(280).height(280)
        } else if (this.qrError !== '') {
          Text(`QR code unavailable: ${this.qrError}. Copy the text instead.`).fontSize(13).opacity(0.7).textAlign(TextAlign.Center)
        }
        Text(this.vm.invite?.text ?? '').fontSize(11).fontFamily('monospace').maxLines(4).copyOption(CopyOptions.LocalDevice)
        Text(this.secondsLeft > 0 ? `Expires in ${Math.floor(this.secondsLeft / 60)}:${(this.secondsLeft % 60).toString().padStart(2, '0')}` : 'Expired')
          .fontSize(13).opacity(0.7)
        Row({ space: 12 }) {
          Button('Copy').buttonStyle(ButtonStyleMode.TEXTUAL).onClick((): void => { copyText(this.vm.invite?.text ?? ''); })
          Button('New invite').buttonStyle(ButtonStyleMode.TEXTUAL).onClick((): void => { this.core.showInvite(); })
        }
      }
      Text('Scan on the phone, or paste the text on a desktop. One invite admits one device; approve it here afterwards.')
        .fontSize(13).opacity(0.7).textAlign(TextAlign.Center)
    }
    .width('100%').height('100%').padding(24)
  }
}

@ComponentV2
export struct JoinView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;

  build() {
    Column({ space: 16 }) {
      Blank()
      if (this.vm.join?.phase !== 'failed' && this.vm.join?.phase !== 'joined') { LoadingProgress().width(40).height(40) }
      Text(this.vm.join?.message ?? '').textAlign(TextAlign.Center)
      if (this.vm.join?.phase === 'failed') { Button('Back').onClick((): void => { this.core.back(); }) }
      Blank()
    }
    .width('100%').height('100%').padding(24)
  }
}

@ComponentV2
export struct DeviceView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local editing: boolean = false;
  @Local editingDevice: string = '';
  @Local grantLockedControl: boolean = false;
  @Local ssh: boolean = false;

  private member(): pb.v1.IMemberView | undefined {
    return this.vm.network?.members?.find((m): boolean => m.deviceId === this.vm.selectedDevice);
  }

  /** What this page offers for the member, as the core decided it. */
  private actions(): pb.app.IDeviceActions { return this.vm.deviceActions ?? pb.app.DeviceActions.create(); }

  build() {
    Column() {
      if (this.member() === undefined) {
        Text('No such device').padding(24)
      } else {
        List() {
          ListItem() { this.row('Kind', this.member()?.kind ?? '') }
          ListItem() { this.row('Role', this.member()?.role ?? '') }
          if (this.member()?.vip) { ListItem() { this.row('Virtual IP', this.member()?.vip ?? '') } }
          ListItem() { this.row('Fingerprint', this.member()?.fingerprint ?? '') }
          if ((this.member()?.link ?? '') !== '') { ListItem() { this.row('Link', this.member()?.link ?? '') } }
          // From the link heartbeat; empty until the device reports it.
          ListItem() { this.row('Version', this.member()?.version || 'not reported') }
          ListItem() { this.row('Commit', this.member()?.commit || 'not reported') }
          if (hasDeviceControls(this.vm, this.member()?.deviceId ?? '')) {
            ListItem() { DeviceControlTiles({ core: this.core, vm: this.vm, deviceId: this.member()?.deviceId ?? '' }) }
          }
          if (this.vm.selectedDeviceHasServices) {
            ListItem() { SectionHeader('Use') }
            ListItem() { this.action('Chrome browser', (): void => { this.core.openBrowser(this.member()?.deviceId ?? ''); }) }
            ListItem() { this.action('Agent sessions', (): void => { this.core.openAgents(); }) }
            ListItem() { this.action('Terminals', (): void => { this.core.openTerminals(); }) }
            if (this.actions().canCover) {
              ListItem() { this.action('Cover its screen', (): void => { this.core.setCover(true, this.member()?.deviceId ?? ''); }) }
              ListItem() { this.action('Uncover its screen', (): void => { this.core.setCover(false, this.member()?.deviceId ?? ''); }) }
            }
            if (this.actions().canSsh) {
              ListItem() { this.action('SSH', (): void => { this.ssh = true; }) }
            }
          }
          if (this.actions().showAccess) {
            ListItem() { Text('Access to other devices').fontSize(13).opacity(0.7).padding({ left: 16, top: 12, bottom: 4 }) }
            ForEach(this.vm.deviceAccess?.sections ?? [], (section: pb.app.IAccessSection): void => {
              ListItem() { this.row(section.title ?? '', section.summary ?? '') }
            }, (section: pb.app.IAccessSection): string => section.id ?? '')
          }
          if (this.actions().canEditAccess) {
            ListItem() { this.action('Change access…', (): void => {
              this.editingDevice = this.vm.selectedDevice ?? '';
              this.editing = true;
            }) }
          }
          if (this.actions().presenceEnrolled != null) {
            ListItem() { SectionHeader('Presence key') }
            ListItem() { this.row('Enrollment', this.actions().presenceEnrolled ? 'Enrolled' : 'Not enrolled') }
            ListItem() {
              Row() {
                Text('Also grant control while locked').layoutWeight(1)
                Toggle({ type: ToggleType.Switch, isOn: this.grantLockedControl })
                  .onChange((value: boolean): void => { this.grantLockedControl = value; })
              }.width('100%').padding(16)
            }
            ListItem() {
              this.action(this.actions().presenceEnrolled ? 'Update enrollment' : 'Enroll key', (): void => {
                this.core.setMemberPresence(this.vm.selectedDevice ?? '', false, this.grantLockedControl);
                this.grantLockedControl = false;
              })
            }
            if (this.actions().presenceEnrolled) {
              ListItem() { this.action('Revoke enrollment', (): void => {
                this.core.setMemberPresence(this.vm.selectedDevice ?? '', true, false);
              }) }
            }
          }
          if (this.actions().canRemove) {
            ListItem() {
              Text('Remove from the network').fontColor('#C62828').padding(16)
                .onClick((): void => { this.confirmRemove(); })
            }
          }
        }
        .width('100%').layoutWeight(1)
        // A component shows one sheet: the SSH sheet hangs on the list.
        .bindSheet($$this.ssh, this.sshSheet(), { height: 300, title: { title: 'SSH' } })
      }
    }
    .width('100%').height('100%')
    .bindSheet($$this.editing, this.scopeSheet(), { height: 560, title: { title: `Access for ${this.member()?.name ?? ''}` } })
  }

  private confirmRemove(): void {
    const member = this.member();
    if (member === undefined) return;
    this.getUIContext().showAlertDialog({
      title: `Remove ${member.name}?`,
      message: 'Its links close within two seconds and it cannot rejoin without a new invite.',
      primaryButton: { value: 'Cancel', action: (): void => {} },
      secondaryButton: { value: 'Remove', fontColor: '#C62828', action: (): void => { this.core.removeDevice(member.deviceId ?? ''); } },
    });
  }

  @Builder
  row(label: string, value: string) {
    Row() {
      Text(label).opacity(0.7)
      Blank()
      Text(value).fontFamily(label === 'Fingerprint' ? 'monospace' : 'sans-serif')
    }
    .width('100%').padding(16)
  }

  @Builder
  action(label: string, run: () => void) {
    Text(label).fontColor($r('app.color.accent')).padding(16).width('100%').onClick((): void => { run(); })
  }

  @Builder
  scopeSheet() {
    AccessEditor({ access: this.vm.deviceAccess ?? pb.app.AccessView.create(), memberRole: this.member()?.role ?? '',
      onSave: (scopes: string[], role: string, clearSavedPorts: boolean): void => {
        if (this.editingDevice === this.vm.selectedDevice && this.actions().canEditAccess) {
          this.core.setScopes(this.editingDevice, scopes, role, clearSavedPorts);
        }
        this.editing = false;
      } })
  }

  @Builder
  sshSheet() {
    SshSheet({ core: this.core, close: (): void => { this.ssh = false; } })
  }
}

class LayoutOption {
  label: string;
  value: FlexPreference;
  constructor(label: string, value: FlexPreference) { this.label = label; this.value = value; }
}

const layoutOptions: LayoutOption[] = [
  new LayoutOption('Automatic', FlexPreference.Automatic), new LayoutOption('Top and bottom', FlexPreference.Always),
  new LayoutOption('Side by side', FlexPreference.Side), new LayoutOption('Single pane', FlexPreference.Disabled),
];

export enum SettingsGroup {
  Index = 'Settings',
  Permissions = 'Permissions',
  General = 'Layout and keyboard',
  Network = 'Network',
  Device = 'This device'
}

@ComponentV2
export struct SettingsView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Param @Require model: MusterModel;
  @Param @Require group: SettingsGroup;
  @Event select: (group: SettingsGroup) => void = (): void => {};
  @Local deciding: pb.v1.IPendingJoin | null = null;
  @Local picking: boolean = false;

  build() {
    List({ space: 4 }) {
      if (this.group === SettingsGroup.Index) {
        ListItem() { this.groupRow(SettingsGroup.Permissions) }
        ListItem() { this.groupRow(SettingsGroup.General) }
        ListItem() { this.groupRow(SettingsGroup.Network) }
        ListItem() { this.groupRow(SettingsGroup.Device) }
      } else if (this.group === SettingsGroup.Permissions) {
        this.permissions()
      } else if (this.group === SettingsGroup.General) {
        this.preferences()
      } else if (this.group === SettingsGroup.Network) {
        ListItem() { SectionHeader('Relay') }
        ListItem() { RelayEditor({ core: this.core, vm: this.vm }) }
      } else if (this.group === SettingsGroup.Device) {
        this.identity()
      }
    }
    .width('100%').height('100%')
    .bindSheet($$this.picking, this.scopeSheet(), { height: 560, title: { title: `Approve ${this.deciding?.name ?? ''}` } })
  }

  @Builder
  groupRow(group: SettingsGroup) {
    Row({ space: 8 }) {
      Text(group).fontSize(17).layoutWeight(1)
      SymbolGlyph($r('sys.symbol.chevron_right')).fontSize(16).fontColor([$r('sys.color.font_secondary')])
    }
    .width('100%').padding(16).borderRadius(12)
    .backgroundColor($r('sys.color.comp_background_secondary')).onClick((): void => { this.select(group); })
  }

  @Builder
  permissions() {
    if (this.vm.network?.accessMode) {
      ListItem() {
        Row() {
          Text('Network access mode')
          Blank()
          if (this.vm.canInvite) {
            Select([{ value: 'Basic' }, { value: 'Advanced' }])
              .selected(this.vm.network?.accessMode === 'basic' ? 0 : 1)
              .onSelect((index: number): void => { this.core.setNetworkAccessMode(index === 0 ? 'basic' : 'advanced'); })
          } else {
            Text(this.vm.network?.accessMode ?? '')
          }
        }.width('100%').padding(16)
      }
    }
    ListItem() { SectionHeader('Member permissions') }
    ForEach(this.vm.pending ?? [], (join: pb.v1.IPendingJoin): void => {
      ListItem() { PendingJoinRow({ join: join, approve: (): void => { this.deciding = join; this.picking = true; },
        reject: (): void => { this.core.decideJoin(join.deviceId ?? '', false, [], 'rejected'); } }) }
    }, (join: pb.v1.IPendingJoin): string => join.deviceId ?? '')
    ForEach(this.vm.network?.members ?? [], (member: pb.v1.IMemberView): void => {
      ListItem() { MemberRow({ member: member }) }
        .onClick((): void => { this.core.selectDevice(member.deviceId ?? ''); })
      if (hasDeviceControls(this.vm, member.deviceId ?? '')) {
        ListItem() { DeviceControlTiles({ core: this.core, vm: this.vm, deviceId: member.deviceId ?? '' }) }
      }
    }, (member: pb.v1.IMemberView): string => member.deviceId ?? '')
    if (this.vm.canInvite) {
      ListItem() {
        Button('Invite device').buttonStyle(ButtonStyleMode.TEXTUAL).margin({ left: 8 })
          .onClick((): void => { this.core.showInvite(); })
      }
    }
  }

  @Builder
  preferences() {
    ListItem() {
      Column({ space: 8 }) {
        Text('Half-fold workspace').fontWeight(FontWeight.Medium)
        Text('Top and bottom: read above a horizontal crease, type and respond below it. Side by side: the list beside the conversation or terminal across a vertical crease. Automatic follows how the phone is folded; while half-folded, a forced layout turns the screen to suit it.')
          .fontSize(13).opacity(0.7)
        Select(layoutOptions.map((option: LayoutOption): SelectOption => ({ value: option.label })))
          .selected(Math.max(0, layoutOptions.findIndex((o: LayoutOption): boolean => o.value === this.model.display.preference)))
          .value(layoutOptions.find((o: LayoutOption): boolean => o.value === this.model.display.preference)?.label ?? 'Automatic')
          .onSelect((index: number): void => { this.model.display.select(layoutOptions[index].value); })
        if (this.model.display.preference === FlexPreference.Always) {
          Row() {
            Column({ space: 2 }) {
              Text('Flip 180°')
              Text('Turns top and bottom upside down while half-folded, for a phone standing on its other edge.')
                .fontSize(13).opacity(0.7)
            }
            .alignItems(HorizontalAlign.Start).layoutWeight(1)
            Toggle({ type: ToggleType.Switch, isOn: this.model.display.flipped })
              .onChange((on: boolean): void => { this.model.display.flip(on); })
          }
          .width('100%')
        }
        if (this.model.display.preferenceError !== '') {
          Text(this.model.display.preferenceError).fontSize(13)
        }
      }
      .alignItems(HorizontalAlign.Start).padding(16).width('100%')
    }

    ListItem() {
      Row() {
        Column({ space: 2 }) {
          Text('Muster keyboard').fontWeight(FontWeight.Medium)
          Text('Pinyin Chinese and English, numbers and symbols in every text field. Off uses the system keyboard.')
            .fontSize(13).opacity(0.7)
        }
        .alignItems(HorizontalAlign.Start).layoutWeight(1)
        Toggle({ type: ToggleType.Switch, isOn: !keyboardPrefs.systemKeyboard })
          .onChange((on: boolean): void => { keyboardPrefs.useSystem(!on); })
      }
      .width('100%').padding({ left: 16, right: 16, top: 8, bottom: 8 })
    }
    ListItem() {
      Row() {
        Column({ space: 2 }) {
          Text('Muster keyboard in terminals').fontWeight(FontWeight.Medium)
          Text('On, the default, types straight into the session and adds Esc, F-keys, Ctrl and Alt, which the system keyboard lacks.')
            .fontSize(13).opacity(0.7)
        }
        .alignItems(HorizontalAlign.Start).layoutWeight(1)
        Toggle({ type: ToggleType.Switch, isOn: !keyboardPrefs.terminalSystemKeyboard })
          .onChange((on: boolean): void => { keyboardPrefs.useSystemInTerminal(!on); })
      }
      .width('100%').padding({ left: 16, right: 16, top: 8, bottom: 8 })
    }
  }

  @Builder
  identity() {
    ListItem() {
      Column({ space: 4 }) {
        Text(this.vm.self?.name ?? '')
        Text(this.vm.self?.fingerprint ?? '').fontFamily('monospace').fontSize(12).opacity(0.7)
        Text(`Key: ${this.vm.self?.keyLabel ?? ''}`).fontSize(13).opacity(0.7)
        Text(`Presence key: ${this.model.keys.presencePublicKey() !== null ? 'HUKS, local authentication' : 'none (set a lock-screen password)'}`)
          .fontSize(13).opacity(0.7)
        Text(`Version: ${this.vm.self?.version || 'not reported'}`).fontSize(13).opacity(0.7)
        Text(`Commit: ${this.vm.self?.commit || 'not reported'}`).fontSize(13).fontFamily('monospace').opacity(0.7)
          .copyOption(CopyOptions.LocalDevice)
      }
      .alignItems(HorizontalAlign.Start).width('100%').padding(16)
    }
    if (this.core.sshPublicKeyLine() !== null) {
      ListItem() {
        Column({ space: 4 }) {
          Text('SSH public key').fontSize(13)
          Text(this.core.sshPublicKeyLine() ?? '').fontFamily('monospace').fontSize(10).maxLines(3).copyOption(CopyOptions.LocalDevice)
          Button('Copy').buttonStyle(ButtonStyleMode.TEXTUAL).onClick((): void => { copyText(this.core.sshPublicKeyLine() ?? ''); })
        }
        .alignItems(HorizontalAlign.Start).width('100%').padding(16)
      }
    }
  }

  @Builder
  scopeSheet() {
    AccessEditor({ access: this.vm.joinAccess ?? pb.app.AccessView.create(), onSave: (scopes: string[]): void => {
      this.core.decideJoin(this.deciding?.deviceId ?? '', true, scopes);
      this.picking = false;
    } })
  }
}

function lockedUseOf(vm: ViewModel, deviceId: string): pb.app.IDeviceLockedUseView | undefined {
  return (vm.deviceLockedUse ?? []).find((v: pb.app.IDeviceLockedUseView): boolean => v.deviceId === deviceId);
}

function maskOf(vm: ViewModel, deviceId: string): pb.app.IDeviceBlackoutView | undefined {
  return (vm.deviceBlackout ?? []).find((v: pb.app.IDeviceBlackoutView): boolean => v.deviceId === deviceId);
}

function hasDeviceControls(vm: ViewModel, deviceId: string): boolean {
  return lockedUseOf(vm, deviceId) !== undefined || maskOf(vm, deviceId) !== undefined;
}

/** Locked Use and the forced mask side by side (ADR 0212); Rust decides what each switch allows. */
@ComponentV2
struct DeviceControlTiles {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Param @Require deviceId: string;
  build() {
    Row({ space: 8 }) {
      if (lockedUseOf(this.vm, this.deviceId) !== undefined) {
        DeviceControlTile({
          title: 'Locked Use',
          status: lockedUseOf(this.vm, this.deviceId)?.status ?? '',
          on: lockedUseOf(this.vm, this.deviceId)?.on ?? false,
          canToggle: lockedUseOf(this.vm, this.deviceId)?.enabled ?? false,
          busy: false,
          onSet: (on: boolean): void => { this.core.setDeviceLockedUse(on, this.deviceId); }
        }).layoutWeight(1)
      }
      if (maskOf(this.vm, this.deviceId) !== undefined) {
        DeviceControlTile({
          title: '强制遮罩',
          status: maskOf(this.vm, this.deviceId)?.status ?? '',
          on: maskOf(this.vm, this.deviceId)?.on ?? false,
          canToggle: maskOf(this.vm, this.deviceId)?.enabled ?? false,
          busy: maskOf(this.vm, this.deviceId)?.busy ?? false,
          onSet: (on: boolean): void => { this.core.setBlackout(on, this.deviceId); }
        }).layoutWeight(1)
      }
    }.width('100%').alignItems(VerticalAlign.Top).padding({ left: 16, right: 16, top: 4, bottom: 4 })
  }
}

@ComponentV2
struct DeviceControlTile {
  @Param title: string = '';
  @Param status: string = '';
  @Param on: boolean = false;
  @Param canToggle: boolean = false;
  @Param busy: boolean = false;
  @Event onSet: (on: boolean) => void = (): void => {};
  build() {
    Column({ space: 2 }) {
      Row({ space: 6 }) {
        Text(this.title).fontSize(14).fontWeight(FontWeight.Medium).maxLines(1)
          .textOverflow({ overflow: TextOverflow.Ellipsis }).layoutWeight(1)
        if (this.busy) {
          LoadingProgress().width(16).height(16)
        }
        Toggle({ type: ToggleType.Switch, isOn: this.on })
          .enabled(this.canToggle)
          .onChange((on: boolean): void => { this.onSet(on); })
      }.width('100%')
      Text(this.status).fontSize(12).fontColor($r('sys.color.font_secondary')).maxLines(3)
        .textOverflow({ overflow: TextOverflow.Ellipsis })
    }
    .alignItems(HorizontalAlign.Start)
    .padding({ left: 12, right: 12, top: 8, bottom: 8 })
    .backgroundColor($r('sys.color.comp_background_secondary'))
    .borderRadius(12)
  }
}

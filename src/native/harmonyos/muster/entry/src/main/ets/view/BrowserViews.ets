// The Chrome extension (Agent Halo Browser) on one device: its setup, and
// the ad-hoc requests the core forwards to it. The target decides what the
// extension offers and whether a request is allowed; this screen shows the
// core's status and result and collects the request text.
import { MusterTextArea } from '../input/KeyboardFields';
import { MusterCore, ViewModel } from '../core/MusterCore';
import { muster as pb } from '../proto/muster';
import { SectionHeader } from './Workspace';

@ComponentV2
export struct BrowserView {
  @Param @Require core: MusterCore;
  @Param @Require vm: ViewModel;
  @Local request: string = '';
  private shownDraft: string = '';

  private browser(): pb.app.IBrowserView { return this.vm.browser ?? pb.app.BrowserView.create(); }
  private status(): pb.v1.IBrowserStatus { return this.browser().status ?? pb.v1.BrowserStatus.create(); }
  private busy(): boolean { return this.browser().busy === true; }

  aboutToAppear(): void {
    this.shownDraft = this.browser().draft ?? '';
    this.request = this.shownDraft;
  }

  // Every frame is a new ViewModel; only a changed draft (a browser link the
  // core received) replaces what is being typed.
  @Monitor('vm')
  draftChanged(): void {
    const draft = this.browser().draft ?? '';
    if (draft !== this.shownDraft) {
      this.shownDraft = draft;
      this.request = draft;
    }
  }

  build() {
    Stack() {
      List() {
        ListItem() { SectionHeader('Chrome extension') }
        ListItem() {
          Column({ space: 10 }) {
            Row({ space: 8 }) {
              if (this.status().connected) {
                SymbolGlyph($r('sys.symbol.checkmark_circle_fill')).fontSize(18).fontColor(['#43A047'])
                Text('Connected').fontSize(15).fontColor('#43A047')
              } else {
                SymbolGlyph($r('sys.symbol.exclamationmark_triangle')).fontSize(18).fontColor([$r('sys.color.font_secondary')])
                Text('Setup needed').fontSize(15).opacity(0.7)
              }
            }
            Text(this.status().message || 'Checking the target device…').fontSize(14)
            if (this.browser().local) {
              Button('1. Set up native host').buttonStyle(ButtonStyleMode.TEXTUAL).enabled(!this.busy())
                .onClick((): void => { this.core.browserSetup(true); })
              Text('In Chrome, turn on Developer mode, choose Load unpacked, and select the AgentHaloBrowser folder. Chrome asks for browser debugging permission.')
                .fontSize(13).opacity(0.7)
              if ((this.status().extensionPath ?? '') !== '') {
                Text(this.status().extensionPath ?? '').fontSize(12).fontFamily('monospace').copyOption(CopyOptions.LocalDevice)
              }
            } else {
              Text('On the target Mac, open Muster → Settings → Chrome extension and complete installation. Then return here and check the connection.')
                .fontSize(13).opacity(0.7)
            }
            Button('Check connection').buttonStyle(ButtonStyleMode.TEXTUAL).enabled(!this.busy())
              .onClick((): void => { this.core.browserSetup(); })
          }
          .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 4, bottom: 8 })
        }
        ListItem() { SectionHeader('Remote access & Locked Use') }
        ListItem() {
          Column({ space: 8 }) {
            Text('Grant this device browser.view to read pages and browser.control to change them in Network → device → permissions. These permissions are separate from agent access.')
              .fontSize(13).opacity(0.7)
            Text('For a locked Mac, also grant locked.control and enable control while locked on the target. Authenticate each remote request. Browser operations keep the Mac locked; no screen cover or temporary unlock is needed.')
              .fontSize(13).opacity(0.7)
          }
          .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 4, bottom: 8 })
        }
        ListItem() { SectionHeader('Browser request') }
        ListItem() {
          Column({ space: 10 }) {
            Row({ space: 12 }) {
              Button('List tabs').buttonStyle(ButtonStyleMode.NORMAL).fontSize(13).enabled(!this.busy())
                .onClick((): void => { this.request = '{"op":"tabs"}'; })
              Button('Connection test').buttonStyle(ButtonStyleMode.NORMAL).fontSize(13).enabled(!this.busy())
                .onClick((): void => { this.request = '{"op":"ping"}'; })
            }
            MusterTextArea({ text: this.request, placeholder: '{"op":"tabs"}', monospace: true, fontSize: 14,
              minHeight: 120, maxHeight: 240, editable: !this.busy(),
              onChange: (v: string): void => { this.request = v; } })
            Text('Use snapshot with a tabId first. Click/type/press also need document and ref from that snapshot.')
              .fontSize(12).opacity(0.7)
            Button(this.browser().local ? 'Run' : 'Run & authenticate').width('100%')
              .enabled(!this.busy() && this.request.trim() !== '')
              .onClick((): void => { this.core.browserRun(this.request); })
          }
          .alignItems(HorizontalAlign.Start).width('100%').padding({ left: 16, right: 16, top: 4, bottom: 8 })
        }
        if ((this.browser().result ?? '') !== '') {
          ListItem() { SectionHeader('Result') }
          ListItem() {
            Text(this.browser().result ?? '').fontSize(12).fontFamily('monospace').copyOption(CopyOptions.LocalDevice)
              .width('100%').padding({ left: 16, right: 16, top: 4, bottom: 16 })
          }
        }
      }
      .width('100%').height('100%')
      if (this.busy()) {
        Column({ space: 8 }) {
          LoadingProgress().width(32).height(32)
          Text('Waiting for browser…').fontSize(14)
        }
        .padding(20).backgroundColor($r('sys.color.comp_background_primary')).borderRadius(14)
      }
    }
    .width('100%').height('100%')
  }
}

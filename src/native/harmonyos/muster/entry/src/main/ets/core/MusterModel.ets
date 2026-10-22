// The one object the pages observe: the latest ViewModel, and the effects
// the platform is in the middle of. It performs effects and forwards
// intents; it decides nothing.
import { FoldDisplay } from '../display/FoldDisplay';
import { keyboardPrefs } from '../input/Keyboard';
import { bundleManager, common } from '@kit.AbilityKit';
import { scanBarcode, scanCore } from '@kit.ScanKit';
import { notificationManager } from '@kit.NotificationKit';
import { connection, socket } from '@kit.NetworkKit';
import { window } from '@kit.ArkUI';
import { deviceInfo } from '@kit.BasicServicesKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { MusterConfig, MusterCore, MusterKeys, SoftwareKeys, ViewModel, Effect } from './MusterCore';
import { HuksKeys, localAuthenticate } from './Keys';
import { muster as pb } from '../proto/muster';

const DOMAIN = 0x4841;
/** The AppStorageV2 key the ability shares its model under. */
export const MODEL_KEY = 'muster';

@ObservedV2
export class MusterModel {
  @Trace vm: ViewModel = pb.app.ViewModel.create();
  @Trace started: boolean = false;
  @Trace startError: string = '';
  /** Local authentication in progress: the reason shown. */
  @Trace authenticating: string = '';
  readonly display: FoldDisplay = new FoldDisplay();
  core: MusterCore | null = null;
  keys: MusterKeys = new SoftwareKeys();
  private context: common.UIAbilityContext | null = null;
  private netConnection: connection.NetConnection | null = null;

  async start(context: common.UIAbilityContext): Promise<void> {
    if (this.core !== null) return;
    this.context = context;
    this.display.open(context);
    keyboardPrefs.open(context);
    try {
      this.keys = await HuksKeys.open();
    } catch (e) {
      hilog.warn(DOMAIN, 'muster', 'HUKS unavailable, software key: %{public}s', (e as Error).message);
      this.keys = new SoftwareKeys();
    }
    const config = new MusterConfig();
    config.dir = context.filesDir + '/muster';
    config.name = deviceInfo.marketName;
    config.version = appVersion();
    try {
      MusterCore.initLogging(3);
      this.core = new MusterCore(config, this.keys,
        (vm: ViewModel): void => { this.vm = vm; },
        (id: string, effect: Effect): void => { this.perform(id, effect); });
      this.started = true;
    } catch (e) {
      this.startError = (e as Error).message;
      return;
    }
    this.netConnection = connection.createNetConnection();
    this.netConnection.on('netAvailable', (): void => { this.core?.lifecycle(pb.app.Lifecycle.NETWORK_CHANGED); });
    this.netConnection.on('netLost', (): void => { this.core?.lifecycle(pb.app.Lifecycle.NETWORK_CHANGED); });
    this.netConnection.register((): void => {});
  }

  setWindow(w: window.Window): void { this.display.attach(w); }
  foreground(): void { this.display.refresh(); this.core?.lifecycle(pb.app.Lifecycle.FOREGROUND); }
  background(): void { this.core?.lifecycle(pb.app.Lifecycle.BACKGROUND); }

  clearWindow(): void { this.display.detach(); }

  stop(): void {
    this.clearWindow();
    this.netConnection?.unregister((): void => {});
    this.core?.close();
    this.core = null;
  }

  private async perform(id: string, effect: Effect): Promise<void> {
    const core = this.core;
    if (core === null) return;
    if (effect.scanQr) {
      try {
        const context = this.context;
        if (context === null) throw new Error('no ability context to scan with');
        const options: scanBarcode.ScanOptions = { scanTypes: [scanCore.ScanType.QR_CODE], enableMultiMode: false, enableAlbum: true };
        const result: scanBarcode.ScanResult = await scanBarcode.startScanForResult(context, options);
        core.answer(id, true, '', new Uint8Array(0), result.originalValue);
      } catch (e) {
        core.answer(id, false, (e as Error).message);
      }
    } else if (effect.authenticate) {
      this.authenticating = effect.authenticate.reason ?? '';
      try {
        const signature = await this.keys.presenceSign(effect.authenticate.message ?? new Uint8Array(0), this.authenticating);
        core.answer(id, true, '', signature);
      } catch (e) {
        core.answer(id, false, (e as Error).message);
      } finally {
        this.authenticating = '';
      }
    } else if (effect.notify) {
      const request: notificationManager.NotificationRequest = {
        id: Math.floor(Math.random() * 1000000),
        content: {
          notificationContentType: notificationManager.ContentType.NOTIFICATION_CONTENT_BASIC_TEXT,
          normal: { title: effect.notify.title ?? 'Muster', text: effect.notify.body ?? '' },
        },
      };
      notificationManager.publish(request).catch((): void => {});
      core.answer(id, true);
    } else if (effect.openUrl) {
      this.context?.openLink(effect.openUrl.url ?? '').catch((): void => {});
      core.answer(id, true);
    } else if (effect.activateNetwork) {
      // The core chose the endpoint and deadline; one connect after a
      // foreground return brings the OS network up. The answer is a retry hint.
      try {
        await activateNetwork(effect.activateNetwork.address ?? '', effect.activateNetwork.port ?? 0,
          effect.activateNetwork.timeoutMs ?? 3000);
        core.answer(id, true);
      } catch (e) {
        core.answer(id, false, (e as Error).message);
      }
    } else if (effect.localAuth) {
      try {
        await localAuthenticate(effect.localAuth.reason ?? '');
        core.answer(id, true);
      } catch (e) {
        core.answer(id, false, (e as Error).message);
      }
    } else if (effect.cover) {
      // A phone has no screen cover; the core only asks a desktop's own shell.
      core.answer(id, false, 'no cover on this platform');
    } else {
      core.answer(id, false, 'unknown effect');
    }
  }
}

/** This bundle's "0.1.0 (1)": version name and code, "" when unreadable. */
function appVersion(): string {
  try {
    const info = bundleManager.getBundleInfoForSelfSync(bundleManager.BundleFlag.GET_BUNDLE_INFO_DEFAULT);
    return `${info.versionName} (${info.versionCode})`;
  } catch (e) {
    hilog.warn(DOMAIN, 'muster', 'bundle version unavailable: %{public}s', (e as Error).message);
    return '';
  }
}

/**
 * Thin OS adapter, as the iOS shell's NetworkActivation: one TCP connect to
 * the endpoint the core chose, within its deadline, so the OS brings the
 * network up. No bytes are sent, and readiness is never a trust decision.
 */
async function activateNetwork(address: string, port: number, timeoutMs: number): Promise<void> {
  if (port <= 0 || port > 65535) throw new Error('Invalid activation port');
  const tcp = socket.constructTCPSocketInstance();
  try {
    const target: socket.NetAddress = { address: address, port: port, family: address.indexOf(':') >= 0 ? 2 : 1 };
    const options: socket.TCPConnectOptions = { address: target, timeout: timeoutMs };
    await tcp.connect(options);
  } finally {
    tcp.close().catch((): void => {});
  }
}

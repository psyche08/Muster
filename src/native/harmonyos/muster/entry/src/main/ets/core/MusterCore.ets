// The typed face of the core on HarmonyOS (ARCHITECTURE §3.7): intents in,
// view models and effects out, as the protobuf messages of app.proto.
import muster from 'libmuster.so';
import { util } from '@kit.ArkTS';
import Long from 'long';
import { muster as pb } from '../proto/muster';

export type ViewModel = pb.app.ViewModel;
export type Effect = pb.app.Effect;

/** A 64-bit field as a number: the generated models decode them as numbers
 *  or Longs, and Number() of a Long is NaN. */
export function uint64(value: number | Long | null | undefined): number {
  if (value === null || value === undefined) return 0;
  return typeof value === 'number' ? value : value.toNumber();
}

/** The JSON `muster_core_new` reads. */
export class MusterConfig {
  dir: string = '';
  mode: string = 'embedded';
  relay: string | null = null;
  name: string = '';
  kind: string = 'phone';
  bind: string = '0.0.0.0:0';
  allowLoopback: boolean = false;
  /** The app's release version the core reports (settings, heartbeat). */
  version: string = '';

  toJson(): Uint8Array {
    const json: Record<string, string | boolean> = {
      'dir': this.dir, 'mode': this.mode, 'name': this.name, 'kind': this.kind,
      'bind': this.bind, 'allow_loopback': this.allowLoopback,
    };
    if (this.relay !== null) json['relay'] = this.relay;
    if (this.version !== '') json['version'] = this.version;
    return new util.TextEncoder().encodeInto(JSON.stringify(json));
  }
}


/** Where the two keys come from; the core sees public points and signatures. */
export interface MusterKeys {
  devicePublicKey(): Uint8Array | null;
  /** The HUKS alias libmuster.so signs with, synchronously in native code; '' for none. */
  deviceKeyAlias(): string;
  presencePublicKey(): Uint8Array | null;
  /** Signs after local authentication; the reason is what the prompt says. */
  presenceSign(message: Uint8Array, reason: string): Promise<Uint8Array>;
}

/** No hardware keys: the core keeps a software key, no presence proofs. */
export class SoftwareKeys implements MusterKeys {
  devicePublicKey(): Uint8Array | null { return null; }
  deviceKeyAlias(): string { return ''; }
  presencePublicKey(): Uint8Array | null { return null; }
  presenceSign(message: Uint8Array, reason: string): Promise<Uint8Array> {
    return Promise.reject(new Error('no presence key on this device'));
  }
}

export type ViewListener = (vm: ViewModel) => void;
export type EffectListener = (id: string, effect: Effect) => void;

class DraftSource {
  id: string;
  read: () => pb.app.IChatDraft;
  constructor(id: string, read: () => pb.app.IChatDraft) { this.id = id; this.read = read; }
}

/**
 * A narrow input callback, owned by the visible composer: it reports its
 * buffer with every intent the shell sends, and Rust decides when to keep
 * it (ComposerAction). Nothing runs on typing.
 */
export class ComposerDraftSource {
  private source: DraftSource | null = null;

  register(read: () => pb.app.IChatDraft): string {
    const id = util.generateRandomUUID();
    this.source = new DraftSource(id, read);
    return id;
  }

  unregister(id: string): void {
    if (this.source !== null && this.source.id === id) this.source = null;
  }

  /** The intent as sent: wrapped with the draft while a composer is on screen. */
  attaching(intent: pb.app.IIntent): pb.app.IIntent {
    const source = this.source;
    if (source === null) return intent;
    return { composerAction: { draft: source.read(), action: intent } };
  }
}

export class MusterCore {
  private handle: number = 0;
  readonly keys: MusterKeys;
  readonly composerDraft: ComposerDraftSource = new ComposerDraftSource();

  constructor(config: MusterConfig, keys: MusterKeys, onView: ViewListener, onEffect: EffectListener) {
    this.keys = keys;
    this.handle = muster.create(
      config.toJson(),
      (bytes: Uint8Array): void => {
        try { onView(pb.app.ViewModel.decode(bytes)); } catch (e) { /* a bad frame is dropped */ }
      },
      (id: string, bytes: Uint8Array): void => {
        let effect: Effect | null = null;
        try { effect = pb.app.Effect.decode(bytes); } catch (e) { effect = null; }
        if (effect === null) { this.answer(id, false, 'undecodable effect'); return; }
        onEffect(id, effect);
      },
      keys.devicePublicKey(),
      keys.presencePublicKey(),
      keys.deviceKeyAlias(),
    );
  }

  static version(): string { return muster.version(); }
  static initLogging(level: number): void { muster.logInit(level); }

  close(): void {
    const h = this.handle;
    this.handle = 0;
    if (h !== 0) muster.destroy(h);
  }

  /** Sends one intent; while a composer is on screen its draft rides along. */
  send(it: pb.app.IIntent): void {
    if (this.handle === 0) return;
    const intent = pb.app.Intent.create(this.composerDraft.attaching(it));
    muster.intent(this.handle, pb.app.Intent.encode(intent).finish());
  }

  lifecycle(event: pb.app.Lifecycle): void {
    if (this.handle !== 0) muster.lifecycle(this.handle, event);
  }

  answer(id: string, ok: boolean, error: string = '', data: Uint8Array = new Uint8Array(0), text: string = ''): void {
    if (this.handle === 0) return;
    const result = pb.app.EffectResult.create({ ok: ok, error: error, data: data, text: text });
    muster.effectResult(this.handle, id, pb.app.EffectResult.encode(result).finish());
  }

  sshPublicKeyLine(): string | null {
    return this.handle !== 0 ? muster.sshPublicKey(this.handle) : null;
  }

  // Convenience intents, one per app.proto message.
  refresh(): void { this.send({ refresh: {} }); }
  back(): void { this.send({ navigate: { back: true } }); }
  navigate(screen: pb.app.Screen): void { this.send({ navigate: { screen: screen } }); }
  dismiss(toastId: string): void { this.send({ dismiss: { toastId: toastId } }); }
  createNetwork(name: string, cidr: string = ''): void { this.send({ createNetwork: { name: name, vnetCidr: cidr } }); }
  showInvite(ttlSecs: number = 0): void { this.send({ showInvite: { ttlSecs: ttlSecs } }); }
  join(invite: string, deviceName: string, kind: string): void {
    this.send({ joinNetwork: { inviteText: invite, deviceName: deviceName, deviceKind: kind } });
  }
  decideJoin(device: string, approve: boolean, scopes: string[] = [], reason: string = ''): void {
    this.send({ decideJoin: { deviceId: device, approve: approve, scopes: scopes, reason: reason } });
  }
  removeDevice(device: string): void { this.send({ removeDevice: { deviceId: device } }); }
  setScopes(device: string, scopes: string[], role: string): void {
    this.send({ setScopes: { deviceId: device, scopes: scopes, role: role } });
  }
  setRelay(relay: string): void { this.send({ setRelay: { relay: relay } }); }
  /** Covers (or uncovers) a desktop's screen; device "" for this one. The core checks who may. */
  setDeviceLockedUse(on: boolean, device: string): void { this.send({ setDeviceLockedUse: { on: on, deviceId: device } }); }
  setCover(on: boolean, device: string = ''): void { this.send({ setCover: { on: on, deviceId: device } }); }
  setPolicy(policy: pb.v1.IPolicy): void { this.send({ setPolicy: { policy: policy } }); }
  selectDevice(device: string): void { this.send({ selectDevice: { deviceId: device } }); }
  /** device "*" for every device, "" for the current target. */
  openAgents(device: string = ''): void { this.send({ openAgents: { deviceId: device } }); }
  openAgent(session: string, device: string = ''): void { this.send({ openAgent: { session: session, deviceId: device } }); }
  /** The Chat tab: provider "claude" (the Claude app) or "codex" (the ChatGPT
   *  app), "" for the last one. The core picks the Mac. */
  openDesktopChats(provider: string = ''): void { this.send({ openDesktopChats: { provider: provider } }); }
  /** A listed conversation by id, or a new one (""); onDesktop shows a new
   *  one on the Mac now instead of with its first message. */
  desktopChatOpen(chat: string, onDesktop: boolean = false): void {
    this.send({ desktopChatOpen: { chat: chat, desktop: onDesktop } });
  }
  /** The open desktop chat's model / effort choices, read from the Mac. */
  chatOptionsLoad(): void { this.send({ chatOptionsLoad: {} }); }
  /** Chooses a model and/or effort by name exactly as the Mac offered it. */
  chatSetOptions(model: string = '', effort: string = ''): void {
    this.send({ chatSetOptions: { model: model, effort: effort } });
  }
  /** `releaseUnconfirmed`: the token of `ChatView.unconfirmed`, once the user
   *  has seen that the earlier message is not on the Mac. */
  agentSend(text: string, attachments: pb.v1.IAgentAttachment[] = [], draftId: string = '',
    releaseUnconfirmed: string = ''): void {
    this.send({ agentSend: { text: text, attachments: attachments, draftId: draftId,
      releaseUnconfirmed: releaseUnconfirmed } });
  }
  agentRespond(interaction: string, option: string, answer: string = '', answers: pb.v1.IQuestionAnswer[] = []): void {
    this.send({ agentRespond: { interaction: interaction, option: option, answer: answer, answers: answers } });
  }
  agentInterrupt(): void { this.send({ agentInterrupt: {} }); }
  cancelLockedUse(): void { this.send({ cancelLockedUse: {} }); }
  agentLoadEarlier(all: boolean = false): void { this.send({ agentLoadEarlier: { all: all } }); }
  openTerminals(device: string = ''): void { this.send({ openTerminals: { deviceId: device } }); }
  openTerminal(session: string, device: string = ''): void { this.send({ openTerminal: { session: session, deviceId: device } }); }
  terminalKey(key: string): void { this.send({ terminalKey: { key: key } }); }
  terminalText(text: string): void { this.send({ terminalText: { text: text } }); }
  codeAction(action: pb.app.CodeAction.Action, id: string = ''): void { this.send({ codeAction: { action: action, id: id, autoReturn: true } }); }
  sessionTerminal(action: pb.app.SessionTerminalAction.Action): void { this.send({ sessionTerminalAction: { action: action } }); }
  terminalCreate(profile: string = ''): void { this.send({ terminalCreate: { profile: profile } }); }
  terminalResize(cols: number, rows: number): void { this.send({ terminalResize: { cols: cols, rows: rows } }); }
  openSsh(port: number, user: string, cols: number, rows: number): void {
    this.send({ openSsh: { port: port, user: user, cols: cols, rows: rows } });
  }
  closeTerminal(): void { this.send({ closeTerminal: {} }); }
  /** Clipboard text for the terminal; the core types it while `TerminalView.canPaste` holds. */
  terminalPaste(text: string): void { this.send({ terminalPaste: { text: text } }); }
  openFiles(path: string = ''): void { this.send({ openFiles: { path: path } }); }
  openFile(path: string): void { this.send({ openFile: { path: path } }); }
  openFileLink(url: string): void { this.send({ openFileLink: { url: url } }); }
  openChange(summary: string, path: string): void { this.send({ openChange: { summaryId: summary, path: path } }); }
  openGit(diffCommit: string = ''): void { this.send({ openGit: { diffCommit: diffCommit } }); }
  /** The Chrome extension on a device ("" for the current target); `draft` prefills a request. */
  openBrowser(device: string = '', draft: string = ''): void { this.send({ openBrowser: { device: device, draft: draft } }); }
  /** Checks the extension's connection; `install` sets up the native host on this device. */
  browserSetup(install: boolean = false): void { this.send({ browserSetup: { install: install } }); }
  browserRun(json: string): void { this.send({ browserRun: { json: json } }); }
  /** kind 1 APNs, 2 FCM, 3 HarmonyOS. */
  setPushToken(kind: number, token: Uint8Array): void { this.send({ setPushToken: { kind: kind, token: token } }); }
}

/** The key names muster-terminal maps to bytes. `ctrl-`, `alt-` and `shift-`
 *  prefixes combine with any of them or with one character; the core decides
 *  what they send, so the shell passes every such name through. */
export class TerminalKeys {
  static readonly ENTER = 'enter';
  static readonly TAB = 'tab';
  static readonly BACKSPACE = 'backspace';
  static readonly ESCAPE = 'escape';
  static readonly UP = 'up';
  static readonly DOWN = 'down';
  static readonly RIGHT = 'right';
  static readonly LEFT = 'left';
  static readonly HOME = 'home';
  static readonly END = 'end';
  static readonly PAGE_UP = 'pageup';
  static readonly PAGE_DOWN = 'pagedown';
  static readonly DELETE = 'delete';
}

/** One run of equally styled cells (muster-terminal's 11-byte encoding). */
export class StyleRun {
  length: number = 0;
  fg: number = 0;
  bg: number = 0;
  flags: number = 0;

  get bold(): boolean { return (this.flags & 1) !== 0; }
  get italic(): boolean { return (this.flags & 2) !== 0; }
  get underline(): boolean { return (this.flags & 4) !== 0; }
  get inverse(): boolean { return (this.flags & 8) !== 0; }
  get dim(): boolean { return (this.flags & 16) !== 0; }
  get strike(): boolean { return (this.flags & 32) !== 0; }

  static decode(bytes: Uint8Array): StyleRun[] {
    const runs: StyleRun[] = [];
    for (let i = 0; i + 11 <= bytes.length; i += 11) {
      const run = new StyleRun();
      run.length = (bytes[i] << 8) | bytes[i + 1];
      run.fg = ((bytes[i + 2] << 24) | (bytes[i + 3] << 16) | (bytes[i + 4] << 8) | bytes[i + 5]) >>> 0;
      run.bg = ((bytes[i + 6] << 24) | (bytes[i + 7] << 16) | (bytes[i + 8] << 8) | bytes[i + 9]) >>> 0;
      run.flags = bytes[i + 10];
      runs.push(run);
    }
    return runs;
  }

  /** A CSS colour for 0x01RRGGBB / 0x02000000|index, or null for default. */
  static css(color: number, palette: string[]): string | null {
    const kind = color >>> 24;
    if (kind === 1) return '#' + (color & 0xffffff).toString(16).padStart(6, '0');
    if (kind === 2) return palette[color & 0xff] ?? null;
    return null;
  }
}

/** The 256-colour palette as CSS strings. */
export function palette256(): string[] {
  const basic: string[] = ['#000000', '#cc3333', '#33b333', '#ccb333', '#4d66e6', '#b34db3', '#33b3b3', '#d9d9d9',
    '#666666', '#ff6666', '#66e666', '#ffe666', '#8099ff', '#e680e6', '#66e6e6', '#ffffff'];
  const out: string[] = [];
  const steps: number[] = [0, 95, 135, 175, 215, 255];
  for (let i = 0; i < 256; i++) {
    if (i < 16) { out.push(basic[i]); continue; }
    if (i < 232) {
      const n = i - 16;
      const r = steps[Math.floor(n / 36)], g = steps[Math.floor(n / 6) % 6], b = steps[n % 6];
      out.push(`#${r.toString(16).padStart(2, '0')}${g.toString(16).padStart(2, '0')}${b.toString(16).padStart(2, '0')}`);
      continue;
    }
    const gray = (8 + (i - 232) * 10).toString(16).padStart(2, '0');
    out.push(`#${gray}${gray}${gray}`);
  }
  return out;
}

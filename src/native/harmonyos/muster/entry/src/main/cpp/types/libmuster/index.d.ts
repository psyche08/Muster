// libmuster.so as ArkTS sees it: the C ABI of muster.h, bytes in and out.
// Effect ids are decimal strings (a u64 does not fit a JS number). The
// public points are 65-byte X9.63 (null: no hardware key); the native side
// signs with the HUKS key named by deviceKeyAlias ('' for none).
export const create: (
  configJson: Uint8Array,
  onView: (bytes: Uint8Array) => void,
  onEffect: (id: string, bytes: Uint8Array) => void,
  devicePublicKey: Uint8Array | null,
  presencePublicKey: Uint8Array | null,
  deviceKeyAlias: string,
) => number;
export const intent: (handle: number, intent: Uint8Array) => void;
export const effectResult: (handle: number, id: string, result: Uint8Array) => void;
export const lifecycle: (handle: number, event: number) => void;
export const sshPublicKey: (handle: number) => string | null;
export const destroy: (handle: number) => void;
export const logInit: (level: number) => void;
export const version: () => string;

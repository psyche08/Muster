// The device key and the presence key in HUKS (ARCHITECTURE §3.4). The
// device key signs without the user; the presence key is bound to local
// authentication, so every signature is one authentication. Public points
// leave as the 65-byte X9.63 form the roster carries; signatures as DER.
// Device signatures are made by libmuster.so through HUKS's synchronous C
// API (muster_napi.cpp), which must use this alias and signProperties().
import { huks } from '@kit.UniversalKeystoreKit';
import { userAuth } from '@kit.UserAuthenticationKit';
import { cryptoFramework } from '@kit.CryptoArchitectureKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { MusterKeys } from './MusterCore';

const DEVICE_ALIAS = 'muster-device-key';
const PRESENCE_ALIAS = 'muster-presence-key';

function storage(): huks.HuksParam {
  return { tag: huks.HuksTag.HUKS_TAG_AUTH_STORAGE_LEVEL, value: huks.HuksAuthStorageLevel.HUKS_AUTH_STORAGE_LEVEL_ECE };
}

function keyProperties(userAuthBound: boolean): huks.HuksParam[] {
  const props: huks.HuksParam[] = [
    { tag: huks.HuksTag.HUKS_TAG_ALGORITHM, value: huks.HuksKeyAlg.HUKS_ALG_ECC },
    { tag: huks.HuksTag.HUKS_TAG_KEY_SIZE, value: huks.HuksKeySize.HUKS_ECC_KEY_SIZE_256 },
    { tag: huks.HuksTag.HUKS_TAG_PURPOSE, value: huks.HuksKeyPurpose.HUKS_KEY_PURPOSE_SIGN | huks.HuksKeyPurpose.HUKS_KEY_PURPOSE_VERIFY },
    { tag: huks.HuksTag.HUKS_TAG_DIGEST, value: huks.HuksKeyDigest.HUKS_DIGEST_SHA256 },
    storage(),
  ];
  if (userAuthBound) {
    props.push({ tag: huks.HuksTag.HUKS_TAG_USER_AUTH_TYPE,
      value: huks.HuksUserAuthType.HUKS_USER_AUTH_TYPE_FINGERPRINT | huks.HuksUserAuthType.HUKS_USER_AUTH_TYPE_FACE | huks.HuksUserAuthType.HUKS_USER_AUTH_TYPE_PIN });
    props.push({ tag: huks.HuksTag.HUKS_TAG_KEY_AUTH_ACCESS_TYPE, value: huks.HuksAuthAccessType.HUKS_AUTH_ACCESS_INVALID_NEW_BIO_ENROLL });
    props.push({ tag: huks.HuksTag.HUKS_TAG_CHALLENGE_TYPE, value: huks.HuksChallengeType.HUKS_CHALLENGE_TYPE_NORMAL });
  }
  return props;
}

function signProperties(): huks.HuksParam[] {
  return [
    { tag: huks.HuksTag.HUKS_TAG_ALGORITHM, value: huks.HuksKeyAlg.HUKS_ALG_ECC },
    { tag: huks.HuksTag.HUKS_TAG_KEY_SIZE, value: huks.HuksKeySize.HUKS_ECC_KEY_SIZE_256 },
    { tag: huks.HuksTag.HUKS_TAG_PURPOSE, value: huks.HuksKeyPurpose.HUKS_KEY_PURPOSE_SIGN },
    { tag: huks.HuksTag.HUKS_TAG_DIGEST, value: huks.HuksKeyDigest.HUKS_DIGEST_SHA256 },
    storage(),
  ];
}

/** The last 65 bytes of a P-256 SubjectPublicKeyInfo are the uncompressed point. */
function x963(spki: Uint8Array): Uint8Array | null {
  if (spki.length < 65 || spki[spki.length - 65] !== 0x04) return null;
  return spki.slice(spki.length - 65);
}

export class HuksKeys implements MusterKeys {
  private devicePoint: Uint8Array | null = null;
  private presencePoint: Uint8Array | null = null;

  /** Loads or creates both keys. Without a lock-screen credential HUKS
   *  cannot create the presence key; the device key still works and
   *  presence proofs are refused. */
  static async open(): Promise<HuksKeys> {
    const keys = new HuksKeys();
    keys.devicePoint = await HuksKeys.ensure(DEVICE_ALIAS, false);
    try {
      keys.presencePoint = await HuksKeys.ensure(PRESENCE_ALIAS, true);
    } catch (e) {
      hilog.warn(0x4841, 'muster', 'no presence key on this device: %{public}s', (e as Error).message);
      keys.presencePoint = null;
    }
    return keys;
  }

  private static async ensure(alias: string, userAuthBound: boolean): Promise<Uint8Array | null> {
    const options: huks.HuksOptions = { properties: [storage()] };
    if (!(await huks.hasKeyItem(alias, options))) {
      await huks.generateKeyItem(alias, { properties: keyProperties(userAuthBound) });
    }
    const exported = await huks.exportKeyItem(alias, options);
    return exported.outData ? x963(exported.outData) : null;
  }

  devicePublicKey(): Uint8Array | null { return this.devicePoint; }
  deviceKeyAlias(): string { return this.devicePoint === null ? '' : DEVICE_ALIAS; }
  presencePublicKey(): Uint8Array | null { return this.presencePoint; }

  async presenceSign(message: Uint8Array, reason: string): Promise<Uint8Array> {
    if (this.presencePoint === null) throw new Error('no presence key: set a lock-screen password first');
    const options: huks.HuksOptions = { properties: signProperties() };
    const session = await huks.initSession(PRESENCE_ALIAS, options);
    const challenge = session.challenge ?? new Uint8Array(0);
    try {
      const token = await localAuthenticate(reason, challenge);
      const finishOptions: huks.HuksOptions = { properties: signProperties(), inData: message };
      finishOptions.properties!.push({ tag: huks.HuksTag.HUKS_TAG_AUTH_TOKEN, value: token });
      const finished = await huks.finishSession(session.handle, finishOptions);
      if (!finished.outData) throw new Error('no signature');
      return finished.outData;
    } catch (e) {
      await huks.abortSession(session.handle, options).catch((): void => {});
      throw e as Error;
    }
  }
}

/**
 * Local authentication (fingerprint, face or PIN) with the system prompt.
 * Over HUKS's challenge the token unlocks a key bound to it once; the
 * LocalAuth effect passes a fresh random challenge and needs only the answer.
 */
export function localAuthenticate(reason: string, challenge: Uint8Array = randomBytes(32)): Promise<Uint8Array> {
  return new Promise<Uint8Array>((resolve: (token: Uint8Array) => void, reject: (error: Error) => void): void => {
    const params: userAuth.AuthParam = {
      challenge: challenge,
      authType: [userAuth.UserAuthType.FINGERPRINT, userAuth.UserAuthType.FACE, userAuth.UserAuthType.PIN],
      authTrustLevel: userAuth.AuthTrustLevel.ATL3,
    };
    const widget: userAuth.WidgetParam = { title: reason };
    const instance = userAuth.getUserAuthInstance(params, widget);
    instance.on('result', {
      onResult(result: userAuth.UserAuthResult): void {
        if (result.result === userAuth.UserAuthResultCode.SUCCESS && result.token) resolve(result.token);
        else reject(new Error(`authentication failed (${result.result})`));
      },
    });
    instance.start();
  });
}

/** Random bytes for tests and challenges. */
export function randomBytes(n: number): Uint8Array {
  return cryptoFramework.createRandom().generateRandomSync(n).data;
}

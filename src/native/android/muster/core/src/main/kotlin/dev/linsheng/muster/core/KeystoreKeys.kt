package dev.linsheng.muster.core

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.PublicKey
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec

/**
 * The device key and the presence key in the Android Keystore (StrongBox
 * when the device has one). The device key signs without the user; the
 * presence key needs authentication for every signature, which is what a
 * presence proof means. Neither leaves the hardware.
 */
class KeystoreKeys private constructor(
    private val device: KeyStore.PrivateKeyEntry,
    private val presence: KeyStore.PrivateKeyEntry,
) : MusterKeys {
    override val devicePublicKey: ByteArray? = x963(device.certificate.publicKey)
    override val presencePublicKey: ByteArray? = x963(presence.certificate.publicKey)

    override fun deviceSign(message: ByteArray): ByteArray? = runCatching {
        sign(device.privateKey, message)
    }.getOrNull()

    /** The presence signature; `presenceSigner` is what BiometricPrompt authorizes. */
    override suspend fun presenceSign(message: ByteArray, reason: String): ByteArray {
        val authorize = presenceAuthorizer ?: throw IllegalStateException("no authentication UI installed")
        val signature = presenceSigner()
        val authorized = authorize(signature, reason)
        authorized.update(message)
        return authorized.sign()
    }

    /** A Signature initialised with the presence key, for BiometricPrompt.CryptoObject. */
    fun presenceSigner(): Signature =
        Signature.getInstance("SHA256withECDSA").apply { initSign(presence.privateKey) }

    /** Installed by the app: shows the biometric prompt for `signature` and returns it authorized. */
    var presenceAuthorizer: (suspend (Signature, String) -> Signature)? = null

    companion object {
        private const val STORE = "AndroidKeyStore"
        private const val DEVICE_ALIAS = "muster-device-key"
        private const val PRESENCE_ALIAS = "muster-presence-key"

        /** Loads or creates both keys. */
        fun open(): KeystoreKeys {
            val store = KeyStore.getInstance(STORE).apply { load(null) }
            val device = entry(store, DEVICE_ALIAS) ?: run {
                generate(DEVICE_ALIAS, userAuthentication = false); entry(store, DEVICE_ALIAS)!!
            }
            val presence = entry(store, PRESENCE_ALIAS) ?: run {
                generate(PRESENCE_ALIAS, userAuthentication = true); entry(store, PRESENCE_ALIAS)!!
            }
            return KeystoreKeys(device, presence)
        }

        private fun entry(store: KeyStore, alias: String): KeyStore.PrivateKeyEntry? =
            store.getEntry(alias, null) as? KeyStore.PrivateKeyEntry

        private fun generate(alias: String, userAuthentication: Boolean) {
            val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
                .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                .setDigests(KeyProperties.DIGEST_SHA256)
                .apply {
                    if (userAuthentication) {
                        setUserAuthenticationRequired(true)
                        // Every signature is one authentication; that is the proof.
                        setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                        setInvalidatedByBiometricEnrollment(true)
                    }
                }
            for (strongBox in listOf(true, false)) {
                try {
                    val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, STORE)
                    generator.initialize(spec.setIsStrongBoxBacked(strongBox).build())
                    generator.generateKeyPair()
                    return
                } catch (_: StrongBoxUnavailableException) {
                    // Fall back to the TEE-backed keystore.
                }
            }
        }

        private fun sign(key: PrivateKey, message: ByteArray): ByteArray =
            Signature.getInstance("SHA256withECDSA").run {
                initSign(key)
                update(message)
                sign()
            }

        /** The uncompressed X9.63 point the roster carries. */
        private fun x963(key: PublicKey): ByteArray? {
            val ec = key as? ECPublicKey ?: return null
            val x = ec.w.affineX.toByteArray().dropWhile { it == 0.toByte() }.toByteArray()
            val y = ec.w.affineY.toByteArray().dropWhile { it == 0.toByte() }.toByteArray()
            val out = ByteArray(65)
            out[0] = 0x04
            System.arraycopy(x, 0, out, 33 - x.size, x.size)
            System.arraycopy(y, 0, out, 65 - y.size, y.size)
            return out
        }
    }
}

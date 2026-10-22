package dev.linsheng.muster.core

/** The C ABI (muster.h) through libmuster_jni.so. Bytes are protobuf messages. */
object MusterNative {
    init {
        System.loadLibrary("muster_jni")
    }

    /** Callbacks the core makes from its own threads. */
    interface Callbacks {
        fun onView(bytes: ByteArray)
        fun onEffect(id: Long, bytes: ByteArray)
        /** The 65-byte X9.63 point of the device key, or null for a software key. */
        fun hardwarePublicKey(): ByteArray?
        /** ECDSA-P256-SHA256 over the message with the device key, DER. */
        fun hardwareSign(message: ByteArray): ByteArray?
        /** The presence key's 65-byte point, or null. */
        fun presencePublicKey(): ByteArray?
    }

    @JvmStatic external fun create(configJson: ByteArray, callbacks: Callbacks): Long
    @JvmStatic external fun intent(handle: Long, intent: ByteArray)
    @JvmStatic external fun effectResult(handle: Long, id: Long, result: ByteArray)
    @JvmStatic external fun lifecycle(handle: Long, event: Int)
    @JvmStatic external fun sshPublicKey(handle: Long): String?
    @JvmStatic external fun destroy(handle: Long)
    @JvmStatic external fun logInit(level: Int)
    @JvmStatic external fun version(): String
}

// The JNI shell around muster.h (ARCHITECTURE §3.7): bytes in, bytes out.
// Callbacks from the core's threads attach to the JVM and call the Kotlin
// Callbacks object; nothing is interpreted here.
#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "muster.h"

typedef struct Shell {
    JavaVM *vm;
    jobject callbacks;  // global ref to dev.linsheng.muster.core.MusterNative$Callbacks
    jmethodID on_view;
    jmethodID on_effect;
    jmethodID hw_public_key;
    jmethodID hw_sign;
    jmethodID presence_public_key;
    MusterCore *core;
    MusterPlatform platform;
} Shell;

/* Attaches the calling thread when it is not a Java thread. */
static JNIEnv *env_for(Shell *shell, int *attached) {
    JNIEnv *env = NULL;
    *attached = 0;
    jint status = (*shell->vm)->GetEnv(shell->vm, (void **)&env, JNI_VERSION_1_6);
    if (status == JNI_EDETACHED) {
        if ((*shell->vm)->AttachCurrentThread(shell->vm, &env, NULL) != JNI_OK) return NULL;
        *attached = 1;
    } else if (status != JNI_OK) {
        return NULL;
    }
    return env;
}

static void env_done(Shell *shell, int attached) {
    if (attached) (*shell->vm)->DetachCurrentThread(shell->vm);
}

static jbyteArray to_java(JNIEnv *env, const uint8_t *bytes, size_t len) {
    jbyteArray array = (*env)->NewByteArray(env, (jsize)len);
    if (array && len > 0) (*env)->SetByteArrayRegion(env, array, 0, (jsize)len, (const jbyte *)bytes);
    return array;
}

/* Copies a byte[] result into `out`; -1 when null or too large. */
static intptr_t from_java(JNIEnv *env, jbyteArray array, uint8_t *out, size_t cap) {
    if (!array) return -1;
    jsize len = (*env)->GetArrayLength(env, array);
    if (len < 0 || (size_t)len > cap) return -1;
    (*env)->GetByteArrayRegion(env, array, 0, len, (jbyte *)out);
    return len;
}

static void on_view(void *ctx, const uint8_t *bytes, size_t len) {
    Shell *shell = ctx;
    int attached;
    JNIEnv *env = env_for(shell, &attached);
    if (!env) return;
    jbyteArray array = to_java(env, bytes, len);
    if (array) {
        (*env)->CallVoidMethod(env, shell->callbacks, shell->on_view, array);
        (*env)->DeleteLocalRef(env, array);
    }
    if ((*env)->ExceptionCheck(env)) (*env)->ExceptionClear(env);
    env_done(shell, attached);
}

static void on_effect(void *ctx, uint64_t id, const uint8_t *bytes, size_t len) {
    Shell *shell = ctx;
    int attached;
    JNIEnv *env = env_for(shell, &attached);
    if (!env) return;
    jbyteArray array = to_java(env, bytes, len);
    if (array) {
        (*env)->CallVoidMethod(env, shell->callbacks, shell->on_effect, (jlong)id, array);
        (*env)->DeleteLocalRef(env, array);
    }
    if ((*env)->ExceptionCheck(env)) (*env)->ExceptionClear(env);
    env_done(shell, attached);
}

static intptr_t key_callback(Shell *shell, jmethodID method, uint8_t *out, size_t cap) {
    int attached;
    JNIEnv *env = env_for(shell, &attached);
    if (!env) return -1;
    jbyteArray array = (jbyteArray)(*env)->CallObjectMethod(env, shell->callbacks, method);
    intptr_t n = -1;
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
    } else {
        n = from_java(env, array, out, cap);
    }
    if (array) (*env)->DeleteLocalRef(env, array);
    env_done(shell, attached);
    return n;
}

static intptr_t hw_public_key(void *ctx, uint8_t *out, size_t cap) {
    Shell *shell = ctx;
    return key_callback(shell, shell->hw_public_key, out, cap);
}

static intptr_t presence_public_key(void *ctx, uint8_t *out, size_t cap) {
    Shell *shell = ctx;
    return key_callback(shell, shell->presence_public_key, out, cap);
}

static intptr_t hw_sign(void *ctx, const uint8_t *msg, size_t msg_len, uint8_t *out, size_t cap) {
    Shell *shell = ctx;
    int attached;
    JNIEnv *env = env_for(shell, &attached);
    if (!env) return -1;
    intptr_t n = -1;
    jbyteArray message = to_java(env, msg, msg_len);
    if (message) {
        jbyteArray signature = (jbyteArray)(*env)->CallObjectMethod(env, shell->callbacks, shell->hw_sign, message);
        if ((*env)->ExceptionCheck(env)) {
            (*env)->ExceptionClear(env);
        } else {
            n = from_java(env, signature, out, cap);
        }
        if (signature) (*env)->DeleteLocalRef(env, signature);
        (*env)->DeleteLocalRef(env, message);
    }
    env_done(shell, attached);
    return n;
}

static void throw_illegal_state(JNIEnv *env, const char *message) {
    jclass cls = (*env)->FindClass(env, "java/lang/IllegalStateException");
    if (cls) (*env)->ThrowNew(env, cls, message);
}

JNIEXPORT jlong JNICALL
Java_dev_linsheng_muster_core_MusterNative_create(JNIEnv *env, jclass cls, jbyteArray config, jobject callbacks) {
    (void)cls;
    Shell *shell = calloc(1, sizeof(Shell));
    if (!shell) {
        throw_illegal_state(env, "out of memory");
        return 0;
    }
    (*env)->GetJavaVM(env, &shell->vm);
    shell->callbacks = (*env)->NewGlobalRef(env, callbacks);
    jclass cb = (*env)->GetObjectClass(env, callbacks);
    shell->on_view = (*env)->GetMethodID(env, cb, "onView", "([B)V");
    shell->on_effect = (*env)->GetMethodID(env, cb, "onEffect", "(J[B)V");
    shell->hw_public_key = (*env)->GetMethodID(env, cb, "hardwarePublicKey", "()[B");
    shell->hw_sign = (*env)->GetMethodID(env, cb, "hardwareSign", "([B)[B");
    shell->presence_public_key = (*env)->GetMethodID(env, cb, "presencePublicKey", "()[B");
    if (!shell->on_view || !shell->on_effect || !shell->hw_public_key || !shell->hw_sign || !shell->presence_public_key) {
        (*env)->DeleteGlobalRef(env, shell->callbacks);
        free(shell);
        throw_illegal_state(env, "Callbacks is missing a method");
        return 0;
    }
    shell->platform.ctx = shell;
    shell->platform.on_view = on_view;
    shell->platform.on_effect = on_effect;
    shell->platform.hw_public_key = hw_public_key;
    shell->platform.hw_sign = hw_sign;
    shell->platform.presence_public_key = presence_public_key;

    jsize len = (*env)->GetArrayLength(env, config);
    jbyte *bytes = (*env)->GetByteArrayElements(env, config, NULL);
    char *error = NULL;
    shell->core = muster_core_new((const uint8_t *)bytes, (size_t)len, &shell->platform, &error);
    (*env)->ReleaseByteArrayElements(env, config, bytes, JNI_ABORT);
    if (!shell->core) {
        throw_illegal_state(env, error ? error : "core refused to start");
        muster_string_free(error);
        (*env)->DeleteGlobalRef(env, shell->callbacks);
        free(shell);
        return 0;
    }
    return (jlong)(intptr_t)shell;
}

JNIEXPORT void JNICALL
Java_dev_linsheng_muster_core_MusterNative_intent(JNIEnv *env, jclass cls, jlong handle, jbyteArray intent) {
    (void)cls;
    Shell *shell = (Shell *)(intptr_t)handle;
    if (!shell || !shell->core || !intent) return;
    jsize len = (*env)->GetArrayLength(env, intent);
    jbyte *bytes = (*env)->GetByteArrayElements(env, intent, NULL);
    muster_core_intent(shell->core, (const uint8_t *)bytes, (size_t)len);
    (*env)->ReleaseByteArrayElements(env, intent, bytes, JNI_ABORT);
}

JNIEXPORT void JNICALL
Java_dev_linsheng_muster_core_MusterNative_effectResult(JNIEnv *env, jclass cls, jlong handle, jlong id, jbyteArray result) {
    (void)cls;
    Shell *shell = (Shell *)(intptr_t)handle;
    if (!shell || !shell->core || !result) return;
    jsize len = (*env)->GetArrayLength(env, result);
    jbyte *bytes = (*env)->GetByteArrayElements(env, result, NULL);
    muster_core_effect_result(shell->core, (uint64_t)id, (const uint8_t *)bytes, (size_t)len);
    (*env)->ReleaseByteArrayElements(env, result, bytes, JNI_ABORT);
}

JNIEXPORT void JNICALL
Java_dev_linsheng_muster_core_MusterNative_lifecycle(JNIEnv *env, jclass cls, jlong handle, jint event) {
    (void)env; (void)cls;
    Shell *shell = (Shell *)(intptr_t)handle;
    if (!shell || !shell->core) return;
    muster_core_lifecycle(shell->core, (uint32_t)event);
}

JNIEXPORT jstring JNICALL
Java_dev_linsheng_muster_core_MusterNative_sshPublicKey(JNIEnv *env, jclass cls, jlong handle) {
    (void)cls;
    Shell *shell = (Shell *)(intptr_t)handle;
    if (!shell || !shell->core) return NULL;
    char *line = muster_core_ssh_public_key(shell->core);
    if (!line) return NULL;
    jstring result = (*env)->NewStringUTF(env, line);
    muster_string_free(line);
    return result;
}

JNIEXPORT void JNICALL
Java_dev_linsheng_muster_core_MusterNative_destroy(JNIEnv *env, jclass cls, jlong handle) {
    (void)cls;
    Shell *shell = (Shell *)(intptr_t)handle;
    if (!shell) return;
    if (shell->core) muster_core_free(shell->core);
    shell->core = NULL;
    (*env)->DeleteGlobalRef(env, shell->callbacks);
    free(shell);
}

JNIEXPORT void JNICALL
Java_dev_linsheng_muster_core_MusterNative_logInit(JNIEnv *env, jclass cls, jint level) {
    (void)env; (void)cls;
    muster_log_init((uint32_t)level);
}

JNIEXPORT jstring JNICALL
Java_dev_linsheng_muster_core_MusterNative_version(JNIEnv *env, jclass cls) {
    (void)cls;
    return (*env)->NewStringUTF(env, muster_version());
}

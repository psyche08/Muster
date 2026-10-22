/* muster-ffi: the whole ABI between an app shell and the Rust core.
 * Messages are protobuf (muster.app.Intent / Effect / EffectResult /
 * ViewModel from app.proto). Callbacks may run on any thread. */
#ifndef MUSTER_FFI_H
#define MUSTER_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MusterCore MusterCore;

typedef struct MusterPlatform {
    void *ctx;
    /* A new ViewModel to render. */
    void (*on_view)(void *ctx, const uint8_t *view_model, size_t len);
    /* An Effect to perform; answer with muster_core_effect_result(id, ...). */
    void (*on_effect)(void *ctx, uint64_t id, const uint8_t *effect, size_t len);
    /* The device key: write the 65-byte X9.63 point; return its length, or
     * < 0 when the platform has none (a software key is used instead). */
    intptr_t (*hw_public_key)(void *ctx, uint8_t *out, size_t cap);
    /* ECDSA-P256-SHA256 over msg with the device key, DER encoded into out;
     * return the length, or < 0 when refused. Synchronous. */
    intptr_t (*hw_sign)(void *ctx, const uint8_t *msg, size_t msg_len, uint8_t *out, size_t cap);
    /* The presence key's public point (65 bytes), or < 0 when none. Its
     * signatures are produced by the Authenticate effect. */
    intptr_t (*presence_public_key)(void *ctx, uint8_t *out, size_t cap);
} MusterPlatform;

/* Logging to stderr at `level` (0 = off, 1 = error, 2 = warn, 3 = info, 4 = debug). */
void muster_desktop_update_startup(void);
/* Synchronous worker-thread preparation; not callable from a Rust runtime.
 * managed: 0 custom socket, 1 bundled service. action: 0 register, 1 recover
 * stopped service; return 0 success, 1 approval required, other native error.
 * No callback after return. Result 0 ready, -1 error (free *error_out with
 * muster_string_free). socket is a NUL-terminated UTF-8 path. */
int32_t muster_desktop_prepare(const char *socket, uint8_t managed,
                             int32_t (*action)(uint32_t), char **error_out);
void muster_log_init(uint32_t level);

/* The crate version, static. */
const char *muster_version(void);

/* Creates and starts a core. `config_json`:
 *   {"dir": "/path/for/state",
 *    "mode": "embedded" | "socket",
 *    "relay": "host:port" (embedded, optional),
 *    "name": "My iPhone", "kind": "phone" | "desktop" | "server",
 *    "socket": "/path/musterd.sock" (socket mode),
 *    "bind": "0.0.0.0:0", "allow_loopback": false,
 *    "version": "1.0 (16)" (embedded, optional: the app's release version,
 *    shown in settings and sent in the link heartbeat)}
 * Returns NULL on failure with *error_out set (free with muster_string_free). */
MusterCore *muster_core_new(const uint8_t *config_json, size_t len, const MusterPlatform *platform, char **error_out);

/* An encoded muster.app.Intent. */
void muster_core_intent(MusterCore *core, const uint8_t *intent, size_t len);

/* The answer to an Effect: an encoded muster.app.EffectResult. */
void muster_core_effect_result(MusterCore *core, uint64_t effect_id, const uint8_t *result, size_t len);

/* muster.app.Lifecycle: 0 foreground, 1 background, 2 network changed, 3 terminating. */
void muster_core_lifecycle(MusterCore *core, uint32_t event);

/* This device's key as an OpenSSH public key line (for authorized_keys),
 * or NULL. Free with muster_string_free. */
char *muster_core_ssh_public_key(MusterCore *core);

/* Stops the core and frees it. */
void muster_core_free(MusterCore *core);

void muster_string_free(char *s);

#ifdef __cplusplus
}
#endif
#endif

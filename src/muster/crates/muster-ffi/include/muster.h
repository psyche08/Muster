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

void muster_desktop_update_startup(void);
/* Desktop startup (ADR 0021, ADR 0227). Both run synchronously on a worker
 * thread and are not callable from a Rust runtime; they share one admission.
 * socket is a NUL-terminated UTF-8 path. managed: 0 a daemon run by hand
 * (no callbacks), 1 the bundled service. No callback after return.
 * action: 0 register (run the system LaunchAgent's job; never stop a
 * running one), 1 recover a stopped job, 2 install the system LaunchAgent
 * (asks for the administrator's password), 3 inspect, 4 reload the system
 * job after an install changed it. Inspect returns facts >= 0: bits 0-1
 * agent (0 missing, 1 current, 2 outdated), 2-3 job (0 absent, 1 stopped,
 * 2 running), 4-5 origin (0 none, 1 system LaunchAgent, 2 other). Other
 * actions return 0 success, 1 approval required, 3 cancelled by the owner,
 * any other value a native error.
 * check: what to show first; changes nothing. resumed: 1 when Muster's own
 * restart or update relaunched the app. Returns 0 ready, 1 start musterd,
 * 2 offer the move to the system LaunchAgent; plus 16 when that needs the
 * administrator's password, plus 32 when it starts without asking; -1 error.
 * prepare: install, register and wait for readiness. Returns 0 ready, -1
 * error. Free *error_out with muster_string_free. */
int32_t muster_desktop_check(const char *socket, uint8_t managed, uint8_t resumed,
                             int32_t (*action)(uint32_t), char **error_out);
int32_t muster_desktop_prepare(const char *socket, uint8_t managed,
                             int32_t (*action)(uint32_t), char **error_out);
/* Logging to stderr at `level` (0 = off, 1 = error, 2 = warn, 3 = info, 4 = debug). */
void muster_log_init(uint32_t level);
/* The same, and also to app.log in the platform's log directory (macOS:
 * ~/Library/Logs/Muster, or $MUSTER_DIR when set), kept to two files of
 * 8 MiB (app.log, app.log.1). For a desktop app, whose stderr goes nowhere. */
void muster_log_init_file(uint32_t level);

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
void muster_core_set_video_callback(MusterCore *core, void (*callback)(void *, uint64_t, const uint8_t *, size_t));
void muster_core_free(MusterCore *core);

/* True for a canonical desktop presence or Locked Use challenge. */
uint8_t muster_is_presence_challenge(const uint8_t *bytes, size_t length);

void muster_string_free(char *s);

#ifdef __cplusplus
}
#endif
#endif

// The NAPI shell around muster.h (ARCHITECTURE §3.7): bytes in, bytes out.
// View models and effects cross into ArkTS through threadsafe functions.
// The key callbacks never do: the core asks for them on whichever thread it
// is on, the ArkTS thread included (it starts there, and signs its link
// certificate while starting), so a hop to ArkTS would wait on itself. The
// public points arrive with create(); signatures come from HUKS's C API,
// which is synchronous where its ArkTS API is not.
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "huks/native_huks_api.h"
#include "huks/native_huks_param.h"
#include "muster.h"
#include "napi/native_api.h"

namespace {

struct Shell {
    MusterCore *core = nullptr;
    MusterPlatform platform{};
    napi_threadsafe_function on_view = nullptr;
    napi_threadsafe_function on_effect = nullptr;
    std::vector<uint8_t> device_point;    // 65-byte X9.63; empty: no hardware key
    std::vector<uint8_t> presence_point;
    std::string device_alias;             // the HUKS key hw_sign uses (Keys.ets)
};

struct ViewEvent {
    std::vector<uint8_t> bytes;
};

struct EffectEvent {
    uint64_t id;
    std::vector<uint8_t> bytes;
};

napi_value bytes_to_array(napi_env env, const std::vector<uint8_t> &bytes) {
    void *data = nullptr;
    napi_value buffer = nullptr;
    napi_create_arraybuffer(env, bytes.size(), &data, &buffer);
    if (!bytes.empty()) memcpy(data, bytes.data(), bytes.size());
    napi_value array = nullptr;
    napi_create_typedarray(env, napi_uint8_array, bytes.size(), buffer, 0, &array);
    return array;
}

std::vector<uint8_t> array_to_bytes(napi_env env, napi_value value) {
    bool is_typed = false;
    napi_is_typedarray(env, value, &is_typed);
    if (!is_typed) return {};
    napi_typedarray_type type;
    size_t length = 0;
    void *data = nullptr;
    napi_value buffer;
    size_t offset = 0;
    napi_get_typedarray_info(env, value, &type, &length, &data, &buffer, &offset);
    if (type != napi_uint8_array || !data) return {};
    const uint8_t *start = static_cast<const uint8_t *>(data);
    return std::vector<uint8_t>(start, start + length);
}

// ArkTS side: onView(bytes: Uint8Array)
void call_on_view(napi_env env, napi_value callback, void *, void *data) {
    auto *event = static_cast<ViewEvent *>(data);
    if (env && callback) {
        napi_value argv[1] = {bytes_to_array(env, event->bytes)};
        napi_value undefined;
        napi_get_undefined(env, &undefined);
        napi_call_function(env, undefined, callback, 1, argv, nullptr);
    }
    delete event;
}

// ArkTS side: onEffect(id: string, bytes: Uint8Array); ids are decimal
// strings because a u64 does not fit a JS number.
void call_on_effect(napi_env env, napi_value callback, void *, void *data) {
    auto *event = static_cast<EffectEvent *>(data);
    if (env && callback) {
        napi_value id;
        napi_create_string_utf8(env, std::to_string(event->id).c_str(), NAPI_AUTO_LENGTH, &id);
        napi_value argv[2] = {id, bytes_to_array(env, event->bytes)};
        napi_value undefined;
        napi_get_undefined(env, &undefined);
        napi_call_function(env, undefined, callback, 2, argv, nullptr);
    }
    delete event;
}

void on_view(void *ctx, const uint8_t *bytes, size_t len) {
    auto *shell = static_cast<Shell *>(ctx);
    if (!shell->on_view) return;
    auto *event = new ViewEvent{std::vector<uint8_t>(bytes, bytes + len)};
    if (napi_call_threadsafe_function(shell->on_view, event, napi_tsfn_nonblocking) != napi_ok) delete event;
}

void on_effect(void *ctx, uint64_t id, const uint8_t *bytes, size_t len) {
    auto *shell = static_cast<Shell *>(ctx);
    if (!shell->on_effect) return;
    auto *event = new EffectEvent{id, std::vector<uint8_t>(bytes, bytes + len)};
    if (napi_call_threadsafe_function(shell->on_effect, event, napi_tsfn_nonblocking) != napi_ok) delete event;
}

intptr_t copy_out(const std::vector<uint8_t> &bytes, uint8_t *out, size_t cap) {
    if (bytes.empty() || bytes.size() > cap) return -1;
    memcpy(out, bytes.data(), bytes.size());
    return static_cast<intptr_t>(bytes.size());
}

intptr_t hw_public_key(void *ctx, uint8_t *out, size_t cap) {
    return copy_out(static_cast<Shell *>(ctx)->device_point, out, cap);
}

intptr_t presence_public_key(void *ctx, uint8_t *out, size_t cap) {
    return copy_out(static_cast<Shell *>(ctx)->presence_point, out, cap);
}

OH_Huks_Param uint_param(uint32_t tag, uint32_t value) {
    OH_Huks_Param param{};
    param.tag = tag;
    param.uint32Param = value;
    return param;
}

// Owns a built HUKS parameter set.
struct ParamSet {
    OH_Huks_ParamSet *set = nullptr;
    ~ParamSet() {
        if (set) OH_Huks_FreeParamSet(&set);
    }
};

// ECDSA P-256 over SHA-256 with the device key, as Keys.ets generated it;
// the storage level must match or HUKS will not find the key.
bool sign_params(ParamSet &params) {
    const OH_Huks_Param list[] = {
        uint_param(OH_HUKS_TAG_ALGORITHM, OH_HUKS_ALG_ECC),
        uint_param(OH_HUKS_TAG_KEY_SIZE, OH_HUKS_ECC_KEY_SIZE_256),
        uint_param(OH_HUKS_TAG_PURPOSE, OH_HUKS_KEY_PURPOSE_SIGN),
        uint_param(OH_HUKS_TAG_DIGEST, OH_HUKS_DIGEST_SHA256),
        uint_param(OH_HUKS_TAG_AUTH_STORAGE_LEVEL, OH_HUKS_AUTH_STORAGE_LEVEL_ECE),
    };
    return OH_Huks_InitParamSet(&params.set).errorCode == OH_HUKS_SUCCESS &&
           OH_Huks_AddParams(params.set, list, sizeof(list) / sizeof(list[0])).errorCode == OH_HUKS_SUCCESS &&
           OH_Huks_BuildParamSet(&params.set).errorCode == OH_HUKS_SUCCESS;
}

// A DER signature into out, on the calling thread; -1 when there is no
// hardware key or HUKS refuses.
intptr_t hw_sign(void *ctx, const uint8_t *msg, size_t msg_len, uint8_t *out, size_t cap) {
    auto *shell = static_cast<Shell *>(ctx);
    if (shell->device_alias.empty() || shell->device_point.empty() || msg_len > UINT32_MAX) return -1;
    ParamSet params;
    if (!sign_params(params)) return -1;
    OH_Huks_Blob alias{static_cast<uint32_t>(shell->device_alias.size()),
                       reinterpret_cast<uint8_t *>(shell->device_alias.data())};
    uint8_t handle_bytes[sizeof(uint64_t)] = {};
    OH_Huks_Blob handle{sizeof(handle_bytes), handle_bytes};
    if (OH_Huks_InitSession(&alias, params.set, &handle, nullptr).errorCode != OH_HUKS_SUCCESS) return -1;
    OH_Huks_Blob in{static_cast<uint32_t>(msg_len), const_cast<uint8_t *>(msg)};
    OH_Huks_Blob signature{static_cast<uint32_t>(cap > UINT32_MAX ? UINT32_MAX : cap), out};
    if (OH_Huks_FinishSession(&handle, params.set, &in, &signature).errorCode != OH_HUKS_SUCCESS) {
        OH_Huks_AbortSession(&handle, params.set);
        return -1;
    }
    return static_cast<intptr_t>(signature.size);
}

napi_threadsafe_function make_tsfn(napi_env env, napi_value callback, const char *name, napi_threadsafe_function_call_js call) {
    napi_value resource_name;
    napi_create_string_utf8(env, name, NAPI_AUTO_LENGTH, &resource_name);
    napi_threadsafe_function tsfn = nullptr;
    napi_create_threadsafe_function(env, callback, nullptr, resource_name, 0, 1, nullptr, nullptr, nullptr, call, &tsfn);
    return tsfn;
}

Shell *shell_of(napi_env env, napi_value value) {
    int64_t handle = 0;
    napi_get_value_int64(env, value, &handle);
    return reinterpret_cast<Shell *>(static_cast<intptr_t>(handle));
}

std::string string_of(napi_env env, napi_value value) {
    size_t length = 0;
    if (napi_get_value_string_utf8(env, value, nullptr, 0, &length) != napi_ok) return {};
    std::string text(length, '\0');
    napi_get_value_string_utf8(env, value, text.data(), length + 1, &length);
    return text;
}

// create(configJson: Uint8Array, onView, onEffect, devicePublicKey: Uint8Array | null,
//        presencePublicKey: Uint8Array | null, deviceKeyAlias: string): number
napi_value Create(napi_env env, napi_callback_info info) {
    size_t argc = 6;
    napi_value argv[6];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    if (argc < 6) {
        napi_throw_error(env, nullptr,
                         "create(config, onView, onEffect, devicePublicKey, presencePublicKey, deviceKeyAlias)");
        return nullptr;
    }
    auto *shell = new Shell();
    shell->on_view = make_tsfn(env, argv[1], "muster onView", call_on_view);
    shell->on_effect = make_tsfn(env, argv[2], "muster onEffect", call_on_effect);
    shell->device_point = array_to_bytes(env, argv[3]);
    shell->presence_point = array_to_bytes(env, argv[4]);
    shell->device_alias = string_of(env, argv[5]);
    shell->platform.ctx = shell;
    shell->platform.on_view = on_view;
    shell->platform.on_effect = on_effect;
    shell->platform.hw_public_key = hw_public_key;
    shell->platform.hw_sign = hw_sign;
    shell->platform.presence_public_key = presence_public_key;

    std::vector<uint8_t> config = array_to_bytes(env, argv[0]);
    char *error = nullptr;
    shell->core = muster_core_new(config.data(), config.size(), &shell->platform, &error);
    if (!shell->core) {
        std::string message = error ? error : "core refused to start";
        muster_string_free(error);
        napi_release_threadsafe_function(shell->on_view, napi_tsfn_abort);
        napi_release_threadsafe_function(shell->on_effect, napi_tsfn_abort);
        delete shell;
        napi_throw_error(env, nullptr, message.c_str());
        return nullptr;
    }
    napi_value handle;
    napi_create_int64(env, static_cast<int64_t>(reinterpret_cast<intptr_t>(shell)), &handle);
    return handle;
}

napi_value Intent(napi_env env, napi_callback_info info) {
    size_t argc = 2;
    napi_value argv[2];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    Shell *shell = argc >= 2 ? shell_of(env, argv[0]) : nullptr;
    if (shell && shell->core) {
        std::vector<uint8_t> bytes = array_to_bytes(env, argv[1]);
        muster_core_intent(shell->core, bytes.data(), bytes.size());
    }
    return nullptr;
}

// effectResult(handle, id: string, bytes)
napi_value EffectResult(napi_env env, napi_callback_info info) {
    size_t argc = 3;
    napi_value argv[3];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    Shell *shell = argc >= 3 ? shell_of(env, argv[0]) : nullptr;
    if (shell && shell->core) {
        char id_text[32] = {0};
        size_t id_len = 0;
        napi_get_value_string_utf8(env, argv[1], id_text, sizeof(id_text), &id_len);
        uint64_t id = strtoull(id_text, nullptr, 10);
        std::vector<uint8_t> bytes = array_to_bytes(env, argv[2]);
        muster_core_effect_result(shell->core, id, bytes.data(), bytes.size());
    }
    return nullptr;
}

napi_value Lifecycle(napi_env env, napi_callback_info info) {
    size_t argc = 2;
    napi_value argv[2];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    Shell *shell = argc >= 2 ? shell_of(env, argv[0]) : nullptr;
    if (shell && shell->core) {
        int32_t event = 0;
        napi_get_value_int32(env, argv[1], &event);
        muster_core_lifecycle(shell->core, static_cast<uint32_t>(event));
    }
    return nullptr;
}

napi_value SshPublicKey(napi_env env, napi_callback_info info) {
    size_t argc = 1;
    napi_value argv[1];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    Shell *shell = argc >= 1 ? shell_of(env, argv[0]) : nullptr;
    napi_value result;
    char *line = (shell && shell->core) ? muster_core_ssh_public_key(shell->core) : nullptr;
    if (!line) {
        napi_get_null(env, &result);
        return result;
    }
    napi_create_string_utf8(env, line, NAPI_AUTO_LENGTH, &result);
    muster_string_free(line);
    return result;
}

napi_value Destroy(napi_env env, napi_callback_info info) {
    size_t argc = 1;
    napi_value argv[1];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    Shell *shell = argc >= 1 ? shell_of(env, argv[0]) : nullptr;
    if (!shell) return nullptr;
    if (shell->core) muster_core_free(shell->core);
    shell->core = nullptr;
    napi_release_threadsafe_function(shell->on_view, napi_tsfn_release);
    napi_release_threadsafe_function(shell->on_effect, napi_tsfn_release);
    delete shell;
    return nullptr;
}

napi_value LogInit(napi_env env, napi_callback_info info) {
    size_t argc = 1;
    napi_value argv[1];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    int32_t level = 3;
    if (argc >= 1) napi_get_value_int32(env, argv[0], &level);
    muster_log_init(static_cast<uint32_t>(level));
    return nullptr;
}

napi_value Version(napi_env env, napi_callback_info) {
    napi_value result;
    napi_create_string_utf8(env, muster_version(), NAPI_AUTO_LENGTH, &result);
    return result;
}

napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor desc[] = {
        {"create", nullptr, Create, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"intent", nullptr, Intent, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"effectResult", nullptr, EffectResult, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"lifecycle", nullptr, Lifecycle, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"sshPublicKey", nullptr, SshPublicKey, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"destroy", nullptr, Destroy, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"logInit", nullptr, LogInit, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"version", nullptr, Version, nullptr, nullptr, nullptr, napi_default, nullptr},
    };
    napi_define_properties(env, exports, sizeof(desc) / sizeof(desc[0]), desc);
    return exports;
}

napi_module module = {
    .nm_version = 1,
    .nm_flags = 0,
    .nm_filename = nullptr,
    .nm_register_func = Init,
    .nm_modname = "muster",
    .nm_priv = nullptr,
    .reserved = {0},
};

}  // namespace

extern "C" __attribute__((constructor)) void RegisterMusterModule() {
    napi_module_register(&module);
}

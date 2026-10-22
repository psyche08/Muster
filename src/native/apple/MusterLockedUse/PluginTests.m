#import "Plugin.m"
#import <assert.h>

static AuthorizationResult result;
static AuthorizationValue context;
static OSStatus setResult(AuthorizationEngineRef engine, AuthorizationResult value) {
    (void)engine; result = value; return errAuthorizationSuccess;
}
static OSStatus getContext(AuthorizationEngineRef engine, AuthorizationString key, AuthorizationContextFlags *flags, const AuthorizationValue **value) {
    (void)engine; (void)key; *flags = 0; *value = &context; return errAuthorizationSuccess;
}
static OSStatus didDeactivate(AuthorizationEngineRef engine) { (void)engine; return errAuthorizationSuccess; }
int main(void) {
    @autoreleasepool {
        AuthorizationCallbacks callbacks = {0};
        callbacks.SetResult = setResult; callbacks.GetContextValue = getContext; callbacks.DidDeactivate = didDeactivate;
        AuthorizationPluginRef plugin = NULL; const AuthorizationPluginInterface *api = NULL;
        assert(AuthorizationPluginCreate(&callbacks, &plugin, &api) == errAuthorizationSuccess);
        AuthorizationMechanismRef mechanism = NULL;
        assert(api->MechanismCreate(plugin, (AuthorizationEngineRef)&callbacks, "not-invoke", &mechanism) != errAuthorizationSuccess);
        const char *cases[] = { "", "root", "__muster_nonexistent_user__", "root\0extra" };
        size_t lengths[] = { 0, 4, strlen(cases[2]), 10 };
        for (size_t i = 0; i < 4; i++) {
            context.length = (UInt32)lengths[i]; context.data = (void *)cases[i];
            result = kAuthorizationResultUndefined;
            assert(api->MechanismCreate(plugin, (AuthorizationEngineRef)&callbacks, "invoke", &mechanism) == errAuthorizationSuccess);
            assert(api->MechanismInvoke(mechanism) == errAuthorizationSuccess);
            assert(result == kAuthorizationResultDeny);
            assert(api->MechanismDeactivate(mechanism) == errAuthorizationSuccess);
            assert(api->MechanismDestroy(mechanism) == errAuthorizationSuccess);
        }
        assert(api->PluginDestroy(plugin) == errAuthorizationSuccess);
        puts("Plugin invalid-context and root-user denial tests passed");
    }
}

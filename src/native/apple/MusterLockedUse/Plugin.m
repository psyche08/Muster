// OS authorization adapter only. Muster's Rust owner verifies the signed
// operation, TTL, one-shot nonce, console, input, cover and watchdog.
#import <Foundation/Foundation.h>
#import <Security/AuthorizationPlugin.h>
#import <Security/AuthorizationTags.h>
#import <Security/Security.h>
#import <bsm/libbsm.h>
#import <pwd.h>
#import <sys/socket.h>
#import <sys/un.h>
#import <sys/stat.h>
#import <fcntl.h>
#import <unistd.h>
#import <arpa/inet.h>
#import <os/log.h>

// Lifecycle only: no nonce, username, socket path or credential is logged.
static os_log_t logger(void) {
    static os_log_t log; static dispatch_once_t once;
    dispatch_once(&once, ^{ log = os_log_create("dev.linsheng.muster.locked-use", "plugin"); });
    return log;
}

typedef struct { const AuthorizationCallbacks *callbacks; } Plugin;
typedef struct { AuthorizationEngineRef engine; Plugin *plugin; int fd; BOOL completed; uint8_t nonce[32]; } Mechanism;

static BOOL transfer(int fd, void *bytes, size_t count, BOOL writing) {
    size_t offset = 0;
    while (offset < count) {
        ssize_t n = writing ? write(fd, (char *)bytes + offset, count - offset) : read(fd, (char *)bytes + offset, count - offset);
        if (n <= 0) return NO;
        offset += (size_t)n;
    }
    return YES;
}
static BOOL daemonPeer(int fd, uid_t expected) {
    uid_t uid; gid_t gid; audit_token_t token; socklen_t length = sizeof(token);
    if (getpeereid(fd, &uid, &gid) != 0 || uid != expected ||
        getsockopt(fd, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &length) != 0 || length != sizeof(token)) return NO;
    NSDictionary *attrs = @{ (__bridge id)kSecGuestAttributeAudit: [NSData dataWithBytes:&token length:sizeof(token)] };
    SecCodeRef code = NULL; SecRequirementRef req = NULL;
    OSStatus status = SecCodeCopyGuestWithAttributes(NULL, (__bridge CFDictionaryRef)attrs, 0, &code);
    if (status != errSecSuccess || !code) return NO;
    status = SecRequirementCreateWithString(CFSTR("identifier \"dev.linsheng.musterd\" and anchor apple generic and certificate leaf[subject.OU] = \"89LGY6BD53\""), 0, &req);
    BOOL ok = status == errSecSuccess && req && SecCodeCheckValidity(code, kSecCSStrictValidate, req) == errSecSuccess;
    if (req) CFRelease(req);
    CFRelease(code);
    return ok;
}
static BOOL consume(Mechanism *m) {
    AuthorizationContextFlags flags = 0; const AuthorizationValue *value = NULL;
    if (m->plugin->callbacks->GetContextValue(m->engine, kAuthorizationEnvironmentUsername, &flags, &value) != errAuthorizationSuccess ||
        !value || !value->data || !value->length || value->length > 256 || memchr(value->data, 0, value->length)) return NO;
    char name[257] = {0}; memcpy(name, value->data, value->length);
    struct passwd record, *resolved = NULL; char buffer[16384];
    if (getpwnam_r(name, &record, buffer, sizeof(buffer), &resolved) != 0 || !resolved || record.pw_uid == 0) return NO;
    NSString *path = [NSString stringWithFormat:@"/Library/Application Support/Muster/LockedUse/%u.plist", record.pw_uid];
    int configFD = open(path.fileSystemRepresentation, O_RDONLY | O_NOFOLLOW | O_CLOEXEC);
    if (configFD < 0) return NO;
    struct stat st;
    if (fstat(configFD, &st) != 0 || !S_ISREG(st.st_mode) || st.st_uid != 0 || (st.st_mode & 0022) || st.st_size <= 0 || st.st_size > 2048) { close(configFD); return NO; }
    NSMutableData *data = [NSMutableData dataWithLength:(NSUInteger)st.st_size];
    BOOL readOK = transfer(configFD, data.mutableBytes, data.length, NO); close(configFD);
    if (!readOK) return NO;
    NSDictionary *config = [NSPropertyListSerialization propertyListWithData:data options:0 format:NULL error:NULL];
    if (![config isKindOfClass:NSDictionary.class] || ![config[@"socket"] isKindOfClass:NSString.class]) return NO;
    NSString *socketPath = config[@"socket"];
    struct sockaddr_un address = { .sun_family = AF_UNIX };
    const char *socketBytes = socketPath.fileSystemRepresentation;
    if (!socketBytes || strlen(socketBytes) >= sizeof(address.sun_path)) return NO;
    memcpy(address.sun_path, socketBytes, strlen(socketBytes) + 1);
    if (lstat(socketBytes, &st) != 0 || !S_ISSOCK(st.st_mode) || st.st_uid != record.pw_uid || (st.st_mode & 0077)) return NO;
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) return NO;
    fcntl(fd, F_SETFD, FD_CLOEXEC);
    struct timeval timeout = { .tv_sec = 1 };
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    int one = 1; setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &one, sizeof(one));
    if (connect(fd, (struct sockaddr *)&address, sizeof(address)) != 0 || !daemonPeer(fd, record.pw_uid)) { close(fd); return NO; }
    uint8_t hello[9] = {'M','L','U','1','C'}; uint32_t uid = htonl(record.pw_uid); memcpy(hello + 5, &uid, 4);
    if (!transfer(fd, hello, sizeof(hello), YES) || !transfer(fd, m->nonce, sizeof(m->nonce), NO)) { close(fd); return NO; }
    m->fd = fd;
    return YES;
}
static OSStatus create(AuthorizationPluginRef plugin, AuthorizationEngineRef engine, AuthorizationMechanismId id, AuthorizationMechanismRef *out) {
    if (strcmp(id, "invoke")) return errAuthorizationInternal;
    Mechanism *m = calloc(1, sizeof(*m)); if (!m) return errAuthorizationInternal;
    m->plugin = (Plugin *)plugin; m->engine = engine; m->fd = -1; *out = (AuthorizationMechanismRef)m;
    os_log(logger(), "mechanism created");
    return errAuthorizationSuccess;
}
static OSStatus invoke(AuthorizationMechanismRef ref) {
    Mechanism *m = (Mechanism *)ref;
    if (m->fd >= 0) { close(m->fd); m->fd = -1; }
    m->completed = NO;
    BOOL allowed = NO;
    @try { allowed = consume(m); } @catch (__unused NSException *exception) { allowed = NO; }
    OSStatus status = m->plugin->callbacks->SetResult(m->engine, allowed ? kAuthorizationResultAllow : kAuthorizationResultDeny);
    // Failed SetResult is ambiguous. Only Destroy following successful Allow
    // is terminal evidence; Deactivate or EOF cannot manufacture a denial.
    m->completed = allowed && status == errAuthorizationSuccess;
    os_log(logger(), "invoke: daemon admitted=%{public}d set_result=%{public}d", allowed, (int)status);
    return status;
}
static OSStatus deactivate(AuthorizationMechanismRef ref) {
    Mechanism *m = (Mechanism *)ref;
    os_log(logger(), "deactivate: completed=%{public}d", m->completed);
    m->completed = NO;
    return m->plugin->callbacks->DidDeactivate(m->engine);
}
static OSStatus destroy(AuthorizationMechanismRef ref) {
    Mechanism *m = (Mechanism *)ref;
    BOOL sent = NO;
    if (m->completed && m->fd >= 0) {
        uint8_t terminal[33] = {'A'}; memcpy(terminal + 1, m->nonce, 32);
        sent = transfer(m->fd, terminal, sizeof(terminal), YES);
    }
    os_log(logger(), "destroy: completed=%{public}d terminal_sent=%{public}d", m->completed, sent);
    if (m->fd >= 0) close(m->fd);
    free(m); return errAuthorizationSuccess;
}
static OSStatus destroyPlugin(AuthorizationPluginRef p) { free(p); return errAuthorizationSuccess; }
static AuthorizationPluginInterface interface = { kAuthorizationPluginInterfaceVersion, destroyPlugin, create, invoke, deactivate, destroy };
OSStatus AuthorizationPluginCreate(const AuthorizationCallbacks *callbacks, AuthorizationPluginRef *out, const AuthorizationPluginInterface **api) {
    Plugin *p = calloc(1, sizeof(*p)); if (!p) return errAuthorizationInternal;
    p->callbacks = callbacks; *out = (AuthorizationPluginRef)p; *api = &interface; return errAuthorizationSuccess;
}

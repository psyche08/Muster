// OS adapters only. Lease/authorization/watchdog decisions belong to Rust.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Security/Security.h>
#import <bsm/libbsm.h>
#import <sys/socket.h>
#import <sys/un.h>
#import <dlfcn.h>
#import <unistd.h>
#import <IOKit/pwr_mgt/IOPMLib.h>

static NSMutableArray<NSWindow *> *panels;

// Remote activity wakes the real system lock UI without synthetic input.
// Rust owns the lifetime and releases the assertion before submitting auth.
bool muster_console_wake_begin(uint32_t *out) {
    IOPMAssertionID assertion = kIOPMNullAssertionID;
    IOReturn result = IOPMAssertionDeclareUserActivity(CFSTR("Muster Locked Use"),
        kIOPMUserActiveRemote, &assertion);
    if (result != kIOReturnSuccess || assertion == kIOPMNullAssertionID) return false;
    *out = assertion;
    return true;
}
bool muster_console_wake_end(uint32_t assertion) {
    return IOPMAssertionRelease(assertion) == kIOReturnSuccess;
}

// OS adapter only: Rust supplies the remaining transaction lifetime. The OS
// releases this assertion at that deadline even if the watchdog stops running.
bool muster_console_display_awake_begin(uint64_t remaining_ms, uint32_t *out) {
    IOPMAssertionID assertion = kIOPMNullAssertionID;
    IOReturn result = IOPMAssertionCreateWithDescription(
        kIOPMAssertionTypePreventUserIdleDisplaySleep,
        CFSTR("Muster Locked Use display"), NULL, NULL, NULL,
        (CFTimeInterval)remaining_ms / 1000.0,
        kIOPMAssertionTimeoutActionRelease, &assertion);
    if (result != kIOReturnSuccess || assertion == kIOPMNullAssertionID) return false;
    *out = assertion;
    return true;
}

// OS-only Return submission. Unlike HID posting, session posting does not
// advance hardware-source input counters on the tested macOS lock screen.
// The Rust caller must establish a locked console, focus and an empty field.
// The authorization plugin remains the sole authority; there is no retry.
// Posting is not evidence that loginwindow processed it or granted access.
static bool post_session_key(CGKeyCode key, CGEventFlags downFlags, pid_t target) {
    if (!CGPreflightPostEventAccess()) return false;
    CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStatePrivate);
    if (!source) return false;
    CGEventSourceSetLocalEventsFilterDuringSuppressionState(source,
        kCGEventFilterMaskPermitLocalMouseEvents | kCGEventFilterMaskPermitSystemDefinedEvents |
        kCGEventFilterMaskPermitLocalKeyboardEvents, kCGEventSuppressionStateSuppressionInterval);
    CGEventRef down = CGEventCreateKeyboardEvent(source, key, true);
    CGEventRef up = CGEventCreateKeyboardEvent(source, key, false);
    bool ready = down && up;
    if (ready) {
        CGEventSetFlags(down, downFlags);
        CGEventSetFlags(up, 0);
        if (target > 0) {
            CGEventPostToPid(target, down);
            CGEventPostToPid(target, up);
        } else {
            CGEventPost(kCGSessionEventTap, down);
            CGEventPost(kCGSessionEventTap, up);
        }
    }
    if (down) CFRelease(down);
    if (up) CFRelease(up);
    CFRelease(source);
    return ready;
}

bool muster_console_post_return(void) {
    return post_session_key(36, 0, 0);
}

// Nonprinting UI reveal, never an authorization submission. Rust requires a
// verified locked loginwindow and a live continuation before this one pair.
bool muster_console_reveal_lock_ui(int32_t pid) {
    // A modifier alone does not reliably reveal the clock-only lock screen.
    // Route one nonprinting Left arrow to the verified loginwindow process,
    // independent of the user-session cover's event routing. Never submit auth.
    if (pid <= 0) return false;
    return post_session_key(123, 0, pid);
}

bool muster_console_sample(uint32_t *uid, uint32_t *asid, bool *locked) {
    CFDictionaryRef raw = CGSessionCopyCurrentDictionary();
    if (!raw) return false;
    NSDictionary *s = CFBridgingRelease(raw);
    NSNumber *user = s[@"kCGSSessionUserIDKey"] ?: s[@"kCGSessionUserIDKey"];
    NSNumber *on = s[@"kCGSSessionOnConsoleKey"];
    NSNumber *done = s[@"kCGSessionLoginDoneKey"];
    NSNumber *screen = s[@"CGSSessionScreenIsLocked"];
    auditinfo_addr_t audit;
    if (!user || !on.boolValue || !done.boolValue || user.unsignedIntValue != getuid() ||
        getaudit_addr(&audit, sizeof(audit)) != 0) return false;
    *uid = user.unsignedIntValue;
    *asid = audit.ai_asid;
    // The key is absent when unlocked in CGSession's actual dictionary.
    *locked = screen ? screen.boolValue : false;
    return true;
}

bool muster_console_cover(void) {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
    if (panels) return false;
    panels = [NSMutableArray array];
    for (NSScreen *screen in NSScreen.screens) {
        // initWithContentRect:...screen: interprets the origin relative to
        // that screen. Passing its global origin adds the offset twice.
        NSRect local = NSMakeRect(0, 0, screen.frame.size.width, screen.frame.size.height);
        NSPanel *panel = [[NSPanel alloc] initWithContentRect:local
            styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
            backing:NSBackingStoreBuffered defer:NO screen:screen];
        [panel setFrame:screen.frame display:NO];
        panel.level = CGShieldingWindowLevel() + 1;
        panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
            NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorStationary;
        panel.backgroundColor = NSColor.blackColor;
        panel.opaque = YES;
        panel.hasShadow = NO;
        panel.hidesOnDeactivate = NO;
        panel.ignoresMouseEvents = NO;
        panel.sharingType = NSWindowSharingNone;
        panel.releasedWhenClosed = NO;
        [panel orderFrontRegardless];
        [panels addObject:panel];
    }
    [NSApp finishLaunching];
    return panels.count > 0;
}

bool muster_console_cover_healthy(bool unlocked) {
    NSArray<NSScreen *> *screens = NSScreen.screens;
    if (!panels || !screens.count || panels.count != screens.count) return false;
    // Switching out of loginwindow can reorder the user-session windows.
    // Reassert the existing nonactivating cover before observing its state;
    // never activate it or move focus away from the AX target.
    if (unlocked) for (NSWindow *panel in panels) [panel orderFrontRegardless];
    for (NSUInteger i = 0; i < screens.count; i++) {
        NSWindow *panel = panels[i];
        if (!panel.visible || !NSEqualRects(panel.frame, screens[i].frame)) return false;
        // Query only our known cover. Enumerating every Electron window can
        // stall the watchdog across the 250 ms freshness boundary at unlock.
        // The same ownership, opacity, layer and onscreen checks still apply.
        NSArray *windows = CFBridgingRelease(CGWindowListCopyWindowInfo(
            kCGWindowListOptionIncludingWindow, (CGWindowID)panel.windowNumber));
        BOOL found = NO;
        for (NSDictionary *w in windows) {
            if ([w[(id)kCGWindowNumber] integerValue] != panel.windowNumber) continue;
            if ([w[(id)kCGWindowOwnerPID] intValue] != getpid() ||
                [w[(id)kCGWindowLayer] intValue] != panel.level ||
                [w[(id)kCGWindowAlpha] doubleValue] != 1.0) return false;
            if (unlocked && ![w[(id)kCGWindowIsOnscreen] boolValue]) return false;
            found = YES;
            break;
        }
        if (!found) return false;
    }
    return true;
}

void muster_console_pump(void) {
    @autoreleasepool {
        NSEvent *event = [NSApp nextEventMatchingMask:NSEventMaskAny
            untilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]
            inMode:NSDefaultRunLoopMode dequeue:YES];
        if (event) [NSApp sendEvent:event];
        [NSApp updateWindows];
    }
}

void muster_console_uncover(void) {
    for (NSWindow *panel in panels) [panel orderOut:nil];
    panels = nil;
}

bool muster_console_lock(void) {
    void *h = dlopen("/System/Library/PrivateFrameworks/login.framework/Versions/Current/login", RTLD_NOW | RTLD_LOCAL);
    if (!h) return false;
    void (*lock)(void) = dlsym(h, "SACLockScreenImmediate");
    if (lock) lock();
    dlclose(h);
    return lock != NULL;
}

// Kernel-supplied audit token, not PID/name supplied by the peer. It avoids
// PID-reuse ambiguity when checking a short-lived authorization host.
bool muster_locked_peer(int fd, const char *requirement, uint32_t expected_uid) {
    uid_t uid; gid_t gid;
    audit_token_t token; socklen_t length = sizeof(token);
    if (getpeereid(fd, &uid, &gid) != 0 || uid != expected_uid ||
        getsockopt(fd, SOL_LOCAL, LOCAL_PEERTOKEN, &token, &length) != 0 || length != sizeof(token)) return false;
    NSDictionary *attrs = @{ (__bridge id)kSecGuestAttributeAudit:
        [NSData dataWithBytes:&token length:sizeof(token)] };
    SecCodeRef code = NULL; SecRequirementRef req = NULL;
    OSStatus s = SecCodeCopyGuestWithAttributes(NULL, (__bridge CFDictionaryRef)attrs, kSecCSDefaultFlags, &code);
    if (s != errSecSuccess || !code) return false;
    s = SecRequirementCreateWithString((__bridge CFStringRef)[NSString stringWithUTF8String:requirement], 0, &req);
    bool ok = s == errSecSuccess && req && SecCodeCheckValidity(code, kSecCSStrictValidate, req) == errSecSuccess;
    if (req) CFRelease(req);
    CFRelease(code);
    return ok;
}

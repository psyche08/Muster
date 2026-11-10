// AppKit and IOPM effects for the mobile blackout helper. This process owns
// its windows and assertions for exactly one connection to its parent.
#import <AppKit/AppKit.h>
#import <IOKit/pwr_mgt/IOPMLib.h>
#import <unistd.h>

extern NSPanel *muster_black_panel(NSScreen *screen, BOOL ignoresMouse);

static BOOL preventIdle(CFStringRef type, IOPMAssertionID *assertion) {
    return IOPMAssertionCreateWithName(type, kIOPMAssertionLevelOn,
        CFSTR("Muster display blackout"), assertion) == kIOReturnSuccess;
}

static BOOL reconcile(NSMutableDictionary<NSNumber *, NSPanel *> *panels) {
    NSMutableSet *seen = [NSMutableSet set];
    NSArray<NSScreen *> *screens = NSScreen.screens;
    BOOL ready = screens.count > 0;
    for (NSScreen *screen in screens) {
        NSNumber *key = screen.deviceDescription[@"NSScreenNumber"];
        if (!key) { ready = NO; continue; }
        [seen addObject:key];
        NSPanel *panel = panels[key];
        if (!panel) {
            panel = muster_black_panel(screen, YES);
            panels[key] = panel;
        }
        [panel setFrame:screen.frame display:YES];
        [panel orderFrontRegardless];
        ready = ready && panel.visible && NSEqualRects(panel.frame, screen.frame);
    }
    for (NSNumber *key in panels.allKeys) {
        if ([seen containsObject:key]) continue;
        [panels[key] orderOut:nil];
        [panels removeObjectForKey:key];
    }
    return ready;
}

bool muster_blackout_run(void) {
    @autoreleasepool {
        IOPMAssertionID system = 0, display = 0;
        if (!preventIdle(kIOPMAssertionTypePreventUserIdleSystemSleep, &system)) return false;
        if (!preventIdle(kIOPMAssertionTypePreventUserIdleDisplaySleep, &display)) {
            IOPMAssertionRelease(system);
            return false;
        }
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [NSApp finishLaunching];
        NSMutableDictionary *panels = [NSMutableDictionary dictionary];
        __block int last = -1;
        void (^refresh)(NSNotification *) = ^(NSNotification *note) {
            int ready = reconcile(panels);
            if (ready != last) {
                char report = ready ? '1' : '0';
                if (write(STDOUT_FILENO, &report, 1) != 1) _exit(1);
                last = ready;
            }
        };
        NSNotificationCenter *app = NSNotificationCenter.defaultCenter;
        NSNotificationCenter *space = NSWorkspace.sharedWorkspace.notificationCenter;
        id layout = [app addObserverForName:NSApplicationDidChangeScreenParametersNotification
            object:nil queue:NSOperationQueue.mainQueue usingBlock:refresh];
        id desktop = [space addObserverForName:NSWorkspaceActiveSpaceDidChangeNotification
            object:nil queue:NSOperationQueue.mainQueue usingBlock:refresh];
        id wake = [space addObserverForName:NSWorkspaceScreensDidWakeNotification
            object:nil queue:NSOperationQueue.mainQueue usingBlock:refresh];
        // EOF also ends the helper if the daemon is killed or upgraded.
        dispatch_source_t parent = dispatch_source_create(DISPATCH_SOURCE_TYPE_READ,
            STDIN_FILENO, 0, dispatch_get_main_queue());
        dispatch_source_set_event_handler(parent, ^{
            char byte;
            if (read(STDIN_FILENO, &byte, 1) <= 0) {
                [NSApp stop:nil];
                [NSApp postEvent:[NSEvent otherEventWithType:NSEventTypeApplicationDefined
                    location:NSZeroPoint modifierFlags:0 timestamp:0 windowNumber:0
                    context:nil subtype:0 data1:0 data2:0] atStart:NO];
            }
        });
        dispatch_resume(parent);
        CGError cursor = CGDisplayHideCursor(kCGDirectMainDisplay);
        refresh(nil);
        [NSApp run];
        if (cursor == kCGErrorSuccess) CGDisplayShowCursor(kCGDirectMainDisplay);
        dispatch_source_cancel(parent);
        [app removeObserver:layout];
        [space removeObserver:desktop];
        [space removeObserver:wake];
        for (NSPanel *panel in panels.allValues) [panel orderOut:nil];
        IOPMAssertionRelease(display);
        IOPMAssertionRelease(system);
        return true;
    }
}

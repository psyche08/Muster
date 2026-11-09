// Kernel parent and Launch Services facts for a running GUI-owned host.
#import <AppKit/AppKit.h>
#import <libproc.h>

int muster_regular_app_parent(int pid) {
    @autoreleasepool {
        struct proc_bsdinfo info = {0};
        if (proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info)) != sizeof(info)) return 0;
        if (info.pbi_ppid <= 1) return 0;
        NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:info.pbi_ppid];
        return app && !app.terminated && app.activationPolicy == NSApplicationActivationPolicyRegular
            ? app.processIdentifier : 0;
    }
}

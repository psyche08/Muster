// Run with Console.m, -fobjc-arc and AppKit/ApplicationServices/Security/IOKit.
// Creates only short-lived display assertions; never unlocks or changes settings.
#import <Foundation/Foundation.h>
#import <IOKit/pwr_mgt/IOPMLib.h>
#import <assert.h>
#import <unistd.h>
extern bool muster_console_display_awake_begin(uint64_t, uint32_t *);
extern bool muster_console_wake_end(uint32_t);
static NSUInteger owned_assertions(void) {
    CFDictionaryRef raw = NULL;
    assert(IOPMCopyAssertionsByProcess(&raw) == kIOReturnSuccess);
    NSDictionary *byPID = CFBridgingRelease(raw);
    NSUInteger count = 0;
    for (NSDictionary *entry in byPID[@(getpid())]) {
        if ([entry[(__bridge id)kIOPMAssertionNameKey] isEqual:@"Muster Locked Use display"] &&
            [entry[(__bridge id)kIOPMAssertionTypeKey] isEqual:(__bridge id)kIOPMAssertionTypePreventUserIdleDisplaySleep]) count++;
    }
    return count;
}
int main(void) {
    @autoreleasepool {
        assert(owned_assertions() == 0);
        uint32_t assertion = 0;
        assert(muster_console_display_awake_begin(5000, &assertion));
        assert(owned_assertions() == 1);
        assert(muster_console_wake_end(assertion));
        assert(owned_assertions() == 0);
        assert(muster_console_display_awake_begin(1500, &assertion));
        assert(owned_assertions() == 1);
        // The OS must enforce release without another call from the owner.
        for (unsigned i = 0; i < 50 && owned_assertions() != 0; i++) usleep(100000);
        assert(owned_assertions() == 0);
        puts("display assertion: explicit release and OS timeout passed");
    }
}

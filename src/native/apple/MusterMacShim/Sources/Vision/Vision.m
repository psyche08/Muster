// What a failed native send's look needs from the OS (ADR 0116): a PNG of an
// app's largest on-screen window through ScreenCaptureKit, and musterd's own
// login-keychain item for the owner's LLM API key. Buffers handed to Rust are
// freed with muster_vision_free, which clears them first.
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <Security/Security.h>

enum {
    MusterCaptureOk = 0,
    MusterCaptureNotAllowed = 1,
    MusterCaptureNoWindow = 2,
    MusterCaptureTimedOut = 3,
    MusterCaptureFailed = 4,
};

bool muster_capture_allowed(void) { return CGPreflightScreenCaptureAccess(); }

// Shows the system prompt once; afterwards the person changes it in System
// Settings. Returns whether access is granted now.
bool muster_capture_request(void) { return CGRequestScreenCaptureAccess(); }

// A completion that may arrive after the caller gave up keeps its result here,
// and releases it if nobody is waiting any more.
@interface MusterCaptureBox : NSObject
@property(nonatomic) dispatch_semaphore_t done;
@property(nonatomic, strong) SCShareableContent *content;
@property(nonatomic) CGImageRef image;
@property(nonatomic) BOOL abandoned;
@end

@implementation MusterCaptureBox
- (void)dealloc {
    if (_image) CGImageRelease(_image);
}
@end

static dispatch_time_t deadline(uint32_t timeout_ms) {
    return dispatch_time(DISPATCH_TIME_NOW, (int64_t)timeout_ms * NSEC_PER_MSEC);
}

static void copy_out(NSData *data, uint8_t **out, size_t *len) {
    *out = malloc(data.length);
    if (!*out) return;
    memcpy(*out, data.bytes, data.length);
    *len = data.length;
}

int32_t muster_capture_window_png(const int32_t *pids, size_t count, uint32_t max_side,
                                  uint32_t timeout_ms, uint8_t **out, size_t *len) {
    @autoreleasepool {
        *out = NULL;
        *len = 0;
        if (!CGPreflightScreenCaptureAccess()) return MusterCaptureNotAllowed;
        MusterCaptureBox *box = [MusterCaptureBox new];
        box.done = dispatch_semaphore_create(0);
        [SCShareableContent getShareableContentExcludingDesktopWindows:YES
                                                   onScreenWindowsOnly:YES
                                                     completionHandler:^(SCShareableContent *content,
                                                                         NSError *error) {
            (void)error;
            box.content = content;
            dispatch_semaphore_signal(box.done);
        }];
        if (dispatch_semaphore_wait(box.done, deadline(timeout_ms))) return MusterCaptureTimedOut;
        if (!box.content) return MusterCaptureFailed;
        SCWindow *best = nil;
        CGFloat area = 0;
        for (SCWindow *window in box.content.windows) {
            pid_t owner = window.owningApplication.processID;
            BOOL ours = NO;
            for (size_t i = 0; i < count; i++) ours = ours || pids[i] == owner;
            if (!ours || !window.onScreen || window.windowLayer != 0) continue;
            CGSize size = window.frame.size;
            if (size.width < 100 || size.height < 100) continue;
            if (size.width * size.height > area) {
                area = size.width * size.height;
                best = window;
            }
        }
        if (!best) return MusterCaptureNoWindow;

        SCContentFilter *filter = [[SCContentFilter alloc] initWithDesktopIndependentWindow:best];
        CGFloat scale = filter.pointPixelScale > 0 ? filter.pointPixelScale : 1;
        CGFloat width = filter.contentRect.size.width * scale;
        CGFloat height = filter.contentRect.size.height * scale;
        CGFloat longest = MAX(width, height);
        if (max_side > 0 && longest > max_side) {
            width = width * max_side / longest;
            height = height * max_side / longest;
        }
        SCStreamConfiguration *config = [SCStreamConfiguration new];
        config.width = (size_t)MAX(1, width);
        config.height = (size_t)MAX(1, height);
        config.showsCursor = NO;
        config.ignoreShadowsSingleWindow = YES;

        MusterCaptureBox *shot = [MusterCaptureBox new];
        shot.done = dispatch_semaphore_create(0);
        [SCScreenshotManager captureImageWithFilter:filter
                                      configuration:config
                                  completionHandler:^(CGImageRef image, NSError *error) {
            (void)error;
            @synchronized(shot) {
                if (image && !shot.abandoned) shot.image = CGImageRetain(image);
            }
            dispatch_semaphore_signal(shot.done);
        }];
        if (dispatch_semaphore_wait(shot.done, deadline(timeout_ms))) {
            @synchronized(shot) {
                shot.abandoned = YES;
            }
            return MusterCaptureTimedOut;
        }
        if (!shot.image) return MusterCaptureFailed;

        NSMutableData *png = [NSMutableData data];
        CGImageDestinationRef destination = CGImageDestinationCreateWithData(
            (__bridge CFMutableDataRef)png, CFSTR("public.png"), 1, NULL);
        if (!destination) return MusterCaptureFailed;
        CGImageDestinationAddImage(destination, shot.image, NULL);
        BOOL encoded = CGImageDestinationFinalize(destination);
        CFRelease(destination);
        if (!encoded || png.length == 0) return MusterCaptureFailed;
        copy_out(png, out, len);
        return *out ? MusterCaptureOk : MusterCaptureFailed;
    }
}

void muster_vision_free(uint8_t *buffer, size_t len) {
    if (!buffer) return;
    memset_s(buffer, len, 0, len);
    free(buffer);
}

// The file-based login keychain, not the data-protection keychain: musterd has
// no keychain access group. The item's access list trusts the process that
// created it, so only musterd reads it back without a prompt.
static NSMutableDictionary *item_query(const char *service, const char *account) {
    return [@{
        (__bridge id)kSecClass : (__bridge id)kSecClassGenericPassword,
        (__bridge id)kSecAttrService : @(service),
        (__bridge id)kSecAttrAccount : @(account),
    } mutableCopy];
}

int32_t muster_secret_set(const char *service, const char *account, const uint8_t *secret,
                          size_t len) {
    @autoreleasepool {
        if (!service || !account || !secret) return errSecParam;
        NSData *data = [NSData dataWithBytes:secret length:len];
        NSMutableDictionary *query = item_query(service, account);
        OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query,
                                        (__bridge CFDictionaryRef) @{(__bridge id)kSecValueData : data});
        if (status != errSecItemNotFound) return status;
        query[(__bridge id)kSecValueData] = data;
        query[(__bridge id)kSecAttrLabel] = @"Muster vision API key";
        return SecItemAdd((__bridge CFDictionaryRef)query, NULL);
    }
}

// Whether the item exists, without reading the secret (no access prompt).
int32_t muster_secret_exists(const char *service, const char *account) {
    @autoreleasepool {
        if (!service || !account) return errSecParam;
        NSMutableDictionary *query = item_query(service, account);
        query[(__bridge id)kSecReturnAttributes] = @YES;
        query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
        CFTypeRef found = NULL;
        OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &found);
        if (found) CFRelease(found);
        return status;
    }
}

// `interactive` false fails instead of showing an access prompt nobody may
// be at the Mac to answer.
int32_t muster_secret_get(const char *service, const char *account, bool interactive,
                          uint8_t **out, size_t *len) {
    @autoreleasepool {
        *out = NULL;
        *len = 0;
        if (!service || !account) return errSecParam;
        NSMutableDictionary *query = item_query(service, account);
        query[(__bridge id)kSecReturnData] = @YES;
        query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
        if (!interactive) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
            query[(__bridge id)kSecUseAuthenticationUI] = (__bridge id)kSecUseAuthenticationUIFail;
#pragma clang diagnostic pop
        }
        CFTypeRef found = NULL;
        OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &found);
        if (status != errSecSuccess) return status;
        NSData *data = CFBridgingRelease(found);
        if (![data isKindOfClass:NSData.class]) return errSecDecode;
        copy_out(data, out, len);
        return *out ? errSecSuccess : errSecAllocate;
    }
}

int32_t muster_secret_delete(const char *service, const char *account) {
    @autoreleasepool {
        if (!service || !account) return errSecParam;
        return SecItemDelete((__bridge CFDictionaryRef)item_query(service, account));
    }
}

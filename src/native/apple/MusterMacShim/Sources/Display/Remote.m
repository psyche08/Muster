// Native desktop profile. One retained display filter per authorized session.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <ImageIO/ImageIO.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>

@interface MusterRemoteDisplay : NSObject
@property(nonatomic, strong) SCContentFilter *filter;
@property(nonatomic) double quality;
@property(nonatomic) CGDirectDisplayID displayID;
@property(nonatomic, strong) SCStreamConfiguration *configuration;
@end
@implementation MusterRemoteDisplay
@end

void *muster_remote_display_open(uint32_t edge, uint32_t quality, bool cursor) {
  @autoreleasepool {
    if (!CGPreflightScreenCaptureAccess())
      return NULL;
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block SCDisplay *display = nil;
    CGDirectDisplayID main = CGMainDisplayID();
    [SCShareableContent
        getShareableContentExcludingDesktopWindows:NO
                               onScreenWindowsOnly:YES
                                 completionHandler:^(
                                     SCShareableContent *content,
                                     NSError *error) {
                                   for (SCDisplay *item in content.displays)
                                     if (item.displayID == main)
                                       display = item;
                                   dispatch_semaphore_signal(done);
                                 }];
    if (dispatch_semaphore_wait(
            done, dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC)) ||
        !display)
      return NULL;
    MusterRemoteDisplay *view = [MusterRemoteDisplay new];
    view.displayID = main;
    view.filter = [[SCContentFilter alloc] initWithDisplay:display
                                          excludingWindows:@[]];
    view.configuration = [SCStreamConfiguration new];
    CGSize points = view.filter.contentRect.size;
    double scale = MIN(view.filter.pointPixelScale, edge / MAX(points.width, points.height));
    view.configuration.width = MAX(1, round(points.width * scale));
    view.configuration.height = MAX(1, round(points.height * scale));
    view.configuration.showsCursor = cursor;
    view.quality = quality / 100.0;
    return (__bridge_retained void *)view;
  }
}

// Bound each frame even for a high-detail desktop at the largest resolution.
static NSData *encodeFrame(CGImageRef image, double quality, size_t cap) {
  size_t edge = MAX(CGImageGetWidth(image), CGImageGetHeight(image));
  for (;;) {
    NSMutableData *data = [NSMutableData data];
    CGImageDestinationRef dest = CGImageDestinationCreateWithData(
        (__bridge CFMutableDataRef)data, CFSTR("public.jpeg"), 1, NULL);
    if (!dest)
      return nil;
    CGImageDestinationAddImage(dest, image, (__bridge CFDictionaryRef) @{
      (__bridge NSString *)kCGImageDestinationLossyCompressionQuality : @(quality),
      (__bridge NSString *)kCGImageDestinationImageMaxPixelSize : @(edge)
    });
    bool encoded = CGImageDestinationFinalize(dest);
    CFRelease(dest);
    if (!encoded)
      return nil;
    if (data.length <= cap)
      return data;
    if (quality > 0) {
      quality = MAX(0, quality - 0.20);
    } else if (edge > 640) {
      edge /= 2;
    } else {
      return nil;
    }
  }
}

size_t muster_remote_frame(void *handle, uint8_t *out, size_t cap) {
  @autoreleasepool {
    MusterRemoteDisplay *view = (__bridge MusterRemoteDisplay *)handle;
    dispatch_semaphore_t done = dispatch_semaphore_create(0);
    __block NSData *frame = nil;
    [SCScreenshotManager
        captureImageWithFilter:view.filter
                 configuration:view.configuration
             completionHandler:^(CGImageRef image, NSError *error) {
               if (image)
                 frame = encodeFrame(image, view.quality, cap);
               dispatch_semaphore_signal(done);
             }];
    if (dispatch_semaphore_wait(
            done, dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC)) ||
        !frame.length || frame.length > cap)
      return 0;
    memcpy(out, frame.bytes, frame.length);
    return frame.length;
  }
}

void muster_remote_close(void *handle) { CFBridgingRelease(handle); }

static void mouse(CGEventType type, CGPoint point, CGMouseButton button) {
  CGEventRef e = CGEventCreateMouseEvent(NULL, type, point, button);
  if (e) {
    CGEventPost(kCGHIDEventTap, e);
    CFRelease(e);
  }
}
bool muster_remote_pointer(void *handle, int kind, uint16_t x, uint16_t y,
                           int scroll) {
  if (!AXIsProcessTrusted())
    return false;
  CGRect r =
      CGDisplayBounds(((__bridge MusterRemoteDisplay *)handle).displayID);
  CGPoint p = CGPointMake(r.origin.x + x * (r.size.width - 1) / 65535.,
                          r.origin.y + y * (r.size.height - 1) / 65535.);
  switch (kind) {
  case 0:
    mouse(kCGEventMouseMoved, p, kCGMouseButtonLeft);
    break;
  case 1:
    mouse(kCGEventLeftMouseDown, p, kCGMouseButtonLeft);
    mouse(kCGEventLeftMouseUp, p, kCGMouseButtonLeft);
    break;
  case 2:
    mouse(kCGEventRightMouseDown, p, kCGMouseButtonRight);
    mouse(kCGEventRightMouseUp, p, kCGMouseButtonRight);
    break;
  case 3: {
    CGEventRef e =
        CGEventCreateScrollWheelEvent(NULL, kCGScrollEventUnitLine, 1, scroll);
    if (!e)
      return false;
    CGEventSetLocation(e, p);
    CGEventPost(kCGHIDEventTap, e);
    CFRelease(e);
    break;
  }
  case 4:
    mouse(kCGEventLeftMouseDown, p, kCGMouseButtonLeft);
    break;
  case 5:
    mouse(kCGEventLeftMouseDragged, p, kCGMouseButtonLeft);
    break;
  case 6:
    mouse(kCGEventLeftMouseUp, p, kCGMouseButtonLeft);
    break;
  default:
    return false;
  }
  return true;
}

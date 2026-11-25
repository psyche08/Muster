#import "screen_video.h"
#import <AppKit/AppKit.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>

@interface MusterScreenVideo () <SCStreamDelegate, SCStreamOutput>
@end

@implementation MusterScreenVideo {
  SCDisplay *_display;
  NSArray<SCWindow *> *_lockWindows;
  int _fps;
  SCStream *_stream;
  NSCondition *_condition;
  CMSampleBufferRef _latest;
  BOOL _ended;
  NSInteger _status;
}

// The lock filter contains only the system login process's visible windows on
// this display. A full-display login window must be present to select it.
static BOOL isSystemLoginProcess(pid_t pid) {
  NSRunningApplication *app = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
  return [app.bundleURL.path isEqualToString:@"/System/Library/CoreServices/loginwindow.app"];
}

static NSArray<SCWindow *> *loginSurface(SCShareableContent *content, SCDisplay *display) {
  NSMutableArray *windows = [NSMutableArray new];
  BOOL coversDisplay = NO;
  for (SCWindow *window in content.windows) {
    SCRunningApplication *owner = window.owningApplication;
    if (!window.onScreen || ![owner.bundleIdentifier isEqualToString:@"com.apple.loginwindow"] ||
        !CGRectIntersectsRect(window.frame, display.frame)) continue;
    if (!isSystemLoginProcess(owner.processID)) continue;
    [windows addObject:window];
    coversDisplay |= CGRectContainsRect(window.frame, display.frame);
  }
  return coversDisplay ? windows : @[];
}

- (instancetype)initWithDisplay:(CGDirectDisplayID)displayID frameRate:(int)fps {
  if (!(self = [super init]) || !CGPreflightScreenCaptureAccess()) return nil;
  dispatch_semaphore_t ready = dispatch_semaphore_create(0);
  __block SCShareableContent *available;
  [SCShareableContent getShareableContentExcludingDesktopWindows:NO onScreenWindowsOnly:YES
      completionHandler:^(SCShareableContent *content, NSError *error) {
        available = content;
        dispatch_semaphore_signal(ready);
      }];
  if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC))) return nil;
  for (SCDisplay *candidate in available.displays) {
    if (candidate.displayID == displayID) _display = candidate;
  }
  if (!_display) return nil;
  _lockWindows = loginSurface(available, _display);
  CGDisplayModeRef mode = CGDisplayCopyDisplayMode(displayID);
  if (!mode) return nil;
  _frameWidth = (int)CGDisplayModeGetPixelWidth(mode);
  _frameHeight = (int)CGDisplayModeGetPixelHeight(mode);
  CGDisplayModeRelease(mode);
  _fps = MAX(1, fps);
  _pixelFormat = kCVPixelFormatType_32BGRA;
  _condition = [NSCondition new];
  _status = -1;
  return self;
}

- (BOOL)lockSurfaceAvailable { return _lockWindows.count != 0; }
- (NSInteger)frameStatus {
  [_condition lock];
  NSInteger value = _status;
  [_condition unlock];
  return value;
}

- (BOOL)startLockedSurface:(BOOL)locked {
  if (locked && !self.lockSurfaceAvailable) return NO;
  SCContentFilter *filter = locked ?
      [[SCContentFilter alloc] initWithDisplay:_display includingWindows:_lockWindows] :
      [[SCContentFilter alloc] initWithDisplay:_display excludingWindows:@[]];
  SCStreamConfiguration *config = [SCStreamConfiguration new];
  config.width = self.frameWidth;
  config.height = self.frameHeight;
  config.pixelFormat = self.pixelFormat;
  config.minimumFrameInterval = CMTimeMake(1, _fps);
  config.preservesAspectRatio = YES;
  extern bool display_cursor;
  config.showsCursor = display_cursor;
  config.queueDepth = 5;
  SCStream *stream = [[SCStream alloc] initWithFilter:filter configuration:config delegate:self];
  dispatch_queue_t queue = dispatch_queue_create("muster.screen.video", DISPATCH_QUEUE_SERIAL);
  NSError *error = nil;
  if (![stream addStreamOutput:self type:SCStreamOutputTypeScreen sampleHandlerQueue:queue error:&error]) return NO;
  [_condition lock];
  _ended = NO;
  _status = -1;
  _stream = stream;
  [_condition unlock];
  dispatch_semaphore_t ready = dispatch_semaphore_create(0);
  __block BOOL started = NO;
  [stream startCaptureWithCompletionHandler:^(NSError *startError) {
    started = startError == nil;
    dispatch_semaphore_signal(ready);
  }];
  if (dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 3 * NSEC_PER_SEC)) || !started) {
    [self stop];
    return NO;
  }
  return YES;
}

- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
  if (type != SCStreamOutputTypeScreen) return;
  NSArray *attachments = (__bridge NSArray *)CMSampleBufferGetSampleAttachmentsArray(sample, false);
  NSNumber *status = attachments.firstObject[SCStreamFrameInfoStatus];
  [_condition lock];
  if (stream == _stream && !_ended && status) {
    _status = status.integerValue;
    switch (_status) {
    case SCFrameStatusComplete:
    case SCFrameStatusStarted:
      if (CMSampleBufferDataIsReady(sample) && CMSampleBufferGetImageBuffer(sample)) {
        if (_latest) CFRelease(_latest);
        _latest = (CMSampleBufferRef)CFRetain(sample);
      }
      break;
    case SCFrameStatusIdle:
      break;
    default:
      _ended = YES;
      break;
    }
    [_condition signal];
  }
  [_condition unlock];
}

- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
  [_condition lock];
  if (stream == _stream) {
    _ended = YES;
    [_condition signal];
  }
  [_condition unlock];
}

- (CMSampleBufferRef)copyFrameWaiting:(NSTimeInterval)seconds ended:(BOOL *)ended {
  NSDate *deadline = [NSDate dateWithTimeIntervalSinceNow:seconds];
  [_condition lock];
  while (!_latest && !_ended && [_condition waitUntilDate:deadline]) {}
  *ended = _ended;
  CMSampleBufferRef sample = _ended ? NULL : _latest;
  if (sample) _latest = NULL;
  [_condition unlock];
  return sample;
}

- (void)stop {
  [_condition lock];
  SCStream *stream = _stream;
  _stream = nil;
  _ended = YES;
  if (_latest) CFRelease(_latest);
  _latest = NULL;
  [_condition broadcast];
  [_condition unlock];
  [stream stopCaptureWithCompletionHandler:^(NSError *error) {}];
}
@end

// Check the current pointer target again when dispatching a locked-session
// event. A secure prompt or another display can cover the sampled login window.
bool muster_login_pointer_target(void) {
  @autoreleasepool {
    CGEventRef event = CGEventCreate(NULL);
    if (!event) return false;
    CGPoint point = CGEventGetLocation(event);
    CFRelease(event);
    NSArray *windows = CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID));
    for (NSDictionary *window in windows) {
      CGRect bounds;
      if ([window[(__bridge id)kCGWindowAlpha] doubleValue] <= 0 ||
          !CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)window[(__bridge id)kCGWindowBounds], &bounds) ||
          !CGRectContainsPoint(bounds, point)) continue;
      return isSystemLoginProcess([window[(__bridge id)kCGWindowOwnerPID] intValue]);
    }
    return false;
  }
}

// Local CLI diagnostic. Samples the physical display, returning metadata only.
int muster_screen_video_sample(void) {
  @autoreleasepool {
    CGDirectDisplayID id = CGMainDisplayID();
    printf("backend=ScreenCaptureKit online=%d active=%d asleep=%d screen_recording=%d\n",
           CGDisplayIsOnline(id), CGDisplayIsActive(id), CGDisplayIsAsleep(id), CGPreflightScreenCaptureAccess());
    MusterScreenVideo *video = [[MusterScreenVideo alloc] initWithDisplay:id frameRate:10];
    if (!video) { puts("sample=display-unavailable"); return 0; }
    printf("login_surface=%d\n", video.lockSurfaceAvailable);
    if (![video startLockedSurface:NO]) { puts("sample=start-failed"); return 0; }
    BOOL ended = NO;
    CMSampleBufferRef sample = [video copyFrameWaiting:4 ended:&ended];
    if (sample) {
      CVImageBufferRef image = CMSampleBufferGetImageBuffer(sample);
      printf("sample=fresh size=%zux%zu\n", CVPixelBufferGetWidth(image), CVPixelBufferGetHeight(image));
      CFRelease(sample);
    } else {
      printf("sample=%s frame_status=%ld\n", ended ? "ended" : "no-frame", (long)video.frameStatus);
    }
    [video stop];
    return 0;
  }
}

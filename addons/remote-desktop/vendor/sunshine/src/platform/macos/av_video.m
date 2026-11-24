/**
 * @file src/platform/macos/av_video.m
 * @brief Definitions for video capture on macOS.
 */
// local includes
#import "av_video.h"

@implementation AVVideo

- (id)initWithDisplay:(CGDirectDisplayID)displayID frameRate:(int)frameRate {
  self = [super init];

  CGDisplayModeRef mode = CGDisplayCopyDisplayMode(displayID);
  if (!mode) {
    [self release];
    return nil;
  }

  self.displayID = displayID;
  self.pixelFormat = kCVPixelFormatType_32BGRA;
  self.frameWidth = (int) CGDisplayModeGetPixelWidth(mode);
  self.frameHeight = (int) CGDisplayModeGetPixelHeight(mode);
  self.minFrameDuration = CMTimeMake(1, frameRate);
  self.session = [[AVCaptureSession alloc] init];
  self.videoOutputs = [[NSMapTable alloc] init];
  self.captureCallbacks = [[NSMapTable alloc] init];
  self.captureSignals = [[NSMapTable alloc] init];

  CFRelease(mode);

  AVCaptureScreenInput *screenInput = [[AVCaptureScreenInput alloc] initWithDisplayID:self.displayID];
  extern bool display_cursor;
  screenInput.capturesCursor = display_cursor;
  [screenInput setMinFrameDuration:self.minFrameDuration];

  if (screenInput && [self.session canAddInput:screenInput]) {
    [self.session addInput:screenInput];
  } else {
    [screenInput release];
    return nil;
  }

  [self.session startRunning];

  return self;
}

- (void)dealloc {
  [self.videoOutputs release];
  [self.captureCallbacks release];
  [self.captureSignals release];
  [self.session stopRunning];
  [super dealloc];
}

- (void)setFrameWidth:(int)frameWidth frameHeight:(int)frameHeight {
  self.frameWidth = frameWidth;
  self.frameHeight = frameHeight;
}

- (dispatch_semaphore_t)capture:(FrameCallbackBlock)frameCallback {
  @synchronized(self) {
    AVCaptureVideoDataOutput *videoOutput = [[AVCaptureVideoDataOutput alloc] init];

    [videoOutput setVideoSettings:@{
      (NSString *) kCVPixelBufferPixelFormatTypeKey: [NSNumber numberWithUnsignedInt:self.pixelFormat],
      (NSString *) kCVPixelBufferWidthKey: [NSNumber numberWithInt:self.frameWidth],
      (NSString *) kCVPixelBufferHeightKey: [NSNumber numberWithInt:self.frameHeight],
      (NSString *) AVVideoScalingModeKey: AVVideoScalingModeResizeAspect,
    }];

    dispatch_queue_attr_t qos = dispatch_queue_attr_make_with_qos_class(DISPATCH_QUEUE_SERIAL, QOS_CLASS_USER_INITIATED, DISPATCH_QUEUE_PRIORITY_HIGH);
    dispatch_queue_t recordingQueue = dispatch_queue_create("videoCaptureQueue", qos);
    [videoOutput setSampleBufferDelegate:self queue:recordingQueue];

    [self.session stopRunning];

    if ([self.session canAddOutput:videoOutput]) {
      [self.session addOutput:videoOutput];
    } else {
      [videoOutput release];
      return nil;
    }

    AVCaptureConnection *videoConnection = [videoOutput connectionWithMediaType:AVMediaTypeVideo];
    dispatch_semaphore_t signal = dispatch_semaphore_create(0);

    [self.videoOutputs setObject:videoOutput forKey:videoConnection];
    [self.captureCallbacks setObject:frameCallback forKey:videoConnection];
    [self.captureSignals setObject:signal forKey:videoConnection];

    [self.session startRunning];

    return signal;
  }
}

- (void)captureOutput:(AVCaptureOutput *)captureOutput
  didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
         fromConnection:(AVCaptureConnection *)connection {
  FrameCallbackBlock callback = [self.captureCallbacks objectForKey:connection];

  if (callback != nil) {
    if (!callback(sampleBuffer)) {
      @synchronized(self) {
        [self.session stopRunning];
        [self.captureCallbacks removeObjectForKey:connection];
        [self.session removeOutput:[self.videoOutputs objectForKey:connection]];
        [self.videoOutputs removeObjectForKey:connection];
        dispatch_semaphore_signal([self.captureSignals objectForKey:connection]);
        [self.captureSignals removeObjectForKey:connection];
        [self.session startRunning];
      }
    }
  }
}

@end

// Local operator diagnostic. Use the same display input and sample callback as
// streaming, but do not map, encode or return pixel data.
int muster_display_sample(void) {
  @autoreleasepool {
    CGDirectDisplayID display = CGMainDisplayID();
    bool permitted = CGPreflightScreenCaptureAccess();
    printf("online=%d active=%d asleep=%d screen_recording=%d\n",
           CGDisplayIsOnline(display), CGDisplayIsActive(display),
           CGDisplayIsAsleep(display), permitted);
    fflush(stdout);
    if (!permitted || !CGDisplayIsOnline(display)) {
      puts("sample=unavailable");
      return 0;
    }
    AVVideo *video = [[AVVideo alloc] initWithDisplay:display frameRate:10];
    if (!video) {
      puts("sample=no-input");
      return 0;
    }
    dispatch_semaphore_t ready = dispatch_semaphore_create(0);
    __block size_t width = 0, height = 0;
    dispatch_semaphore_t capturing = [video capture:^bool(CMSampleBufferRef sample) {
      CVImageBufferRef image = CMSampleBufferGetImageBuffer(sample);
      if (width == 0 && image && CMSampleBufferDataIsReady(sample)) {
        width = CVPixelBufferGetWidth(image);
        height = CVPixelBufferGetHeight(image);
        dispatch_semaphore_signal(ready);
      }
      return true;
    }];
    bool received = capturing && dispatch_semaphore_wait(ready, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC)) == 0;
    [video.session stopRunning];
    if (received) {
      printf("sample=fresh size=%zux%zu\n", width, height);
    } else {
      puts("sample=no-frame");
    }
    [video release];
    return 0;
  }
}

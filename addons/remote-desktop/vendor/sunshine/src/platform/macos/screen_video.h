#pragma once
#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <CoreMedia/CoreMedia.h>

@interface MusterScreenVideo : NSObject
@property(nonatomic) int frameWidth;
@property(nonatomic) int frameHeight;
@property(nonatomic) OSType pixelFormat;
@property(nonatomic, readonly) BOOL lockSurfaceAvailable;
@property(nonatomic, readonly) NSInteger frameStatus;

- (instancetype)initWithDisplay:(CGDirectDisplayID)displayID frameRate:(int)fps;
- (BOOL)startLockedSurface:(BOOL)locked;
- (void)stop;
// Returns a retained sample, nil on timeout, or sets ended on stream failure.
- (CMSampleBufferRef)copyFrameWaiting:(NSTimeInterval)seconds ended:(BOOL *)ended;
@end

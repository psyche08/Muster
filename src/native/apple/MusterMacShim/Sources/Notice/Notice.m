// A small notification-style panel at the top right of the main display.
// It never activates, takes focus or receives clicks, so it cannot change
// where the person's keyboard or mouse input goes.
#import <AppKit/AppKit.h>

static NSPanel *notice;

static NSTextField *label(NSString *text, NSFont *font, NSColor *color) {
    NSTextField *field = [NSTextField labelWithString:text];
    field.font = font;
    field.textColor = color;
    field.lineBreakMode = NSLineBreakByTruncatingTail;
    field.maximumNumberOfLines = 1;
    field.translatesAutoresizingMaskIntoConstraints = NO;
    return field;
}

bool muster_notice_show(const char *title, const char *body) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        if (notice || !title || !body) return false;
        NSScreen *screen = NSScreen.mainScreen ?: NSScreen.screens.firstObject;
        if (!screen) return false;
        const CGFloat width = 340, height = 64, margin = 12;
        NSRect visible = screen.visibleFrame;
        NSRect frame = NSMakeRect(NSMaxX(visible) - width - margin,
                                  NSMaxY(visible) - height - margin, width, height);
        NSPanel *panel = [[NSPanel alloc] initWithContentRect:frame
            styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
            backing:NSBackingStoreBuffered defer:NO];
        panel.level = NSStatusWindowLevel;
        panel.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
            NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorStationary;
        panel.ignoresMouseEvents = YES;
        panel.hasShadow = YES;
        panel.opaque = NO;
        panel.backgroundColor = NSColor.clearColor;
        panel.releasedWhenClosed = NO;

        NSVisualEffectView *background = [[NSVisualEffectView alloc]
            initWithFrame:NSMakeRect(0, 0, width, height)];
        background.material = NSVisualEffectMaterialPopover;
        background.blendingMode = NSVisualEffectBlendingModeBehindWindow;
        background.state = NSVisualEffectStateActive;
        background.wantsLayer = YES;
        background.layer.cornerRadius = 12;
        background.layer.masksToBounds = YES;
        panel.contentView = background;

        NSTextField *heading = label([NSString stringWithUTF8String:title] ?: @"",
            [NSFont boldSystemFontOfSize:13], NSColor.labelColor);
        NSTextField *detail = label([NSString stringWithUTF8String:body] ?: @"",
            [NSFont systemFontOfSize:12], NSColor.secondaryLabelColor);
        [background addSubview:heading];
        [background addSubview:detail];
        [NSLayoutConstraint activateConstraints:@[
            [heading.leadingAnchor constraintEqualToAnchor:background.leadingAnchor constant:16],
            [heading.trailingAnchor constraintEqualToAnchor:background.trailingAnchor constant:-16],
            [heading.topAnchor constraintEqualToAnchor:background.topAnchor constant:13],
            [detail.leadingAnchor constraintEqualToAnchor:heading.leadingAnchor],
            [detail.trailingAnchor constraintEqualToAnchor:heading.trailingAnchor],
            [detail.topAnchor constraintEqualToAnchor:heading.bottomAnchor constant:3],
        ]];
        [panel orderFrontRegardless];
        notice = panel;
        return true;
    }
}

// OS adapters only: the black gamma table of the Locked Use cover (ADR 0223).
// Rust decides which display is blackened and which gets the black panel.
//
// A display's transfer table maps every pixel value to an output level. An
// all-zero table shows every pixel black without changing what the window
// server composites, so no window is occluded. No request goes to the display
// itself. macOS puts the ColorSync tables back when the process that changed
// them ends, however it ends.
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <math.h>

// Never more than this many entries are read back.
#define MUSTER_GAMMA_ENTRIES 4096
// A table entry at or below this is black.
#define MUSTER_GAMMA_BLACK (1.0f / 1024.0f)

uint32_t muster_display_online(uint32_t *displays, uint32_t capacity) {
    CGDisplayCount count = 0;
    if (CGGetOnlineDisplayList(capacity, displays, &count) != kCGErrorSuccess) return 0;
    return count;
}

// The display a mirror set shows on, which is where its panel goes.
uint32_t muster_display_mirror_master(uint32_t display) {
    CGDirectDisplayID master = CGDisplayMirrorsDisplay(display);
    return master == kCGNullDirectDisplay ? display : master;
}

bool muster_display_asleep(uint32_t display) {
    return CGDisplayIsAsleep(display);
}

// The tables below are static, too large for the stack; the watchdog calls
// these from its main thread only.
typedef struct {
    uint32_t count;
    CGGammaValue red[MUSTER_GAMMA_ENTRIES], green[MUSTER_GAMMA_ENTRIES], blue[MUSTER_GAMMA_ENTRIES];
} MusterGammaTable;

static bool read_table(uint32_t display, MusterGammaTable *table) {
    table->count = 0;
    return CGGetDisplayTransferByTable(display, MUSTER_GAMMA_ENTRIES, table->red, table->green,
                                       table->blue, &table->count) == kCGErrorSuccess &&
        table->count > 0;
}

// Whether the display's table maps every value to black.
bool muster_gamma_black_holds(uint32_t display) {
    static MusterGammaTable table;
    if (!read_table(display, &table)) return false;
    for (uint32_t i = 0; i < table.count; i++) {
        if (fmaxf(table.red[i], fmaxf(table.green[i], table.blue[i])) > MUSTER_GAMMA_BLACK) return false;
    }
    return true;
}

// Sets an all-zero table and reads it back. A display that does not read back
// black gets its previous table again, and false.
bool muster_gamma_black(uint32_t display) {
    static MusterGammaTable previous;
    if (!read_table(display, &previous)) return false;
    static const CGGammaValue zero[256] = {0};
    if (CGSetDisplayTransferByTable(display, 256, zero, zero, zero) == kCGErrorSuccess &&
        muster_gamma_black_holds(display)) {
        return true;
    }
    CGSetDisplayTransferByTable(display, previous.count, previous.red, previous.green, previous.blue);
    return false;
}

// Every display's ColorSync table, as the user set it.
void muster_gamma_restore(void) {
    CGDisplayRestoreColorSyncSettings();
}

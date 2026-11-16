// OS adapters only: display brightness for the Locked Use cover (ADR 0222).
// Rust decides which display is dimmed, which gets the black panel, and when
// a level is restored.
//
// macOS sets the brightness of its own displays (built-in, Apple external)
// through DisplayServices. Other external displays are reached over DDC/CI,
// VCP 0x10, the way m1ddc does it on Apple silicon: the display's
// IOMobileFramebuffer, then the external DCPAVServiceProxy under it, then
// IOAVService I2C at chip address 0x37 (0xB7 behind an MCDP29xx bridge).
// Every private symbol is looked up at run time, so a macOS without one
// leaves that display to the black panel instead of failing to load.
#import <ColorSync/ColorSync.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <dlfcn.h>
#import <stdlib.h>
#import <unistd.h>

typedef CFTypeRef IOAVServiceRef;

static struct {
    IOAVServiceRef (*create)(CFAllocatorRef, io_service_t);
    IOReturn (*read)(IOAVServiceRef, uint32_t, uint32_t, void *, uint32_t);
    IOReturn (*write)(IOAVServiceRef, uint32_t, uint32_t, void *, uint32_t);
    CFDictionaryRef (*info)(CGDirectDisplayID);
    bool (*can)(CGDirectDisplayID);
    int (*get)(CGDirectDisplayID, float *);
    int (*set)(CGDirectDisplayID, float);
} api;

static void load(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        void *iokit = dlopen("/System/Library/Frameworks/IOKit.framework/IOKit", RTLD_NOW | RTLD_LOCAL);
        if (iokit) {
            api.create = dlsym(iokit, "IOAVServiceCreateWithService");
            api.read = dlsym(iokit, "IOAVServiceReadI2C");
            api.write = dlsym(iokit, "IOAVServiceWriteI2C");
        }
        void *core = dlopen("/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay", RTLD_NOW | RTLD_LOCAL);
        if (core) api.info = dlsym(core, "CoreDisplay_DisplayCreateInfoDictionary");
        void *services = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW | RTLD_LOCAL);
        if (services) {
            api.can = dlsym(services, "DisplayServicesCanChangeBrightness");
            api.get = dlsym(services, "DisplayServicesGetBrightness");
            api.set = dlsym(services, "DisplayServicesSetBrightness");
        }
    });
}

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

// ColorSync's display lookups assert that the process has already connected
// to the window server, and they do not connect it themselves. A CoreGraphics
// display call does. Without one first, the restore helper aborted.
static void connect_window_server(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{ (void)CGMainDisplayID(); });
}

// The UUID macOS keeps for a display across reconnects and restarts.
bool muster_display_uuid(uint32_t display, char *out, size_t size) {
    connect_window_server();
    CFUUIDRef uuid = CGDisplayCreateUUIDFromDisplayID(display);
    if (!uuid) return false;
    CFStringRef text = CFUUIDCreateString(kCFAllocatorDefault, uuid);
    CFRelease(uuid);
    if (!text) return false;
    bool ok = CFStringGetCString(text, out, (CFIndex)size, kCFStringEncodingASCII);
    CFRelease(text);
    return ok;
}

bool muster_display_asleep(uint32_t display) {
    connect_window_server();
    return CGDisplayIsAsleep(display);
}

uint32_t muster_display_from_uuid(const char *text) {
    connect_window_server();
    CFStringRef string = CFStringCreateWithCString(kCFAllocatorDefault, text, kCFStringEncodingASCII);
    if (!string) return 0;
    CFUUIDRef uuid = CFUUIDCreateFromString(kCFAllocatorDefault, string);
    CFRelease(string);
    if (!uuid) return 0;
    uint32_t display = CGDisplayGetDisplayIDFromUUID(uuid);
    CFRelease(uuid);
    return display;
}

bool muster_native_brightness_get(uint32_t display, float *value) {
    load();
    return api.can && api.get && api.can(display) && api.get(display, value) == 0;
}

bool muster_native_brightness_set(uint32_t display, float value) {
    load();
    return api.can && api.set && api.can(display) && api.set(display, value) == 0;
}

typedef struct {
    IOAVServiceRef service;
    uint32_t chip;
} MusterDdc;

static bool external_proxy(io_service_t proxy) {
    CFTypeRef location = IORegistryEntryCreateCFProperty(proxy, CFSTR("Location"), kCFAllocatorDefault, 0);
    bool external = location && CFGetTypeID(location) == CFStringGetTypeID() &&
        CFStringCompare(location, CFSTR("External"), 0) == kCFCompareEqualTo;
    if (location) CFRelease(location);
    return external;
}

static uint32_t proxy_chip(io_service_t proxy) {
    io_registry_entry_t parent = MACH_PORT_NULL;
    if (IORegistryEntryGetParentEntry(proxy, kIOServicePlane, &parent) != KERN_SUCCESS) return 0x37;
    CFTypeRef provider = IORegistryEntryCreateCFProperty(parent, CFSTR("EPICProviderClass"), kCFAllocatorDefault, 0);
    bool bridge = provider && CFGetTypeID(provider) == CFStringGetTypeID() &&
        CFStringCompare(provider, CFSTR("AppleDCPMCDP29XX"), 0) == kCFCompareEqualTo;
    if (provider) CFRelease(provider);
    IOObjectRelease(parent);
    return bridge ? 0xB7 : 0x37;
}

// Where the display's framebuffer sits in the I/O Registry. CoreDisplay is a
// display call and runs on the main thread; muster_ddc_open needs only IOKit.
bool muster_ddc_location(uint32_t display, char *out, size_t size) {
    load();
    if (!api.create || !api.read || !api.write || !api.info || CGDisplayIsBuiltin(display)) return false;
    NSDictionary *info = CFBridgingRelease(api.info(display));
    NSString *location = info[@"IODisplayLocation"];
    if (![location isKindOfClass:NSString.class]) return false;
    return [location getCString:out maxLength:size encoding:NSUTF8StringEncoding];
}

// The external DDC channel of the framebuffer at `location`, or NULL when it
// has none. A waking display creates its I2C service again: the old handle
// stops answering, and this finds the new one once it exists. Any thread.
void *muster_ddc_open(const char *location) {
    load();
    if (!api.create || !location) return NULL;
    CFStringRef path = CFStringCreateWithCString(kCFAllocatorDefault, location, kCFStringEncodingUTF8);
    if (!path) return NULL;
    io_registry_entry_t adapter = IORegistryEntryCopyFromPath(kIOMainPortDefault, path);
    CFRelease(path);
    if (adapter == MACH_PORT_NULL) return NULL;
    uint64_t wanted = 0;
    kern_return_t found_adapter = IORegistryEntryGetRegistryEntryID(adapter, &wanted);
    IOObjectRelease(adapter);
    if (found_adapter != KERN_SUCCESS) return NULL;
    io_registry_entry_t root = IORegistryGetRootEntry(kIOMainPortDefault);
    io_iterator_t entries = MACH_PORT_NULL;
    kern_return_t iterating = IORegistryEntryCreateIterator(root, kIOServicePlane,
        kIORegistryIterateRecursively, &entries);
    IOObjectRelease(root);
    if (iterating != KERN_SUCCESS) return NULL;
    MusterDdc *ddc = NULL;
    bool under = false;
    io_service_t entry;
    // A framebuffer's DCPAVServiceProxy follows it in the service plane.
    while (!ddc && (entry = IOIteratorNext(entries)) != MACH_PORT_NULL) {
        if (IOObjectConformsTo(entry, "IOMobileFramebuffer")) {
            uint64_t id = 0;
            under = IORegistryEntryGetRegistryEntryID(entry, &id) == KERN_SUCCESS && id == wanted;
        } else if (under) {
            io_name_t name;
            if (IORegistryEntryGetName(entry, name) == KERN_SUCCESS &&
                strcmp(name, "DCPAVServiceProxy") == 0 && external_proxy(entry)) {
                IOAVServiceRef service = api.create(kCFAllocatorDefault, entry);
                if (service) {
                    ddc = calloc(1, sizeof(MusterDdc));
                    ddc->service = service;
                    ddc->chip = proxy_chip(entry);
                }
            }
        }
        IOObjectRelease(entry);
    }
    IOObjectRelease(entries);
    return ddc;
}

void muster_ddc_close(void *handle) {
    MusterDdc *ddc = handle;
    if (!ddc) return;
    CFRelease(ddc->service);
    free(ddc);
}

static bool ddc_send(MusterDdc *ddc, uint8_t *packet, uint32_t length) {
    // Some displays drop a single request; m1ddc sends each one twice.
    for (int i = 0; i < 2; i++) {
        usleep(10000);
        if (api.write(ddc->service, ddc->chip, 0x51, packet, length) != kIOReturnSuccess) return false;
    }
    return true;
}

// Get VCP Feature. False unless the display answered for this code with a
// valid checksum and no error.
bool muster_ddc_read(void *handle, uint8_t code, uint16_t *current, uint16_t *maximum) {
    MusterDdc *ddc = handle;
    if (!ddc) return false;
    uint8_t request[4] = {0x82, 0x01, code, 0};
    request[3] = 0x6E ^ 0x51 ^ request[0] ^ request[1] ^ request[2];
    if (!ddc_send(ddc, request, sizeof request)) return false;
    uint8_t reply[12] = {0};
    usleep(ddc->chip == 0xB7 ? 50000 : 10000);
    if (api.read(ddc->service, ddc->chip, 0x51, reply, sizeof reply) != kIOReturnSuccess) return false;
    // Source, length, 0x02, result, code, type, maximum (2), current (2), checksum.
    uint8_t check = 0x50;
    for (int i = 0; i < 10; i++) check ^= reply[i];
    if (reply[0] != 0x6E || reply[1] != 0x88 || reply[2] != 0x02 || reply[3] != 0 ||
        reply[4] != code || reply[10] != check) return false;
    *maximum = (uint16_t)(reply[6] << 8 | reply[7]);
    *current = (uint16_t)(reply[8] << 8 | reply[9]);
    return true;
}

// Set VCP Feature. The display does not acknowledge it; read the value back.
bool muster_ddc_write(void *handle, uint8_t code, uint16_t value) {
    MusterDdc *ddc = handle;
    if (!ddc) return false;
    uint8_t packet[6] = {0x84, 0x03, code, (uint8_t)(value >> 8), (uint8_t)(value & 0xFF), 0};
    packet[5] = 0x6E ^ 0x51 ^ packet[0] ^ packet[1] ^ packet[2] ^ packet[3] ^ packet[4];
    return ddc_send(ddc, packet, sizeof packet);
}

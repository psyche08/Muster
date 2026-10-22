#include "MusterDiagnostics.h"
#include <mach/mach.h>
#include <mach/mach_time.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <pthread.h>
#include <string.h>
#include <uuid/uuid.h>

// Snapshot image metadata in dyld callbacks, when images are still mapped. No
// dladdr/dyld, allocation, locks, logging or language runtime inside suspension.
#define IMAGE_LIMIT 2048
static MusterStackImage images[IMAGE_LIMIT];
static pthread_mutex_t image_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_once_t prepare_once = PTHREAD_ONCE_INIT;
static mach_timebase_info_data_t timebase;

static void image_added(const struct mach_header *header, intptr_t slide) {
    if (header->magic != MH_MAGIC_64) return;
    const struct mach_header_64 *h = (const void *)header;
    MusterStackImage image = {0};
    image.base = (uintptr_t)header;
    const char *arch = h->cputype == CPU_TYPE_ARM64 ? "arm64" :
                       h->cputype == CPU_TYPE_X86_64 ? "x86_64" : "unknown";
    if (h->cputype == CPU_TYPE_ARM64 &&
        (h->cpusubtype & ~CPU_SUBTYPE_MASK) == CPU_SUBTYPE_ARM64E) arch = "arm64e";
    strlcpy(image.arch, arch, sizeof(image.arch));
    const struct load_command *command = (const void *)(h + 1);
    for (uint32_t i = 0; i < h->ncmds; i++) {
        if (command->cmd == LC_UUID) {
            uuid_unparse_lower(((const struct uuid_command *)command)->uuid, image.uuid);
        } else if (command->cmd == LC_SEGMENT_64) {
            const struct segment_command_64 *segment = (const void *)command;
            // Some OS images place instructions in __TEXT_EXEC. Keep the
            // Mach-O header as the ASLR base used by atos, covering all code.
            uintptr_t end = (uintptr_t)(segment->vmaddr + slide + segment->vmsize);
            if ((segment->initprot & VM_PROT_EXECUTE) && end > image.base &&
                end - image.base > image.size) image.size = end - image.base;
        }
        command = (const void *)((const char *)command + command->cmdsize);
    }
    // The callback runs under dyld's image lifetime protection. Retain only the
    // basename, never app/container paths (which can include user identifiers).
    for (uint32_t i = 0; i < _dyld_image_count(); i++) {
        if (_dyld_get_image_header(i) != header) continue;
        const char *path = _dyld_get_image_name(i);
        if (path) {
            const char *base = strrchr(path, '/');
            strlcpy(image.name, base ? base + 1 : path, sizeof(image.name));
        }
        break;
    }
    pthread_mutex_lock(&image_lock);
    for (int i = 0; i < IMAGE_LIMIT; i++) {
        if (!images[i].base) { images[i] = image; break; }
    }
    pthread_mutex_unlock(&image_lock);
}
static void image_removed(const struct mach_header *header, intptr_t slide) {
    (void)slide;
    pthread_mutex_lock(&image_lock);
    for (int i = 0; i < IMAGE_LIMIT; i++) {
        if (images[i].base == (uintptr_t)header) images[i].base = 0;
    }
    pthread_mutex_unlock(&image_lock);
}
static void prepare(void) {
    mach_timebase_info(&timebase);
    _dyld_register_func_for_add_image(image_added);
    _dyld_register_func_for_remove_image(image_removed);
}
void muster_stack_prepare(void) { pthread_once(&prepare_once, prepare); }
bool muster_stack_image(uintptr_t pc, MusterStackImage *result) {
    muster_stack_prepare();
    bool found = false;
    pthread_mutex_lock(&image_lock);
    for (int i = 0; i < IMAGE_LIMIT; i++) {
        if (images[i].base && pc >= images[i].base && pc - images[i].base < images[i].size) {
            *result = images[i]; found = true; break;
        }
    }
    pthread_mutex_unlock(&image_lock);
    return found;
}
static uint64_t elapsed_ns(uint64_t start) {
    return (mach_absolute_time() - start) * timebase.numer / timebase.denom;
}
static uintptr_t code_address(uintptr_t address) {
#if defined(__arm64__)
    // XPACLRI is in the architectural HINT space: strips PAC on arm64e devices
    // even when this translation unit targets arm64; harmless on older CPUs.
    __asm__ volatile("mov x30, %0\n\thint #7\n\tmov %0, x30" : "+r"(address) : : "x30");
#endif
    return address;
}
// Keep this C-only region bounded and independent of locks held by the target.
static void capture(thread_t thread, MusterThreadStack *result) {
    uint64_t start = mach_absolute_time();
    result->status = thread_suspend(thread);
    if (result->status != KERN_SUCCESS) return;
    uintptr_t pc = 0, fp = 0, sp = 0;
#if defined(__arm64__)
    arm_thread_state64_t state;
    mach_msg_type_number_t count = ARM_THREAD_STATE64_COUNT;
    result->status = thread_get_state(thread, ARM_THREAD_STATE64, (thread_state_t)&state, &count);
    if (result->status == KERN_SUCCESS) {
        pc = arm_thread_state64_get_pc(state);
        fp = arm_thread_state64_get_fp(state);
        sp = arm_thread_state64_get_sp(state);
    }
#elif defined(__x86_64__)
    x86_thread_state64_t state;
    mach_msg_type_number_t count = x86_THREAD_STATE64_COUNT;
    result->status = thread_get_state(thread, x86_THREAD_STATE64, (thread_state_t)&state, &count);
    if (result->status == KERN_SUCCESS) { pc = state.__rip; fp = state.__rbp; sp = state.__rsp; }
#else
    result->status = KERN_NOT_SUPPORTED;
    result->stop = MUSTER_STACK_UNSUPPORTED;
#endif
    if (result->status == KERN_SUCCESS) {
        if (pc) result->pcs[result->count++] = code_address(pc);
        while (fp && result->count < MUSTER_STACK_FRAMES) {
            if (elapsed_ns(start) >= 1000000) { result->stop = MUSTER_STACK_TIME_LIMIT; break; }
            // An 8 MiB upper bound is deliberately conservative; never dereference
            // arbitrary stack pointers directly, including a damaged frame chain.
            if (fp < sp || fp - sp > 8 * 1024 * 1024 || fp % sizeof(uintptr_t)) {
                result->stop = MUSTER_STACK_INVALID_FRAME; break;
            }
            uintptr_t frame[2];
            vm_size_t size = 0;
            kern_return_t kr = vm_read_overwrite(mach_task_self(), fp, sizeof(frame),
                                                      (vm_address_t)frame, &size);
            if (kr != KERN_SUCCESS || size != sizeof(frame)) { result->stop = MUSTER_STACK_UNREADABLE; break; }
            if (frame[1]) result->pcs[result->count++] = code_address(frame[1]);
            if (frame[0] && frame[0] <= fp) { result->stop = MUSTER_STACK_INVALID_FRAME; break; }
            fp = frame[0];
        }
        if (fp && result->count == MUSTER_STACK_FRAMES) result->stop = MUSTER_STACK_LIMIT;
    }
    // Exactly one balancing resume for every successful suspend, including errors.
    result->resume_status = thread_resume(thread);
    result->duration_ns = elapsed_ns(start);
}
void muster_thread_stack(uint64_t thread_id, MusterThreadStack *result) {
    memset(result, 0, sizeof(*result));
    result->status = KERN_INVALID_ARGUMENT;
    muster_stack_prepare();
    uint64_t caller = 0;
    pthread_threadid_np(NULL, &caller);
    if (caller == thread_id) return;
    thread_act_array_t threads = NULL;
    mach_msg_type_number_t count = 0;
    result->status = task_threads(mach_task_self(), &threads, &count);
    if (result->status != KERN_SUCCESS) return;
    result->status = KERN_INVALID_NAME;
    for (mach_msg_type_number_t i = 0; i < count; i++) {
        thread_identifier_info_data_t info;
        mach_msg_type_number_t info_count = THREAD_IDENTIFIER_INFO_COUNT;
        if (thread_info(threads[i], THREAD_IDENTIFIER_INFO, (thread_info_t)&info, &info_count) == KERN_SUCCESS &&
            info.thread_id == thread_id) { capture(threads[i], result); break; }
    }
    for (mach_msg_type_number_t i = 0; i < count; i++) mach_port_deallocate(mach_task_self(), threads[i]);
    vm_deallocate(mach_task_self(), (vm_address_t)threads, count * sizeof(thread_t));
}

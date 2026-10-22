#ifndef MUSTER_DIAGNOSTICS_H
#define MUSTER_DIAGNOSTICS_H
#include <stdint.h>
#include <stdbool.h>
#define MUSTER_STACK_FRAMES 64
// Status is a Mach error for acquisition failures. Stop explains a partial walk.
enum { MUSTER_STACK_END, MUSTER_STACK_LIMIT, MUSTER_STACK_INVALID_FRAME,
       MUSTER_STACK_UNREADABLE, MUSTER_STACK_TIME_LIMIT, MUSTER_STACK_UNSUPPORTED };
typedef struct {
    int32_t status, resume_status;
    uint32_t count, stop;
    uint64_t duration_ns;
    uintptr_t pcs[MUSTER_STACK_FRAMES];
} MusterThreadStack;
typedef struct {
    uintptr_t base, size;
    char uuid[37], name[256], arch[16];
} MusterStackImage;
// Call on the diagnostic queue before suspending any thread.
void muster_stack_prepare(void);
// Own-process threads only; refuses the caller. Never resolves symbols while suspended.
void muster_thread_stack(uint64_t thread_id, MusterThreadStack *result);
bool muster_stack_image(uintptr_t pc, MusterStackImage *result);
#endif

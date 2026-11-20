#include <string.h>
#include "overrides.h"

/* __aeabi_memclr4(void *dst, uint32_t size) */
RECOMP_OVERRIDE(0x00110490) {
    uint32_t dst = ctx->r[0];
    uint32_t size = ctx->r[1];
    uint8_t *span = mem_write_span(ctx, dst, size);
    if (LIKELY(span != NULL)) {
        memset(span, 0, size);
    } else {
        uint32_t i = 0;
        for (; i + 4 <= size; i += 4) {
            mem_write32(ctx, dst + i, 0);
        }
        for (; i < size; i++) {
            mem_write8(ctx, dst + i, 0);
        }
    }
    ctx->r[0] = dst + size;
    RETURN_TO(ctx->r[14]);
}

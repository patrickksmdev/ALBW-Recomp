#include "overrides.h"

/* sead::ListNode::insertBefore(sead::ListNode *n) */
RECOMP_OVERRIDE(0x0010F1DC) {
    uint32_t node = ctx->r[0];
    uint32_t inserted = ctx->r[1];
    uint32_t prev = mem_read32(ctx, node + 0);

    mem_write32(ctx, node + 0, inserted);
    mem_write32(ctx, inserted + 4, node);
    mem_write32(ctx, inserted + 0, prev);
    if (prev != 0) {
        mem_write32(ctx, prev + 4, inserted);
    }

    RETURN_TO(ctx->r[14]);
}

/* sead::ListNode::erase_() */
RECOMP_OVERRIDE(0x0010F1F8) {
    uint32_t node = ctx->r[0];
    uint32_t prev = mem_read32(ctx, node + 0);
    uint32_t next = mem_read32(ctx, node + 4);

    if (prev != 0) {
        mem_write32(ctx, prev + 4, next);
    }
    if (next != 0) {
        mem_write32(ctx, next + 0, prev);
    }
    mem_write32(ctx, node + 4, 0);
    mem_write32(ctx, node + 0, 0);

    RETURN_TO(ctx->r[14]);
}

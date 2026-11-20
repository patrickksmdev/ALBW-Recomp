#include <string.h>
#include "overrides.h"

extern void albw_get_view_params(Context *ctx, float *out_top_width, float *out_fov_scale);

static inline float read_f32_sead(Context *ctx, uint32_t addr) {
    uint32_t bits = mem_read32(ctx, addr);
    float f;
    memcpy(&f, &bits, sizeof(f));
    return f;
}

static inline void write_f32_sead(Context *ctx, uint32_t addr, float f) {
    uint32_t bits;
    memcpy(&bits, &f, sizeof(bits));
    mem_write32(ctx, addr, bits);
}

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

/* sead::PerspectiveProjection::doUpdateMatrixImpl_(sead::Matrix44f *out) */
RECOMP_OVERRIDE(0x0057F6C0) {
    uint32_t self = ctx->r[0];
    uint32_t out = ctx->r[1];

    float near_val = read_f32_sead(ctx, self + 0x94u);
    float far_val  = read_f32_sead(ctx, self + 0x98u);
    float tan_half = read_f32_sead(ctx, self + 0xA8u);
    float aspect   = read_f32_sead(ctx, self + 0xACu);
    float off_x    = read_f32_sead(ctx, self + 0xB0u);
    float off_y    = read_f32_sead(ctx, self + 0xB4u);

    /* Only widen heap 3D top-screen projections (5:3 aspect ~1.667); skip stack-allocated derived projections (e.g. 0x003881E4) and 4:3 bottom screen */
    if (self >= 0x10000000u && aspect > 1.5f) {
        float top_width = 400.0f;
        float fov_scale = 1.0f;
        albw_get_view_params(ctx, &top_width, &fov_scale);
        if (fov_scale < 0.5f) fov_scale = 0.5f;
        if (fov_scale > 2.0f) fov_scale = 2.0f;
        if (top_width < 1.0f) top_width = 400.0f;
        tan_half *= fov_scale;
        aspect *= (top_width / 400.0f);
    }

    float inv_tan = 1.0f / tan_half;
    float m00 = inv_tan / aspect;
    float m02 = 2.0f * off_x;
    float m11 = inv_tan;
    float m12 = 2.0f * off_y;
    float inv_fn = 1.0f / (far_val - near_val);
    float m22 = -far_val * inv_fn;
    float m23 = -far_val * near_val * inv_fn;

    write_f32_sead(ctx, out + 0x00u, m00);
    write_f32_sead(ctx, out + 0x04u, 0.0f);
    write_f32_sead(ctx, out + 0x08u, m02);
    write_f32_sead(ctx, out + 0x0Cu, 0.0f);

    write_f32_sead(ctx, out + 0x10u, 0.0f);
    write_f32_sead(ctx, out + 0x14u, m11);
    write_f32_sead(ctx, out + 0x18u, m12);
    write_f32_sead(ctx, out + 0x1Cu, 0.0f);

    write_f32_sead(ctx, out + 0x20u, 0.0f);
    write_f32_sead(ctx, out + 0x24u, 0.0f);
    write_f32_sead(ctx, out + 0x28u, m22);
    write_f32_sead(ctx, out + 0x2Cu, m23);

    write_f32_sead(ctx, out + 0x30u, 0.0f);
    write_f32_sead(ctx, out + 0x34u, 0.0f);
    write_f32_sead(ctx, out + 0x38u, -1.0f);
    write_f32_sead(ctx, out + 0x3Cu, 0.0f);

    RETURN_TO(ctx->r[14]);
}

/* sead::Projection::getDeviceProjectionMatrix() */
RECOMP_OVERRIDE(0x0057D5C4) {
    uint32_t self = ctx->r[0];
    uint32_t vtable = mem_read32(ctx, self + 0x0u);
    uint32_t update_fn = mem_read32(ctx, vtable + 0x14u);

    if (self >= 0x10000000u && (update_fn & ~3u) == 0x0057F6C0u) {
        static struct {
            uint32_t ptr;
            float top_width;
            float fov_scale;
        } cache[16];
        static uint32_t next_slot = 0;

        float top_width = 400.0f;
        float fov_scale = 1.0f;
        albw_get_view_params(ctx, &top_width, &fov_scale);

        int found = 0;
        for (int i = 0; i < 16; i++) {
            if (cache[i].ptr == self) {
                found = 1;
                if (cache[i].top_width != top_width || cache[i].fov_scale != fov_scale) {
                    cache[i].top_width = top_width;
                    cache[i].fov_scale = fov_scale;
                    mem_write8(ctx, self + 0x4u, 1);
                }
                break;
            }
        }
        if (!found) {
            uint32_t slot = next_slot++ & 15u;
            cache[slot].ptr = self;
            cache[slot].top_width = top_width;
            cache[slot].fov_scale = fov_scale;
            mem_write8(ctx, self + 0x4u, 1);
        }
    }

    return original_0x0057D5C4(ctx);
}

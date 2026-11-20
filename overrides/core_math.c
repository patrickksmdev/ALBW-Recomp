#include <math.h>
#include <string.h>
#include "overrides.h"

static inline float read_f32(Context *ctx, uint32_t addr) {
    uint32_t bits = mem_read32(ctx, addr);
    float f;
    memcpy(&f, &bits, sizeof(f));
    return f;
}

static inline void write_f32(Context *ctx, uint32_t addr, float f) {
    uint32_t bits;
    memcpy(&bits, &f, sizeof(bits));
    mem_write32(ctx, addr, bits);
}

static inline void read_vec3(Context *ctx, uint32_t addr, float out[3]) {
    out[0] = read_f32(ctx, addr + 0);
    out[1] = read_f32(ctx, addr + 4);
    out[2] = read_f32(ctx, addr + 8);
}

static inline void write_vec3(Context *ctx, uint32_t addr, const float v[3]) {
    write_f32(ctx, addr + 0, v[0]);
    write_f32(ctx, addr + 4, v[1]);
    write_f32(ctx, addr + 8, v[2]);
}

/* sead::Random::getU32() */
RECOMP_OVERRIDE(0x00111A64) {
    uint32_t p = ctx->r[0];
    uint32_t x = mem_read32(ctx, p + 0);
    uint32_t y = mem_read32(ctx, p + 4);
    uint32_t z = mem_read32(ctx, p + 8);
    uint32_t w = mem_read32(ctx, p + 12);

    uint32_t t = x ^ (x << 11);
    x = y;
    y = z;
    z = w;
    w = (w ^ (w >> 19)) ^ (t ^ (t >> 8));

    mem_write32(ctx, p + 0, x);
    mem_write32(ctx, p + 4, y);
    mem_write32(ctx, p + 8, z);
    mem_write32(ctx, p + 12, w);

    ctx->r[0] = w;
    RETURN_TO(ctx->r[14]);
}

/* Vec3::setAdd(const Vec3& a, const Vec3& b) */
RECOMP_OVERRIDE(0x00115C54) {
    float a[3], b[3], res[3];
    read_vec3(ctx, ctx->r[1], a);
    read_vec3(ctx, ctx->r[2], b);
    for (int i = 0; i < 3; i++) {
        res[i] = a[i] + b[i];
    }
    write_vec3(ctx, ctx->r[0], res);
    RETURN_TO(ctx->r[14]);
}

/* Vec3::setSub(const Vec3& a, const Vec3& b) */
RECOMP_OVERRIDE(0x00115C70) {
    float a[3], b[3], res[3];
    read_vec3(ctx, ctx->r[1], a);
    read_vec3(ctx, ctx->r[2], b);
    for (int i = 0; i < 3; i++) {
        res[i] = a[i] - b[i];
    }
    write_vec3(ctx, ctx->r[0], res);
    RETURN_TO(ctx->r[14]);
}

/* Vec3::setMul(const Vec3& v, float s) */
RECOMP_OVERRIDE(0x00115D70) {
    float v[3], s, res[3];
    read_vec3(ctx, ctx->r[1], v);
    s = vfp_s(ctx, 0);
    for (int i = 0; i < 3; i++) {
        res[i] = v[i] * s;
    }
    write_vec3(ctx, ctx->r[0], res);
    RETURN_TO(ctx->r[14]);
}

/* Vec3::setDiv(const Vec3& a, float s) */
RECOMP_OVERRIDE(0x00115D88) {
    float a[3], s, inv, res[3];
    read_vec3(ctx, ctx->r[1], a);
    s = vfp_s(ctx, 0);
    inv = 1.0f / s;
    for (int i = 0; i < 3; i++) {
        res[i] = a[i] * inv;
    }
    write_vec3(ctx, ctx->r[0], res);
    RETURN_TO(ctx->r[14]);
}

/* Vec3::setScaleAdd(float scale, const Vec3& a, const Vec3& b) */
RECOMP_OVERRIDE(0x00115DAC) {
    float a[3], b[3], res[3];
    float scale = vfp_s(ctx, 0);
    read_vec3(ctx, ctx->r[1], a);
    read_vec3(ctx, ctx->r[2], b);
    for (int i = 0; i < 3; i++) {
        res[i] = b[i] + a[i] * scale;
    }
    write_vec3(ctx, ctx->r[0], res);
    RETURN_TO(ctx->r[14]);
}

/* Vec3::normalize() -> returns length in s0 */
RECOMP_OVERRIDE(0x00115DC8) {
    float v[3];
    read_vec3(ctx, ctx->r[0], v);
    float len = sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    vfp_set_s(ctx, 0, len);
    if (len > 0.0f) {
        float inv = 1.0f / len;
        v[0] *= inv;
        v[1] *= inv;
        v[2] *= inv;
        write_vec3(ctx, ctx->r[0], v);
    }
    RETURN_TO(ctx->r[14]);
}

/* Vec3::cross(const Vec3& a, const Vec3& b) */
RECOMP_OVERRIDE(0x00115E10) {
    float a[3], b[3], res[3];
    read_vec3(ctx, ctx->r[1], a);
    read_vec3(ctx, ctx->r[2], b);
    res[0] = a[1] * b[2] - a[2] * b[1];
    res[1] = a[2] * b[0] - a[0] * b[2];
    res[2] = a[0] * b[1] - a[1] * b[0];
    write_vec3(ctx, ctx->r[0], res);
    RETURN_TO(ctx->r[14]);
}

/* Mtx33::fromMtx34(Mtx33* out, const Mtx34* in) */
RECOMP_OVERRIDE(0x001151D8) {
    uint32_t dst = ctx->r[0];
    uint32_t src = ctx->r[1];
    for (int row = 0; row < 3; row++) {
        float v[3];
        read_vec3(ctx, src + (uint32_t)(row * 16), v);
        write_vec3(ctx, dst + (uint32_t)(row * 12), v);
    }
    ctx->r[0] = dst + 36u;
    RETURN_TO(ctx->r[14]);
}

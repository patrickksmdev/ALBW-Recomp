#include "overrides.h"

/*
 * PC x86_64 has coherent CPU/GPU/DSP memory, so 3DS cache flush IPC calls
 * (gsp::Gpu#0x0008 and dsp::DSP#0x0013) are safe no-ops without svc 0x32.
 */

extern uint32_t albw_gsp_trigger_cmd_queue(Context *ctx);
extern uint32_t albw_fs_read(
    Context *ctx,
    uint32_t handle,
    uint32_t out_bytes_ptr,
    uint64_t offset,
    uint32_t size,
    uint32_t dest
);
extern uint32_t albw_fs_close(Context *ctx, uint32_t handle);

/* nn::gsp::CTR::detail::FlushDataCache (gsp::Gpu#0x0008) */
RECOMP_OVERRIDE(0x003E3D74) {
    ctx->r[0] = 0;
    RETURN_TO(ctx->r[14]);
}

/* nn::dsp::CTR::FlushDataCache (dsp::DSP#0x0013) */
RECOMP_OVERRIDE(0x003D2D40) {
    ctx->r[0] = 0;
    RETURN_TO(ctx->r[14]);
}

/* nn::gsp::CTR::detail::TriggerCmdReqQueue (gsp::Gpu#0x000C) */
RECOMP_OVERRIDE(0x003E3DD8) {
    ctx->r[0] = albw_gsp_trigger_cmd_queue(ctx);
    RETURN_TO(ctx->r[14]);
}

/* nn::fs::detail::FileStream::Read (FSFile#0x0802) */
RECOMP_OVERRIDE(0x003C9A84) {
    uint32_t handle = mem_read32(ctx, ctx->r[0]);
    uint32_t out_bytes_ptr = ctx->r[1];
    uint64_t offset = ((uint64_t)ctx->r[3] << 32) | (uint64_t)ctx->r[2];
    uint32_t dest = mem_read32(ctx, ctx->r[13] + 0);
    uint32_t size = mem_read32(ctx, ctx->r[13] + 4);
    ctx->r[0] = albw_fs_read(ctx, handle, out_bytes_ptr, offset, size, dest);
    RETURN_TO(ctx->r[14]);
}

/* nn::fs::detail::FileStream::Close (FSFile#0x0808) */
RECOMP_OVERRIDE(0x003C9ADC) {
    uint32_t handle = mem_read32(ctx, ctx->r[0]);
    ctx->r[0] = albw_fs_close(ctx, handle);
    RETURN_TO(ctx->r[14]);
}

/* nn::dsp::CTR::ConvertProcessAddressFromDspDram (dsp::DSP#0x000C) */
RECOMP_OVERRIDE(0x003D2F18) {
    uint32_t address = ctx->r[1];
    uint32_t out_ptr = ctx->r[2];
    uint32_t converted = 0x1FF40000u + (address << 1);
    mem_write32(ctx, out_ptr, converted);
    ctx->r[0] = 0;
    RETURN_TO(ctx->r[14]);
}

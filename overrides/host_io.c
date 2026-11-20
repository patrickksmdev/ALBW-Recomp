#include "overrides.h"

/*
 * PC x86_64 has coherent CPU/GPU/DSP memory, so 3DS cache flush IPC calls
 * (gsp::Gpu#0x0008 and dsp::DSP#0x0013) are safe no-ops without svc 0x32.
 */

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

#include <stdio.h>
#include <string.h>
#include "overrides.h"

extern void albw_get_view_params(Context *ctx, float *out_top_width, float *out_fov_scale);
extern float albw_get_hud_scale(void);
extern void albw_notify_item_menu_active(void);

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

static void read_pane_name(Context *ctx, uint32_t pane, char out[25]) {
    for (int j = 0; j < 24; j++) {
        uint8_t c = mem_read8(ctx, pane + 0x80u + (uint32_t)j);
        out[j] = (c >= 32 && c < 127) ? (char)c : '\0';
        if (out[j] == '\0') break;
    }
    out[24] = '\0';
}

static int is_main_hud_pane(const char *cname) {
    return strncmp(cname, "L_Heart", 7) == 0 ||
           strncmp(cname, "L_HangonGauge", 13) == 0 ||
           strncmp(cname, "L_DoButton", 10) == 0 ||
           strncmp(cname, "L_Key_", 6) == 0 ||
           strncmp(cname, "L_Effects_", 10) == 0;
}

static void albw_scale_hud_from_pane(Context *ctx, uint32_t pane, int is_draw_submit) {
    enum { ROOT_NONE = 0, ROOT_MAIN_HUD = 1, ROOT_TOP_COUNTER = 2, ROOT_OTHER = 3, ROOT_ITEM_MENU = 4 };
    static struct {
        uint32_t root;
        uint8_t kind;
    } roots[64];
    static int root_n = 0;

    static struct {
        uint32_t ptr;
        float base_tx, base_ty;
        float base_sx, base_sy;
    } hud_panes[256];
    static int hud_pane_n = 0;

    if (pane == 0) return;

    uint32_t cur = pane;
    uint32_t root = pane;
    for (int depth = 0; depth < 12 && cur != 0; depth++) {
        root = cur;
        cur = mem_read32(ctx, cur + 0x0Cu);
    }

    int r_idx = -1;
    for (int i = 0; i < root_n; i++) {
        if (roots[i].root == root) { r_idx = i; break; }
    }

    if (r_idx < 0 && root_n < 64) {
        char rname[25];
        read_pane_name(ctx, root, rname);
        if (strcmp(rname, "RootPane") == 0) {
            uint8_t kind = ROOT_OTHER;
            uint32_t stack[12];
            int sp = 0;
            stack[sp++] = root;
            for (int iter = 0; iter < 64 && sp > 0 && kind == ROOT_OTHER; iter++) {
                uint32_t p = stack[--sp];
                uint32_t count = mem_read32(ctx, p + 0x10u);
                uint32_t node = mem_read32(ctx, p + 0x14u);
                uint32_t sentinel = p + 0x14u;
                for (uint32_t k = 0; k < count && k < 48 && node != 0 && node != sentinel; k++) {
                    uint32_t child = node - 0x04u;
                    char cname[25];
                    read_pane_name(ctx, child, cname);
                    if (strncmp(cname, "L_Heart_", 8) == 0 || strncmp(cname, "L_HangonGauge", 13) == 0) {
                        kind = ROOT_MAIN_HUD;
                        break;
                    }
                    if (strncmp(cname, "N_KimoB_", 8) == 0 || strncmp(cname, "N_Kinsuta_", 10) == 0) {
                        kind = ROOT_TOP_COUNTER;
                        break;
                    }
                    if (strcmp(cname, "L_Xbtn_00") == 0 || strcmp(cname, "P_CursorX_00") == 0) {
                        kind = ROOT_ITEM_MENU;
                        break;
                    }
                    if (strncmp(cname, "N_", 2) == 0 && sp < 12) {
                        stack[sp++] = child;
                    }
                    node = mem_read32(ctx, node + 0x00u);
                }
            }
            r_idx = root_n++;
            roots[r_idx].root = root;
            roots[r_idx].kind = kind;
        }
    }

    if (r_idx >= 0 && roots[r_idx].kind == ROOT_ITEM_MENU) {
        if ((mem_read8(ctx, pane + 0x44u) & 1u) != 0 && mem_read8(ctx, pane + 0x46u) > 0) {
            char pname[25];
            read_pane_name(ctx, pane, pname);
            if (strcmp(pname, "W_Window_00") == 0 || strcmp(pname, "P_WinBase_00") == 0) {
                albw_notify_item_menu_active();
            }
        }
    }

    float hud_scale = albw_get_hud_scale();

    for (int r = 0; r < root_n; r++) {
        if (roots[r].kind == ROOT_MAIN_HUD) {
            uint32_t stack[12];
            int sp = 0;
            stack[sp++] = roots[r].root;
            for (int iter = 0; iter < 64 && sp > 0; iter++) {
                uint32_t p = stack[--sp];
                uint32_t count = mem_read32(ctx, p + 0x10u);
                uint32_t node = mem_read32(ctx, p + 0x14u);
                uint32_t sentinel = p + 0x14u;
                for (uint32_t k = 0; k < count && k < 48 && node != 0 && node != sentinel; k++) {
                    uint32_t child = node - 0x04u;
                    char cname[25];
                    read_pane_name(ctx, child, cname);
                    if (strncmp(cname, "N_", 2) == 0 && sp < 12) {
                        stack[sp++] = child;
                    } else if (is_main_hud_pane(cname)) {
                        int idx = -1;
                        for (int h = 0; h < hud_pane_n; h++) {
                            if (hud_panes[h].ptr == child) { idx = h; break; }
                        }
                        if (idx < 0 && hud_pane_n < 256) {
                            idx = hud_pane_n++;
                            hud_panes[idx].ptr = child;
                            hud_panes[idx].base_tx = read_f32_sead(ctx, child + 0x1Cu);
                            hud_panes[idx].base_ty = read_f32_sead(ctx, child + 0x20u);
                            hud_panes[idx].base_sx = read_f32_sead(ctx, child + 0x34u);
                            hud_panes[idx].base_sy = read_f32_sead(ctx, child + 0x38u);
                        }
                        if (idx >= 0) {
                            float cx = (hud_panes[idx].base_tx < 0.0f) ? -194.0f : 194.0f;
                            float cy = (hud_panes[idx].base_ty < 0.0f) ? -114.0f : 114.0f;
                            write_f32_sead(ctx, child + 0x1Cu, cx + (hud_panes[idx].base_tx - cx) * hud_scale);
                            write_f32_sead(ctx, child + 0x20u, cy + (hud_panes[idx].base_ty - cy) * hud_scale);
                            write_f32_sead(ctx, child + 0x34u, hud_panes[idx].base_sx * hud_scale);
                            write_f32_sead(ctx, child + 0x38u, hud_panes[idx].base_sy * hud_scale);
                        }
                    }
                    node = mem_read32(ctx, node + 0x00u);
                }
            }
        } else if (roots[r].kind == ROOT_TOP_COUNTER) {
            /* Top-right counter HUD: scale N_Pos_00 directly (already anchored at (200, 120)) */
            uint32_t count = mem_read32(ctx, roots[r].root + 0x10u);
            uint32_t node = mem_read32(ctx, roots[r].root + 0x14u);
            uint32_t sentinel = roots[r].root + 0x14u;
            for (uint32_t k = 0; k < count && k < 8 && node != 0 && node != sentinel; k++) {
                uint32_t child = node - 0x04u;
                char cname[25];
                read_pane_name(ctx, child, cname);
                if (strcmp(cname, "N_Pos_00") == 0) {
                    int idx = -1;
                    for (int h = 0; h < hud_pane_n; h++) {
                        if (hud_panes[h].ptr == child) { idx = h; break; }
                    }
                    if (idx < 0 && hud_pane_n < 256) {
                        idx = hud_pane_n++;
                        hud_panes[idx].ptr = child;
                        hud_panes[idx].base_sx = read_f32_sead(ctx, child + 0x34u);
                        hud_panes[idx].base_sy = read_f32_sead(ctx, child + 0x38u);
                    }
                    if (idx >= 0) {
                        float target_sx = hud_panes[idx].base_sx * hud_scale;
                        float target_sy = hud_panes[idx].base_sy * hud_scale;
                        write_f32_sead(ctx, child + 0x34u, target_sx);
                        write_f32_sead(ctx, child + 0x38u, target_sy);

                        /* If CalculateMtx already ran this frame before Draw, patch mGlbMtx on N_Pos_00 and its subtree */
                        float glb_sx = read_f32_sead(ctx, child + 0x48u);
                        float glb_sy = read_f32_sead(ctx, child + 0x5Cu);
                        float dsx = glb_sx - target_sx;
                        float dsy = glb_sy - target_sy;
                        if ((dsx > 0.001f || dsx < -0.001f || dsy > 0.001f || dsy < -0.001f) &&
                            (glb_sx > 0.001f || glb_sx < -0.001f) &&
                            (glb_sy > 0.001f || glb_sy < -0.001f)) {
                            float rx = target_sx / glb_sx;
                            float ry = target_sy / glb_sy;
                            float ax = read_f32_sead(ctx, child + 0x54u);
                            float ay = read_f32_sead(ctx, child + 0x64u);
                            uint32_t stack[64];
                            int sp = 0;
                            stack[sp++] = child;
                            for (int iter = 0; iter < 128 && sp > 0; iter++) {
                                uint32_t p = stack[--sp];
                                write_f32_sead(ctx, p + 0x48u, read_f32_sead(ctx, p + 0x48u) * rx);
                                write_f32_sead(ctx, p + 0x4Cu, read_f32_sead(ctx, p + 0x4Cu) * rx);
                                write_f32_sead(ctx, p + 0x50u, read_f32_sead(ctx, p + 0x50u) * rx);
                                write_f32_sead(ctx, p + 0x54u, ax + (read_f32_sead(ctx, p + 0x54u) - ax) * rx);
                                write_f32_sead(ctx, p + 0x58u, read_f32_sead(ctx, p + 0x58u) * ry);
                                write_f32_sead(ctx, p + 0x5Cu, read_f32_sead(ctx, p + 0x5Cu) * ry);
                                write_f32_sead(ctx, p + 0x60u, read_f32_sead(ctx, p + 0x60u) * ry);
                                write_f32_sead(ctx, p + 0x64u, ay + (read_f32_sead(ctx, p + 0x64u) - ay) * ry);

                                uint32_t ccount = mem_read32(ctx, p + 0x10u);
                                uint32_t cnode = mem_read32(ctx, p + 0x14u);
                                uint32_t csent = p + 0x14u;
                                for (uint32_t ck = 0; ck < ccount && ck < 32 && cnode != 0 && cnode != csent; ck++) {
                                    uint32_t sub = cnode - 0x04u;
                                    if (sp < 64) stack[sp++] = sub;
                                    cnode = mem_read32(ctx, cnode + 0x00u);
                                }
                            }
                        }
                    }
                    break;
                }
                node = mem_read32(ctx, node + 0x00u);
            }
        }
    }
}

/* nw::lyt::Pane draw/submit helper called from 0x004373AC */
RECOMP_OVERRIDE(0x00503388) {
    albw_scale_hud_from_pane(ctx, ctx->r[2], 1);
    return original_0x00503388(ctx);
}

/* nw::lyt::Pane traversal functors (TextBox/Picture/Window/Bound) */
RECOMP_OVERRIDE(0x00437244) {
    albw_scale_hud_from_pane(ctx, ctx->r[1], 0);
    return original_0x00437244(ctx);
}

RECOMP_OVERRIDE(0x0043729C) {
    albw_scale_hud_from_pane(ctx, ctx->r[1], 0);
    return original_0x0043729C(ctx);
}

RECOMP_OVERRIDE(0x004372F4) {
    albw_scale_hud_from_pane(ctx, ctx->r[1], 0);
    return original_0x004372F4(ctx);
}

#include "imgui.h"
#include <cstdint>
#include <cstring>
#include <vector>

struct ImGuiMenuState {
    // Inputs from Rust
    float display_w;
    float display_h;
    float dpi_scale;
    float delta_time;
    bool menu_open;
    bool show_fps;
    float current_fps;
    int slot;
    bool slot_has_save;

    // Settings (in/out)
    int resolution;         // 1..=6
    int scale;              // 1..=6
    int layout;             // 0..=4
    int top_shape;          // 0..=2 (5:3, 16:9, 21:9)
    float fov_scale;        // 0.80..=1.50
    float hud_scale;        // 0.55..=1.00
    int filter;             // 0..=2
    bool integer_scale;
    bool hardware_rasterizer;
    bool frame_interpolation;
    float volume;           // 0.0..=1.0
    bool mute;

    // Actions out (bitmask)
    // bit 0: Resume
    // bit 1: Reset
    // bit 2: Fullscreen
    // bit 3: SaveState
    // bit 4: LoadState
    // bit 5: OpenSettings (advanced/controls)
    // bit 6: Quit
    // bit 7: SettingsChanged
    uint32_t actions;
};

struct ImGuiBridgeVertex {
    float x, y;
    float u, v;
    uint8_t r, g, b, a;
};

struct ImGuiBridgeDrawCmd {
    uint32_t clip_x1;
    uint32_t clip_y1;
    uint32_t clip_x2;
    uint32_t clip_y2;
    uint32_t vtx_offset;
    uint32_t vtx_count;
    uint32_t idx_offset;
    uint32_t idx_count;
};

static ImGuiContext* g_ctx = nullptr;
static bool g_font_uploaded = false;
static std::vector<uint8_t> g_font_pixels;
static int g_font_w = 0;
static int g_font_h = 0;
static std::vector<ImGuiBridgeVertex> g_vertices;
static std::vector<uint32_t> g_indices;
static std::vector<ImGuiBridgeDrawCmd> g_cmds;

static void apply_albw_style() {
    ImGui::StyleColorsDark();
    ImGuiStyle& style = ImGui::GetStyle();
    style.WindowPadding = ImVec2(16.0f, 14.0f);
    style.FramePadding = ImVec2(10.0f, 6.0f);
    style.ItemSpacing = ImVec2(10.0f, 8.0f);
    style.WindowRounding = 6.0f;
    style.FrameRounding = 4.0f;
    style.GrabRounding = 4.0f;
    style.TabRounding = 4.0f;
    style.WindowBorderSize = 1.0f;
    style.FrameBorderSize = 1.0f;
    style.WindowTitleAlign = ImVec2(0.5f, 0.5f);

    ImVec4* c = style.Colors;
    c[ImGuiCol_WindowBg]         = ImVec4(0.07f, 0.08f, 0.11f, 0.94f);
    c[ImGuiCol_Border]           = ImVec4(0.25f, 0.28f, 0.36f, 0.85f);
    c[ImGuiCol_TitleBg]          = ImVec4(0.09f, 0.10f, 0.14f, 1.00f);
    c[ImGuiCol_TitleBgActive]    = ImVec4(0.12f, 0.13f, 0.18f, 1.00f);
    c[ImGuiCol_FrameBg]          = ImVec4(0.13f, 0.15f, 0.20f, 1.00f);
    c[ImGuiCol_FrameBgHovered]   = ImVec4(0.18f, 0.21f, 0.28f, 1.00f);
    c[ImGuiCol_FrameBgActive]    = ImVec4(0.24f, 0.28f, 0.36f, 1.00f);
    c[ImGuiCol_Button]           = ImVec4(0.15f, 0.17f, 0.23f, 1.00f);
    c[ImGuiCol_ButtonHovered]    = ImVec4(0.85f, 0.68f, 0.22f, 0.35f);
    c[ImGuiCol_ButtonActive]     = ImVec4(0.85f, 0.68f, 0.22f, 0.65f);
    c[ImGuiCol_Tab]              = ImVec4(0.12f, 0.14f, 0.19f, 1.00f);
    c[ImGuiCol_TabHovered]       = ImVec4(0.85f, 0.68f, 0.22f, 0.45f);
    c[ImGuiCol_TabSelected]      = ImVec4(0.85f, 0.68f, 0.22f, 0.75f);
    c[ImGuiCol_CheckMark]        = ImVec4(0.90f, 0.74f, 0.28f, 1.00f);
    c[ImGuiCol_SliderGrab]       = ImVec4(0.85f, 0.68f, 0.22f, 0.85f);
    c[ImGuiCol_SliderGrabActive] = ImVec4(0.95f, 0.78f, 0.30f, 1.00f);
    c[ImGuiCol_Header]           = ImVec4(0.85f, 0.68f, 0.22f, 0.30f);
    c[ImGuiCol_HeaderHovered]    = ImVec4(0.85f, 0.68f, 0.22f, 0.45f);
    c[ImGuiCol_HeaderActive]     = ImVec4(0.85f, 0.68f, 0.22f, 0.65f);
}

extern "C" {

void albw_imgui_init(const uint8_t* font_data, uint32_t font_size, float dpi_scale) {
    if (g_ctx) return;
    g_ctx = ImGui::CreateContext();
    ImGuiIO& io = ImGui::GetIO();
    io.IniFilename = nullptr;
    io.BackendFlags |= ImGuiBackendFlags_RendererHasVtxOffset;

    if (font_data && font_size > 0) {
        ImFontConfig cfg;
        cfg.FontDataOwnedByAtlas = false;
        float px = 17.0f * (dpi_scale > 0.0f ? dpi_scale : 1.0f);
        io.Fonts->AddFontFromMemoryTTF((void*)font_data, (int)font_size, px, &cfg);
    }

    unsigned char* pixels = nullptr;
    int w = 0, h = 0;
    io.Fonts->GetTexDataAsRGBA32(&pixels, &w, &h);
    g_font_w = w;
    g_font_h = h;
    g_font_pixels.resize((size_t)w * (size_t)h * 4);
    for (int i = 0; i < w * h; i++) {
        uint8_t a = pixels[i * 4 + 3];
        // Premultiplied sRGB RGBA for zakuro-gpu
        g_font_pixels[i * 4 + 0] = a;
        g_font_pixels[i * 4 + 1] = a;
        g_font_pixels[i * 4 + 2] = a;
        g_font_pixels[i * 4 + 3] = a;
    }
    io.Fonts->SetTexID((ImTextureID)(uintptr_t)1);
    g_font_uploaded = false;
    apply_albw_style();
}

void albw_imgui_mouse_pos(float x, float y) {
    if (!g_ctx) return;
    ImGui::GetIO().AddMousePosEvent(x, y);
}

void albw_imgui_mouse_button(int button, bool down) {
    if (!g_ctx) return;
    ImGui::GetIO().AddMouseButtonEvent(button, down);
}

void albw_imgui_mouse_wheel(float dx, float dy) {
    if (!g_ctx) return;
    ImGui::GetIO().AddMouseWheelEvent(dx, dy);
}

bool albw_imgui_wants_mouse() {
    if (!g_ctx) return false;
    return ImGui::GetIO().WantCaptureMouse;
}

bool albw_imgui_take_font_atlas(const uint8_t** out_pixels, int* out_w, int* out_h) {
    if (g_font_uploaded || g_font_pixels.empty()) return false;
    g_font_uploaded = true;
    *out_pixels = g_font_pixels.data();
    *out_w = g_font_w;
    *out_h = g_font_h;
    return true;
}

void albw_imgui_frame(ImGuiMenuState* st) {
    if (!g_ctx || !st) return;
    st->actions = 0;

    ImGuiIO& io = ImGui::GetIO();
    float scale = st->dpi_scale > 0.0f ? st->dpi_scale : 1.0f;
    io.DisplaySize = ImVec2(st->display_w / scale, st->display_h / scale);
    io.DisplayFramebufferScale = ImVec2(scale, scale);
    io.DeltaTime = st->delta_time > 0.0f ? st->delta_time : (1.0f / 60.0f);

    ImGui::NewFrame();

    if (st->show_fps) {
        ImGui::SetNextWindowPos(ImVec2(10.0f, 10.0f), ImGuiCond_Always);
        ImGui::SetNextWindowBgAlpha(0.65f);
        ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_AlwaysAutoResize |
                                 ImGuiWindowFlags_NoSavedSettings | ImGuiWindowFlags_NoFocusOnAppearing |
                                 ImGuiWindowFlags_NoNav | ImGuiWindowFlags_NoInputs;
        if (ImGui::Begin("##fps", nullptr, flags)) {
            ImGui::Text("%.0f FPS", st->current_fps);
        }
        ImGui::End();
    }

    if (st->menu_open) {
        ImVec2 center(io.DisplaySize.x * 0.5f, io.DisplaySize.y * 0.5f);
        ImGui::SetNextWindowPos(center, ImGuiCond_Always, ImVec2(0.5f, 0.5f));
        ImGui::SetNextWindowSize(ImVec2(460.0f, 0.0f), ImGuiCond_Always);
        ImGuiWindowFlags flags = ImGuiWindowFlags_NoResize | ImGuiWindowFlags_NoCollapse | ImGuiWindowFlags_NoMove;

        if (ImGui::Begin("A Link Between Worlds — Native PC", nullptr, flags)) {
            if (ImGui::BeginTabBar("ALBWMainTabs")) {
                if (ImGui::BeginTabItem("Jogo")) {
                    ImGui::Spacing();
                    if (ImGui::Button("Continuar", ImVec2(-1.0f, 34.0f))) {
                        st->actions |= (1u << 0);
                    }
                    if (ImGui::Button("Reiniciar", ImVec2(-1.0f, 30.0f))) {
                        st->actions |= (1u << 1);
                    }
                    if (ImGui::Button("Tela Cheia (F11)", ImVec2(-1.0f, 30.0f))) {
                        st->actions |= (1u << 2);
                    }
                    ImGui::Separator();
                    ImGui::AlignTextToFramePadding();
                    ImGui::Text("Save State Slot:");
                    ImGui::SameLine();
                    ImGui::SetNextItemWidth(100.0f);
                    ImGui::InputInt("##slot", &st->slot);
                    if (st->slot < 1) st->slot = 1;
                    if (st->slot > 9) st->slot = 9;
                    float half = (ImGui::GetContentRegionAvail().x - ImGui::GetStyle().ItemSpacing.x) * 0.5f;
                    if (ImGui::Button("Salvar (F5)", ImVec2(half, 30.0f))) {
                        st->actions |= (1u << 3);
                    }
                    ImGui::SameLine();
                    ImGui::BeginDisabled(!st->slot_has_save);
                    if (ImGui::Button("Carregar (F8)", ImVec2(half, 30.0f))) {
                        st->actions |= (1u << 4);
                    }
                    ImGui::EndDisabled();
                    ImGui::Separator();
                    if (ImGui::Button("Controles e Avançado...", ImVec2(-1.0f, 30.0f))) {
                        st->actions |= (1u << 5);
                    }
                    if (ImGui::Button("Sair", ImVec2(-1.0f, 30.0f))) {
                        st->actions |= (1u << 6);
                    }
                    ImGui::EndTabItem();
                }

                if (ImGui::BeginTabItem("Vídeo")) {
                    ImGui::Spacing();
                    bool changed = false;
                    changed |= ImGui::Checkbox("Renderizador Hardware (Vulkan/GL)", &st->hardware_rasterizer);
                    ImGui::BeginDisabled(!st->hardware_rasterizer);
                    changed |= ImGui::SliderInt("Resolução Interna", &st->resolution, 1, 6, "%dx");
                    changed |= ImGui::Checkbox("Interpolação de Frames", &st->frame_interpolation);
                    ImGui::EndDisabled();
                    changed |= ImGui::SliderInt("Escala da Janela", &st->scale, 1, 6, "%dx");
                    const char* layouts[] = { "Empilhadas", "Lado a Lado", "Tela Principal Grande", "Só Superior", "Só Inferior" };
                    changed |= ImGui::Combo("Layout das Telas", &st->layout, layouts, 5);
                    const char* shapes[] = { "Console (5:3)", "Widescreen (16:9)", "Ultrawide (21:9)" };
                    changed |= ImGui::Combo("Proporção da Tela", &st->top_shape, shapes, 3);
                    const char* filters[] = { "Suave (Smooth)", "Nítido (Sharp)", "Pixels" };
                    changed |= ImGui::Combo("Filtro", &st->filter, filters, 3);
                    changed |= ImGui::Checkbox("Escala Inteira", &st->integer_scale);
                    changed |= ImGui::Checkbox("Mostrar FPS", &st->show_fps);
                    if (changed) st->actions |= (1u << 7);
                    ImGui::EndTabItem();
                }

                if (ImGui::BeginTabItem("Áudio")) {
                    ImGui::Spacing();
                    bool changed = false;
                    int vol_pct = (int)(st->volume * 100.0f + 0.5f);
                    if (ImGui::SliderInt("Volume", &vol_pct, 0, 100, "%d%%")) {
                        st->volume = (float)vol_pct / 100.0f;
                        changed = true;
                    }
                    changed |= ImGui::Checkbox("Mudo", &st->mute);
                    if (changed) st->actions |= (1u << 7);
                    ImGui::EndTabItem();
                }

                if (ImGui::BeginTabItem("Extras PC")) {
                    ImGui::Spacing();
                    ImGui::TextColored(ImVec4(0.90f, 0.74f, 0.28f, 1.0f), "Tela & Widescreen");
                    ImGui::Separator();
                    bool single_16_9 = (st->layout == 3 && st->top_shape == 1);
                    if (ImGui::Checkbox("Single Screen 16:9 (Tela Única)", &single_16_9)) {
                        if (single_16_9) {
                            st->layout = 3;    // Screens::TopOnly
                            st->top_shape = 1; // TopShape::Wide16x9
                        } else {
                            st->layout = 0;    // Screens::Stacked
                            st->top_shape = 0; // TopShape::Console
                        }
                        st->actions |= (1u << 7);
                    }
                    if (ImGui::SliderFloat("FOV (Câmera 3D)", &st->fov_scale, 0.85f, 1.25f, "%.2fx")) {
                        st->actions |= (1u << 7);
                    }
                    ImGui::SameLine();
                    if (ImGui::Button("Padrão##fov")) {
                        st->fov_scale = 1.0f;
                        st->actions |= (1u << 7);
                    }
                    if (ImGui::SliderFloat("Escala do HUD", &st->hud_scale, 0.55f, 1.00f, "%.2fx")) {
                        st->actions |= (1u << 7);
                    }
                    ImGui::SameLine();
                    if (ImGui::Button("Padrão##hud")) {
                        st->hud_scale = 1.0f;
                        st->actions |= (1u << 7);
                    }
                    ImGui::Spacing();
                    ImGui::TextColored(ImVec4(0.90f, 0.74f, 0.28f, 1.0f), "Em Desenvolvimento (M5)");
                    ImGui::Separator();
                    ImGui::BulletText("HUD nativo de PC (corações, rupees, itens X/Y)");
                    ImGui::BulletText("60 FPS+ / Frame pacing livre");
                    ImGui::BulletText("Câmera livre no analógico direito");
                    ImGui::EndTabItem();
                }
                ImGui::EndTabBar();
            }
        }
        ImGui::End();
    }

    ImGui::Render();

    g_vertices.clear();
    g_indices.clear();
    g_cmds.clear();

    ImDrawData* dd = ImGui::GetDrawData();
    if (!dd || dd->CmdListsCount == 0) return;

    float fb_w = dd->DisplaySize.x * dd->FramebufferScale.x;
    float fb_h = dd->DisplaySize.y * dd->FramebufferScale.y;
    ImVec2 clip_off = dd->DisplayPos;
    ImVec2 clip_scale = dd->FramebufferScale;

    for (int n = 0; n < dd->CmdListsCount; n++) {
        const ImDrawList* cmd_list = dd->CmdLists[n];
        uint32_t base_vtx = (uint32_t)g_vertices.size();
        uint32_t base_idx = (uint32_t)g_indices.size();

        for (int i = 0; i < cmd_list->VtxBuffer.Size; i++) {
            const ImDrawVert& v = cmd_list->VtxBuffer[i];
            uint8_t r = (uint8_t)(v.col & 0xFF);
            uint8_t g = (uint8_t)((v.col >> 8) & 0xFF);
            uint8_t b = (uint8_t)((v.col >> 16) & 0xFF);
            uint8_t a = (uint8_t)((v.col >> 24) & 0xFF);
            // Premultiplied sRGB RGBA for zakuro-gpu
            uint8_t pr = (uint8_t)(((uint32_t)r * a + 127) / 255);
            uint8_t pg = (uint8_t)(((uint32_t)g * a + 127) / 255);
            uint8_t pb = (uint8_t)(((uint32_t)b * a + 127) / 255);
            g_vertices.push_back(ImGuiBridgeVertex{
                v.pos.x * clip_scale.x,
                v.pos.y * clip_scale.y,
                v.uv.x,
                v.uv.y,
                pr, pg, pb, a
            });
        }
        for (int i = 0; i < cmd_list->IdxBuffer.Size; i++) {
            g_indices.push_back(cmd_list->IdxBuffer[i]);
        }

        for (int cmd_i = 0; cmd_i < cmd_list->CmdBuffer.Size; cmd_i++) {
            const ImDrawCmd& pcmd = cmd_list->CmdBuffer[cmd_i];
            if (pcmd.UserCallback != nullptr || pcmd.ElemCount == 0) continue;

            float cx1 = (pcmd.ClipRect.x - clip_off.x) * clip_scale.x;
            float cy1 = (pcmd.ClipRect.y - clip_off.y) * clip_scale.y;
            float cx2 = (pcmd.ClipRect.z - clip_off.x) * clip_scale.x;
            float cy2 = (pcmd.ClipRect.w - clip_off.y) * clip_scale.y;
            if (cx2 <= cx1 || cy2 <= cy1 || cx2 <= 0.0f || cy2 <= 0.0f || cx1 >= fb_w || cy1 >= fb_h) {
                continue;
            }
            if (cx1 < 0.0f) cx1 = 0.0f;
            if (cy1 < 0.0f) cy1 = 0.0f;
            if (cx2 > fb_w) cx2 = fb_w;
            if (cy2 > fb_h) cy2 = fb_h;

            g_cmds.push_back(ImGuiBridgeDrawCmd{
                (uint32_t)cx1,
                (uint32_t)cy1,
                (uint32_t)cx2,
                (uint32_t)cy2,
                base_vtx + pcmd.VtxOffset,
                (uint32_t)cmd_list->VtxBuffer.Size - pcmd.VtxOffset,
                base_idx + pcmd.IdxOffset,
                pcmd.ElemCount
            });
        }
    }
}

uint32_t albw_imgui_cmd_count() {
    return (uint32_t)g_cmds.size();
}

const ImGuiBridgeDrawCmd* albw_imgui_cmds() {
    return g_cmds.data();
}

const ImGuiBridgeVertex* albw_imgui_vertices() {
    return g_vertices.data();
}

const uint32_t* albw_imgui_indices() {
    return g_indices.data();
}

} // extern "C"

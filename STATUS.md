# ALBW Native Recompilation — Status do Projeto

Documento vivo para manter o contexto técnico, decisões de arquitetura e progresso da remoção gradual da emulação.

---

## Objetivo & Diretrizes

- **Meta:** Transformar *The Legend of Zelda: A Link Between Worlds* (`CTRJack` / `00040000000EC300`, USA) em um port nativo de PC independente do emulador Zakuro, removendo camadas de emulação aos poucos enquanto o jogo continua testável a cada passo.
- **Regra legal (estrita):**
  - Nunca versionar ROMs (`.cci`, `.3ds`, etc.), assets (`romfs/`) ou código C gerado automaticamente a partir da ROM (`albw-port/code/`, `librecomp.a`).
  - O repositório versiona apenas código próprio (`overrides/`, `albw-port/src/`, scripts) e o runtime open-source derivado do `zakuro/` (licença MIT).
- **Estratégia de repositório:** Monorepo em `/games/albw-recomp` (`main`), incorporando `zakuro/` diretamente para poder desmontar, fundir e substituir seus crates livremente sem depender de fork externo.

---

## Estrutura Atual (`/games/albw-recomp`)

| Caminho | Papel | Versionado no Git? |
|---|---|---|
| `overrides/` | Funções do jogo escritas à mão em C (`RECOMP_OVERRIDE`) | Sim |
| `zakuro/` | Runtime inicial em Rust (kernel HLE, GPU Vulkan, áudio, FS) | Sim (MIT) |
| `albw-port/` | Projeto Cargo do executável `albw-native` (sem `code/` e `target/`) | Sim (esqueleto) |
| `build.sh` | Automação (`runtime`, `full`, `run`) | Sim |
| `3dsrecomp/` | Recompilador estático ARM/Thumb/VFP → C | Não (ferramenta local) |
| `albw-decomp-ai/` | Base de consulta (~64,1% das funções do ALBW mapeadas/descompiladas) | Não (referência) |
| `*.cci` | ROM local do usuário para leitura de assets em runtime | Não |

---

## Fluxo de Trabalho

- **Alterou apenas Rust (`zakuro/` ou `albw-port/src/`):**
  ```bash
  ./build.sh runtime
  ```
- **Alterou ou adicionou overrides em C (`overrides/*.c`):**
  ```bash
  ./build.sh full
  ```
- **Testar o jogo:**
  ```bash
  ./build.sh run
  ```

---

## Progresso Atual

### Concluído
- [x] **M0 — Fundação & Monorepo:**
  - Compilação do `3dsrecomp` e análise da ROM (`CTRJack`: 25.242 funções, 0 módulos CRO, 91,6% código, 134 hints).
  - Geração e compilação do port standalone `albw-port` (`albw-native` testado e rodando).
  - Inicialização do Git com `.gitignore` blindado, `build.sh` e remoção de `zakuro/.git` (commit `1ac2ba2` na `main`).
  - Correção de `albw-port/Cargo.toml` para caminho relativo (`../zakuro/crates/zakuro`).
  - Primeiros 10 overrides nativos em C:
    - [overrides/core_math.c](overrides/core_math.c) (9 funções): `sead::Random::getU32()`, `Vec3::setAdd`, `Vec3::setSub`, `Vec3::setMul`, `Vec3::setDiv`, `Vec3::setScaleAdd`, `Vec3::normalize`, `Vec3::cross`, `Mtx33::fromMtx34`.
    - [overrides/core_mem.c](overrides/core_mem.c) (1 função): `__aeabi_memclr4`.

- [x] **M1 — Poda de `zakuro` & `zakuro-core`:**
  - Mapeamento em runtime via `albw-run.log`: 9 serviços (`APT:U`, `cecd:u`, `cfg:u`, `dsp::DSP`, `fs:USER`, `gsp::Gpu`, `hid:USER`, `ndm:u`, `ptm:u`), 23 SVCs e 0 IPCs não implementados.
  - Removidos 11 serviços ociosos (`boss`, `frd`, `ir`, `ir_rst`, `ldr_ro`, `nfc`, `y2r`, `cam`, `mic`, `uds`, `dlp`), subsistemas `camera`/`microphone`/`room`/`download_play`, UI de multiplayer/cheats/câmera/microfone e o recompilador/downloader Zig em runtime (`recompile.rs`, `zig.rs`, `recomp3ds`, `nokhwa`, `sha2`).

### Em andamento / Pendente imediato
- [ ] **M2 (em andamento):** Expandir `overrides/` (12 overrides core: `core_math.c` com 9, `core_mem.c` com 1, `core_sead.c` com 2) e zerar os últimos `0,10%` de fallback para `zakuro-cpu` (`0x0016AC14..0x0016AC34`, `0x00115E9C`).
- [ ] **M3 (em andamento):** Ponte direta [zakuro-core/src/native_bridge.rs](zakuro/crates/zakuro-core/src/native_bridge.rs) + [overrides/host_io.c](overrides/host_io.c) (4 overrides ativos sem `svc 0x32`: `gsp::Gpu#0x0008`, `dsp::DSP#0x0013` `FlushDataCache`, `gsp::Gpu#0x000C` `TriggerCmdReqQueue` e `FSFile#0x0802` `Read`). Próximo: abertura/fechamento de `fs` e áudio `dsp`.

---

## Marcos do Projeto (Roadmap de Remoção da Emulação)

| Marco | Foco | Entregáveis Principais | Critério de Pronto |
|---|---|---|---|
| **M0** | Fundação & Monorepo | `3dsrecomp`, `albw-port`, `build.sh`, monorepo `zakuro/`, primeiros overrides | `[x]` Jogo compilando e rodando via `./build.sh run` |
| **M1** | Poda de `zakuro` & `zakuro-core` | Mapear `srv`/`svc` usados pelo ALBW; remover serviços ociosos (`cam`, `nfc`, `uds`, `dlp`, `ir`, `cecd`, etc.), UI de multiplayer/cheats e recompilador em runtime | `[x]` Boot direto no ALBW com runtime enxuto e sem regressão |
| **M2** | Core Nativo & Zero `zakuro-cpu` | Expandir `overrides/` (matemática `sead`/VFP, `mem*`/`str*`, containers) com base em `albw-decomp-ai`; auditar `INTERPRET(...)` reais | `[ ]` Execução 100% nativa sem fallback para `zakuro-cpu` |
| **M3** | I/O Nativo (PC ↔ Jogo) | Substituir `hid` (input), `fs` (assets/save) e `dsp` (áudio) por chamadas diretas ao host sem IPC emulado (`svc 0x32`) | `[ ]` Input, arquivos e áudio sem camada HLE de serviço 3DS |
| **M4** | Kernel & GPU Desacoplados | Reduzir microkernel HLE (`svc`/threads/sync) a primitivas do SO e desacoplar renderização Vulkan (`zakuro-gpu`) do modelo de emulador | `[ ]` Executável nativo independente da arquitetura do emulador |

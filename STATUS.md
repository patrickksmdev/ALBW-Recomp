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
- [x] Compilação do `3dsrecomp` e análise da ROM (`CTRJack`: 25.242 funções, 0 módulos CRO, 91,6% código, 134 hints).
- [x] Geração e compilação do port standalone `albw-port` (`albw-native` testado e rodando).
- [x] Inicialização do Git com `.gitignore` blindado e criação do `build.sh`.
- [x] Correção de `albw-port/Cargo.toml` para caminho relativo (`../zakuro/crates/zakuro`).
- [x] Remoção de `zakuro/.git` para consolidar o monorepo.
- [x] Primeiros overrides nativos em C em `overrides/core_math.c` (9 funções) e `overrides/core_mem.c` (1 função):
  - `0x00111A64` — `sead::Random::getU32()`
  - `0x00115C54` — `Vec3::setAdd(const Vec3&, const Vec3&)`
  - `0x00115C70` — `Vec3::setSub(const Vec3&, const Vec3&)`
  - `0x00115D70` — `Vec3::setMul(const Vec3&, float)`
  - `0x00115D88` — `Vec3::setDiv(const Vec3&, float)`
  - `0x00115DAC` — `Vec3::setScaleAdd(float, const Vec3&, const Vec3&)`
  - `0x00115DC8` — `Vec3::normalize()`
  - `0x00115E10` — `Vec3::cross(const Vec3&, const Vec3&)`
  - `0x001151D8` — `Mtx33::fromMtx34(Mtx33*, const Mtx34*)`
  - `0x00110490` — `__aeabi_memclr4(void*, uint32_t)`

### Em andamento / Pendente imediato
- [ ] Fazer o commit inicial do monorepo na `main` (`git add . && git commit`).
- [ ] Mapear quais serviços 3DS (`srv`) e `svc` o ALBW realmente utiliza em runtime.

---

## Roadmap de Remoção da Emulação

1. **Enxugar `zakuro-core` e `zakuro`:**
   - Remover serviços de 3DS não utilizados pelo jogo (`cam`, `nfc`, `uds`, `dlp`, `ir`, `cecd`, etc.).
   - Remover UI/menus/cheats/biblioteca multi-jogo do emulador, deixando apenas o boot direto do ALBW.
2. **Zerar fallbacks do interpretador (`zakuro-cpu`):**
   - Monitorar instruções que ainda caem em `INTERPRET(...)` (110 ocorrências estáticas mapeadas no código gerado) e substituir funções VFP/asm por C nativo até remover a dependência de `zakuro-cpu`.
3. **Comunicação nativa PC ↔ Jogo:**
   - Substituir gradualmente `hid` (input), `fs` (arquivos) e `dsp` (áudio) por chamadas diretas ao host sem passar por IPC emulado (`svc 0x32`).
   - Expandir `overrides/` em lotes usando funções e structs já documentadas em `albw-decomp-ai`.

# DRW-012 — snapshots comparáveis: Drawing, SVG, PDF e Iced

> Cópia do corpus `featheroffice/fixtures/drw012/` do monorepo FeatherOffice, onde o FeatherSlide também o usa. Os comandos com `featherslide/` e `featheroffice/scripts/` valem dentro do monorepo.

**Status:** testes e corpus publicados na `main`; `[/]` até confirmação de Cargo e **captura visual real da GUI** em Linux/Windows. Este teste não pode, sozinho, provar igualdade de pixels rasterizados.

## Cena dourada

Uma `Cena` neutra de **160 × 100 pontos**, duas formas retangulares parcialmente sobrepostas: vermelho `#ff0000` ao fundo e azul `#0000ff` à frente. Possui geometria, cores e **ordem Z observáveis**, sem fontes, gradientes nem filtros que introduzam ambiguidades entre backends.

| Fixture | Contrato verificado |
| --- | --- |
| [snapshot.svg](snapshot.svg) | `viewBox`, comandos vetoriais, RGB, opacidade e ordem dos caminhos (byte a byte). |
| [pdf-comandos.txt](pdf-comandos.txt) | `stream` PDF 1.4, sistema Y invertido, comandos `m/l/h/f`, RGB e ordem Z (byte a byte, ignorando offsets e xref do contêiner). |
| [iced-coordenadas.csv](iced-coordenadas.csv) | Quatro âncoras: ponto documental, pixel lógico Iced e pixel físico na DPR 2. |

`feather-draw/tests/drw012_snapshots.rs` verifica SVG/PDF e conversão via `Viewport`. O teste `featherslide-gui/src/drawing_iced_snapshots.rs` percorre a **conversão de caminho real da GUI** `caminho_para_iced`, verifica `validar`, os pontos físicos/lógicos e a rejeição explícita de gradientes sem equivalência no Iced. Por isso não há renderização silenciosamente degradada.

### Política de tolerância

- **Vetores determinísticos (SVG e stream PDF):** snapshot exato. A mudança requer análise e atualização deliberada dos arquivos versionados. PDF completo não é comparado byte a byte porque os offsets e a estrutura de objetos não são o desenho propriamente dito.
- **Coordenadas Iced:** até **±0,25 px lógico** e **±0,5 px físico**, além de **±0,001 ponto** no retorno `Viewport::documento`. A DPR não deve alterar `logico`.
- **Cores na rasterização real:** diferença de até **10/255 por canal RGB** em cada pixel; permite até **2% dos pixels** excederem esse valor (antialiasing e arredondamento do framebuffer). Aplicar o mesmo perfil de cor, opacidade sobre branco e escala, e inspecionar divergências perto de bordas.
- **Alinhamento obrigatório:** antes da comparação PPM P6, todas as imagens devem ter **exatamente a mesma largura, altura, origem, zoom, DPR e plano de fundo**. O comparador falha com dimensões diferentes. Não aplica deslocamento ou blur automático, porque isso poderia encobrir um bug de geometria.

No snapshot de `Viewport`, a escala é **1,5 px lógico/ponto**, origem Iced `(3,5)` px lógicos e DPR `2`. Um ponto de documento `(10,10)` vira `(18,20)` px lógicos e `(36,40)` px físicos. Para comparar renders reais em PPM, normalizar os vetores para 3 px físicos por ponto (480 × 300 px) e posicionar em `(+6,+10)` px sobre um canvas **486 × 310 px**, equivalente ao viewport da amostra; a captura Iced deve usar a mesma configuração.

## Validar após git pull

```bash
git pull --ff-only origin main

# Testa backends neutros e fixture dourada.
cargo test --test drw012_snapshots -j 2

# Cria arquivos reais com os mesmos bytes/comandos comparados no teste.
cargo run --example drw012_snapshots -j 2 -- /tmp/featheroffice-drw012

# Compara a ponte real de desenho do FeatherSlide/Iced (sem gerar screenshots).
cargo test --manifest-path featherslide/crates/featherslide-gui/Cargo.toml drw012_ -j 2 -- --nocapture

# Testes do comparador de pixels sem dependências externas.
python3 -m unittest discover -s featheroffice/scripts -p 'test_drw012_comparar_ppm.py' -v
```

Arquivos resultantes: `/tmp/featheroffice-drw012/snapshot.svg`, `snapshot.pdf` e `pdf-comandos.txt`.

### Comparação de renders reais, após captura/alinhamento

Rasterizar o SVG e o PDF (por exemplo, com Inkscape e Poppler instalados **apenas na máquina de teste**), posicionar ambos com a origem e escala descritas acima e salvar **PPM binário P6 de 8 bits, plano de fundo branco**. Capturar a mesma cena no FeatherSlide Iced com o mesmo viewport e salvar como `iced.ppm` depois de normalizar dimensões.

```bash
python3 featheroffice/scripts/drw012_comparar_ppm.py \
  /tmp/drw012-svg.ppm /tmp/drw012-pdf.ppm /tmp/drw012-iced.ppm \
  --tolerancia-canal 10 --max-percentual 2
```

O resultado JSON informa diferença máxima/média por canal, quantidade e percentual de pixels divergentes em cada saída. **Exit code 0** significa que todas as comparações passaram, **1** significa divergência acima do limiar, **2** indica entrada inválida. A comparação real depende de três PPMs gerados/alinhados e **ainda não ocorreu nesta entrega**.

As GUIs Iced **não são executadas em modo headless** por esses testes. Não afirmar paridade visual Linux/Windows nem marcar `[x]` sem smoke real, capturas e aceite do mantenedor.

## Arquivos e integração

- `tests/drw012_snapshots.rs`.
- `examples/drw012_snapshots.rs`.
- `featherslide/crates/featherslide-gui/src/drawing_iced_snapshots.rs`.
- `featheroffice/scripts/drw012_comparar_ppm.py` e `test_drw012_comparar_ppm.py`.
- CI: `.github/workflows/featheroffice-foundation.yml` e `.github/workflows/featherslide-rust.yml`.

**Nenhum formato proprietário `.fsdz`, `.fdoz` ou `.fshz` foi alterado.**

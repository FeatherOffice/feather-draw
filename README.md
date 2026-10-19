# feather-draw

Motor vetorial compartilhado da suíte **FeatherWorks**: geometria, caminhos,
estilos, unidades, exportação para SVG e PDF e a tinta à mão livre da aba
Desenhar, com pressão da caneta.

É uma **biblioteca**, não um aplicativo: cada programa da suíte a compila
dentro do próprio executável. Quem instala só a planilha não precisa instalar
`feather-draw` à parte, nem DLL ou `.so`.

Antes se chamava `featheroffice-drawing` e morava no monorepo FeatherWorks;
os motores comuns perderam o "office" no nome, no mesmo padrão da
[`feather-ui`](https://github.com/feather-works/feather-ui).

## Por que ela existe

Gráficos, formas, tinta e páginas exportadas precisam sair iguais na tela
(Iced), no PDF e no SVG, em todos os aplicativos. Por isso o desenho é
descrito uma vez só, numa `Cena` neutra de `Figura`s, e cada saída apenas a
traduz. Todos os aplicativos usam **a mesma versão** deste crate, para que os
tipos (`Cena`, `Figura`, `Tinta`…) sejam os mesmos entre o motor de gráficos,
os formatos de arquivo e as interfaces.

O crate **não depende de GUI nem de formato de arquivo** e não tem
dependências externas: só a biblioteca padrão do Rust.

## O que tem

| Módulo | Para quê |
|---|---|
| raiz (`lib.rs`) | `Ponto`, `Retangulo`, `Cor`, `Transformacao`, `Figura`, `Cena` |
| `geometria` | hit-test, snap, limites e alinhamento em coordenadas de documento |
| `caminhos` | caminhos vetoriais, curvas de Bézier, arcos e aproximação controlada |
| `estilos` | traço, tracejado, junção, extremos, preenchimento sólido e gradiente, estilo de texto |
| `unidades` | pontos, pixels lógicos e físicos; `Viewport` e grade visível |
| `svg` | exportador SVG 1.1 determinístico (mesma cena, mesmos bytes) |
| `pdf` | exportador PDF 1.4 vetorial e determinístico, com texto em Helvetica (WinAnsi) |
| `tinta` | tinta à mão livre: traços com pressão e tempo, suavização, borracha, laço, reconhecimento de forma, contorno de largura variável, formato de texto próprio e polilinha DrawingML |

## Quem usa

FeatherSheet, FeatherSlide, FeatherDocs, FeatherPDF e FeatherCAD, além dos
motores comuns do monorepo FeatherWorks (`featheroffice-charts`,
`featheroffice-io`, `featheroffice-app-adapters`). A interface da aba
Desenhar fica na `feather-ui` (módulo `tinta`); o motor dos traços fica aqui.

## Como usar

No `Cargo.toml` do aplicativo:

```toml
[dependencies]
feather-draw = { git = "https://github.com/feather-works/feather-draw", tag = "v0.1.0" }
```

Para desenvolver a biblioteca e o aplicativo lado a lado, aponte para a cópia
local sem mudar a dependência acima (no pacote raiz da compilação):

```toml
[patch."https://github.com/feather-works/feather-draw"]
feather-draw = { path = "../feather-draw" }
```

No código:

```rust
use feather_draw::{exportar_svg, Cena, Cor, Figura, Ponto};
use feather_draw::tinta::{Tinta, Traco, CurvaPressao};
```

## Desenvolvimento

```bash
cargo test
cargo clippy --all-targets
cargo run --example drw012_snapshots -- /tmp/feather-draw-drw012
```

Os testes de `tests/drw012_snapshots.rs` comparam SVG e comandos PDF com o
corpus dourado de [`fixtures/drw012/`](fixtures/drw012/README.md). Uma mudança
nesses arquivos precisa ser deliberada e conferida.

## Privacidade

Nenhum código faz acesso à rede, telemetria ou leitura de arquivos por conta
própria: o crate só transforma dados em memória que o aplicativo entrega.

## Licença

GPL-3.0 (ver [`LICENSE`](LICENSE)).

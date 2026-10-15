//! Tinta à mão livre (aba Desenhar da suíte): traços de caneta e de
//! marca-texto com pressão e tempo opcionais, suavização e simplificação
//! (para não guardar milhares de pontos), borracha por traço inteiro e por
//! ponto (que divide o traço), seleção por laço, "tinta para forma" e
//! conversão para [`Caminho`]/[`Figura`] (Iced, PDF e SVG desenham igual).
//!
//! Unidade neutra, como o resto do crate: o consumidor escolhe (pixels da
//! planilha a 100 %, pontos do slide…) e a mantém. Sem dependência de GUI.
//! Formato de texto próprio em [`texto`]; polilinha DrawingML em [`drawingml`].

pub mod drawingml;
pub mod formas;
pub mod pressao;
pub mod texto;

pub use drawingml::{
    traco_de_drawingml, traco_para_drawingml, traco_para_drawingml_com_pressao, DesenhoDrawingMl, EMU_POR_PIXEL,
};
pub use formas::{reconhecer_forma, Forma};
pub use pressao::{
    caminho_de_poligonos, contorno_largura_variavel, pressao_da_velocidade, pressoes_por_velocidade,
    simplificar_com_pressao, CurvaPressao, PRESSAO_SINTETICA_MIN, VELOCIDADE_FINA,
};
pub use texto::ErroTinta;

use crate::{
    Caminho, Comando, Cor, EstiloVetor, Extremo, Figura, Juncao, Ponto, Retangulo, Stroke,
    Transformacao,
};

/// Fração mínima dos pontos de um traço que precisa cair dentro do laço
/// para o traço ser selecionado (o laço desenhado à mão é impreciso).
pub const FRACAO_LACO: f32 = 0.75;

/// Um ponto capturado: posição, pressão da caneta (`0..=1`, quando o
/// dispositivo informa) e instante em milissegundos desde o início do traço.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PontoTinta {
    pub x: f32,
    pub y: f32,
    pub pressao: Option<f32>,
    pub tempo: Option<u32>,
}

impl PontoTinta {
    pub const fn novo(x: f32, y: f32) -> Self {
        Self { x, y, pressao: None, tempo: None }
    }
    pub fn com_pressao(mut self, pressao: f32) -> Self {
        self.pressao = pressao.is_finite().then_some(pressao.clamp(0.0, 1.0));
        self
    }
    pub fn com_tempo(mut self, ms: u32) -> Self {
        self.tempo = Some(ms);
        self
    }
    pub const fn ponto(self) -> Ponto {
        Ponto::novo(self.x, self.y)
    }
    fn valido(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl From<Ponto> for PontoTinta {
    fn from(p: Ponto) -> Self {
        Self::novo(p.x, p.y)
    }
}

/// Ferramenta que fez o traço. O marca-texto nasce translúcido e largo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ferramenta {
    Caneta,
    MarcaTexto,
}

impl Ferramenta {
    pub const fn opacidade_padrao(self) -> f32 {
        match self {
            Self::Caneta => 1.0,
            Self::MarcaTexto => 0.4,
        }
    }
}

/// Um traço de tinta. `id` é atribuído por [`Tinta::adicionar`] e não muda
/// enquanto o traço existir (a borracha de ponto preserva o id no primeiro
/// pedaço). `reto` liga os pontos por segmentos retos (formas reconhecidas);
/// falso desenha uma curva suave passando perto dos pontos.
#[derive(Debug, Clone, PartialEq)]
pub struct Traco {
    pub id: u64,
    pub pontos: Vec<PontoTinta>,
    pub cor: Cor,
    pub espessura: f32,
    pub ferramenta: Ferramenta,
    /// Opacidade de todo o traço em `[0,1]`, multiplicada pelo alfa da cor.
    pub opacidade: f32,
    pub reto: bool,
    /// A pressão dos pontos foi estimada pela velocidade (sem mesa), não
    /// medida pela caneta. Gravada como `v` no texto.
    pub pressao_sintetica: bool,
}

impl Traco {
    pub fn novo(pontos: Vec<PontoTinta>, cor: Cor, espessura: f32, ferramenta: Ferramenta) -> Self {
        Self {
            id: 0,
            pontos,
            cor,
            espessura,
            ferramenta,
            opacidade: ferramenta.opacidade_padrao(),
            reto: false,
            pressao_sintetica: false,
        }
    }

    /// Válido para guardar/desenhar: ao menos um ponto, tudo finito.
    pub fn valido(&self) -> bool {
        !self.pontos.is_empty()
            && self.pontos.iter().all(|p| p.valido())
            && self.espessura.is_finite()
            && self.espessura > 0.0
            && self.opacidade.is_finite()
            && (0.0..=1.0).contains(&self.opacidade)
    }

    /// Caixa dos pontos, sem a espessura.
    pub fn limites(&self) -> Option<Retangulo> {
        limites_de(self.pontos.iter().map(|p| p.ponto()))
    }

    /// Suaviza levemente e simplifica (Douglas-Peucker): um traço de mil
    /// pontos capturados vira algumas dezenas, sem mudar o desenho além de
    /// `tolerancia` (mesma unidade dos pontos).
    pub fn preparar(&mut self, tolerancia: f32) {
        if self.pontos.len() > 2 {
            let suave = suavizar(&self.pontos, 1);
            self.pontos = if self.tem_pressao() {
                // A força também conta: a variação de raio acima da tolerância fica.
                simplificar_com_pressao(&suave, tolerancia, self.espessura * 0.5 * (1.0 - CurvaPressao::PADRAO.minimo))
            } else {
                simplificar(&suave, tolerancia)
            };
        }
    }

    /// Menor distância do ponto até a linha central do traço.
    pub fn distancia_ate(&self, p: Ponto) -> f32 {
        match self.pontos.as_slice() {
            [] => f32::INFINITY,
            [unico] => distancia(unico.ponto(), p),
            pts => pts
                .windows(2)
                .map(|s| distancia_segmento(p, s[0].ponto(), s[1].ponto()))
                .fold(f32::INFINITY, f32::min),
        }
    }

    /// O traço passa a até `raio` do ponto (considerando a própria espessura)?
    pub fn atinge(&self, p: Ponto, raio: f32) -> bool {
        self.distancia_ate(p) <= raio.max(0.0) + self.espessura / 2.0
    }

    pub fn transladar(&mut self, dx: f32, dy: f32) {
        for p in &mut self.pontos {
            p.x += dx;
            p.y += dy;
        }
    }

    /// Aplica a transformação aos pontos; a espessura acompanha a escala média.
    pub fn transformar(&self, t: Transformacao) -> Self {
        let escala = (t.a * t.d - t.b * t.c).abs().sqrt();
        let mut novo = self.clone();
        for p in &mut novo.pontos {
            let q = t.aplicar(p.ponto());
            p.x = q.x;
            p.y = q.y;
        }
        if escala.is_finite() && escala > 0.0 {
            novo.espessura *= escala;
        }
        novo
    }

    /// Caminho da linha central: segmentos retos (`reto`) ou curvas
    /// quadráticas pelos pontos médios (traço à mão, sem quinas).
    pub fn caminho(&self) -> Caminho {
        let mut caminho = Caminho::novo();
        let pts: Vec<Ponto> = self.pontos.iter().map(|p| p.ponto()).collect();
        let Some(&primeiro) = pts.first() else { return caminho };
        caminho.adicionar(Comando::Mover(primeiro));
        if pts.len() == 1 {
            // Toque sem arrastar: ponto redondo (extremo redondo desenha um disco).
            caminho.adicionar(Comando::Linha(primeiro));
            return caminho;
        }
        if self.reto || pts.len() == 2 {
            for &p in &pts[1..] {
                caminho.adicionar(Comando::Linha(p));
            }
            return caminho;
        }
        for i in 1..pts.len() - 1 {
            caminho.adicionar(Comando::Quadratica {
                controle: pts[i],
                fim: meio(pts[i], pts[i + 1]),
            });
        }
        caminho.adicionar(Comando::Linha(pts[pts.len() - 1]));
        caminho
    }

    /// Estilo de desenho: contorno redondo, sem preenchimento.
    pub fn estilo(&self) -> EstiloVetor {
        let mut stroke = Stroke::novo(self.cor, self.espessura);
        stroke.extremo = match self.ferramenta {
            Ferramenta::Caneta => Extremo::Redondo,
            Ferramenta::MarcaTexto => Extremo::Quadrado,
        };
        stroke.juncao = Juncao::Redonda;
        EstiloVetor {
            preenchimento: None,
            stroke: Some(stroke),
            opacidade: self.opacidade.clamp(0.0, 1.0),
            texto: None,
        }
    }

    /// Figura pronta para a `Cena` (SVG, PDF e Iced).
    pub fn figura(&self) -> Figura {
        Figura::Caminho { caminho: self.caminho(), estilo: self.estilo() }
    }

    /// Reconhece a forma desenhada (linha, seta, retângulo, triângulo,
    /// elipse/círculo). `None` quando o traço não parece nenhuma delas.
    pub fn reconhecer_forma(&self) -> Option<Forma> {
        let pts: Vec<Ponto> = self.pontos.iter().map(|p| p.ponto()).collect();
        reconhecer_forma(&pts)
    }

    /// O mesmo traço (id, cor, espessura, ferramenta) com o desenho exato da forma.
    pub fn com_forma(&self, forma: &Forma) -> Self {
        Self {
            pontos: forma.pontos().into_iter().map(PontoTinta::from).collect(),
            reto: forma.poligonal(),
            // A forma perfeita sai com largura constante.
            pressao_sintetica: false,
            ..self.clone()
        }
    }
}

/// Conjunto de traços de um documento (uma aba, um slide, uma página).
/// A ordem da lista é a ordem de desenho (o último fica por cima).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tinta {
    tracos: Vec<Traco>,
    proximo_id: u64,
}

impl Tinta {
    pub fn nova() -> Self {
        Self::default()
    }
    pub fn tracos(&self) -> &[Traco] {
        &self.tracos
    }
    pub fn len(&self) -> usize {
        self.tracos.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tracos.is_empty()
    }
    fn novo_id(&mut self) -> u64 {
        self.proximo_id = self.proximo_id.max(1);
        let id = self.proximo_id;
        self.proximo_id += 1;
        id
    }

    /// Acrescenta por cima dos demais com um id novo (o `id` recebido é
    /// ignorado). Devolve o id, ou `None` se o traço for inválido.
    pub fn adicionar(&mut self, mut traco: Traco) -> Option<u64> {
        if !traco.valido() {
            return None;
        }
        traco.id = self.novo_id();
        let id = traco.id;
        self.tracos.push(traco);
        Some(id)
    }

    /// Acrescenta mantendo o id (leitura de arquivo). Recusa id 0 ou repetido.
    pub fn inserir_com_id(&mut self, traco: Traco) -> Result<(), ErroTinta> {
        if traco.id == 0 || self.obter(traco.id).is_some() {
            return Err(ErroTinta::IdRepetido(traco.id));
        }
        if !traco.valido() {
            return Err(ErroTinta::TracoInvalido(traco.id));
        }
        self.proximo_id = self.proximo_id.max(traco.id + 1);
        self.tracos.push(traco);
        Ok(())
    }

    pub fn obter(&self, id: u64) -> Option<&Traco> {
        self.tracos.iter().find(|t| t.id == id)
    }
    pub fn obter_mut(&mut self, id: u64) -> Option<&mut Traco> {
        self.tracos.iter_mut().find(|t| t.id == id)
    }
    pub fn remover(&mut self, id: u64) -> Option<Traco> {
        let i = self.tracos.iter().position(|t| t.id == id)?;
        Some(self.tracos.remove(i))
    }
    pub fn limpar(&mut self) {
        self.tracos.clear();
    }

    /// Traço mais de cima atingido no ponto.
    pub fn traco_em(&self, p: Ponto, raio: f32) -> Option<u64> {
        self.tracos.iter().rev().find(|t| t.atinge(p, raio)).map(|t| t.id)
    }

    /// Borracha de traço: apaga o traço inteiro mais de cima sob o ponto.
    pub fn apagar_traco_em(&mut self, p: Ponto, raio: f32) -> Option<Traco> {
        let id = self.traco_em(p, raio)?;
        self.remover(id)
    }

    /// Borracha de traço arrastada: apaga todo traço que o caminho tocar.
    pub fn apagar_tracos_no_caminho(&mut self, caminho: &[Ponto], raio: f32) -> Vec<Traco> {
        let amostras = amostrar_caminho(caminho, raio);
        let mut apagados = Vec::new();
        self.tracos.retain(|t| {
            let tocado = amostras.iter().any(|&p| t.atinge(p, raio));
            if tocado {
                apagados.push(t.clone());
            }
            !tocado
        });
        apagados
    }

    /// Borracha de ponto: tira a parte dos traços dentro do disco
    /// (`centro`, `raio`) e divide o que sobrar. O primeiro pedaço mantém o
    /// id; os demais ganham ids novos e ficam logo acima, na mesma ordem.
    /// Devolve se algo mudou.
    pub fn apagar_pontos_em(&mut self, centro: Ponto, raio: f32) -> bool {
        if !(raio.is_finite() && raio > 0.0) {
            return false;
        }
        let mut mudou = false;
        let mut saida = Vec::with_capacity(self.tracos.len());
        let antigos = std::mem::take(&mut self.tracos);
        for traco in antigos {
            match cortar_no_disco(&traco, centro, raio) {
                None => saida.push(traco),
                Some(pedacos) => {
                    mudou = true;
                    for (i, pontos) in pedacos.into_iter().enumerate() {
                        let mut novo = Traco { pontos, ..traco.clone() };
                        novo.id = if i == 0 { traco.id } else { 0 };
                        saida.push(novo);
                    }
                }
            }
        }
        for t in &mut saida {
            if t.id == 0 {
                t.id = self.novo_id();
            }
        }
        self.tracos = saida;
        mudou
    }

    /// Borracha de ponto arrastada ao longo de um caminho.
    pub fn apagar_pontos_no_caminho(&mut self, caminho: &[Ponto], raio: f32) -> bool {
        let mut mudou = false;
        for p in amostrar_caminho(caminho, raio) {
            mudou |= self.apagar_pontos_em(p, raio);
        }
        mudou
    }

    /// Ids dos traços cercados pelo laço (polígono fechado implícito):
    /// ao menos [`FRACAO_LACO`] dos pontos do traço caem dentro.
    pub fn selecionar_laco(&self, laco: &[Ponto]) -> Vec<u64> {
        if laco.len() < 3 {
            return Vec::new();
        }
        self.tracos
            .iter()
            .filter(|t| {
                let dentro = t.pontos.iter().filter(|p| ponto_no_poligono(p.ponto(), laco)).count();
                !t.pontos.is_empty() && dentro as f32 >= FRACAO_LACO * t.pontos.len() as f32
            })
            .map(|t| t.id)
            .collect()
    }

    pub fn mover(&mut self, ids: &[u64], dx: f32, dy: f32) {
        for t in self.tracos.iter_mut().filter(|t| ids.contains(&t.id)) {
            t.transladar(dx, dy);
        }
    }

    /// Caixa que contém os traços indicados (todos, se `ids` vazio).
    pub fn limites(&self, ids: &[u64]) -> Option<Retangulo> {
        limites_de(
            self.tracos
                .iter()
                .filter(|t| ids.is_empty() || ids.contains(&t.id))
                .flat_map(|t| t.pontos.iter().map(|p| p.ponto())),
        )
    }

    /// Troca o traço `id` pela forma reconhecida (tinta para forma).
    /// Devolve a forma aplicada.
    pub fn converter_em_forma(&mut self, id: u64) -> Option<Forma> {
        let traco = self.obter(id)?;
        let forma = traco.reconhecer_forma()?;
        let novo = traco.com_forma(&forma);
        *self.obter_mut(id)? = novo;
        Some(forma)
    }
}

/// Média ponderada 1-2-1 em `passadas` rodadas. As pontas ficam paradas;
/// a pressão é suavizada junto quando os três vizinhos a têm.
pub fn suavizar(pontos: &[PontoTinta], passadas: usize) -> Vec<PontoTinta> {
    let mut atual = pontos.to_vec();
    for _ in 0..passadas {
        if atual.len() < 3 {
            break;
        }
        let mut novo = atual.clone();
        for i in 1..atual.len() - 1 {
            let (a, b, c) = (atual[i - 1], atual[i], atual[i + 1]);
            novo[i].x = 0.25 * a.x + 0.5 * b.x + 0.25 * c.x;
            novo[i].y = 0.25 * a.y + 0.5 * b.y + 0.25 * c.y;
            if let (Some(pa), Some(pb), Some(pc)) = (a.pressao, b.pressao, c.pressao) {
                novo[i].pressao = Some(0.25 * pa + 0.5 * pb + 0.25 * pc);
            }
        }
        atual = novo;
    }
    atual
}

/// Douglas-Peucker (sem recursão): mantém os pontos que se afastam mais de
/// `tolerancia` da corda. Pontas sempre ficam. Pressão e tempo acompanham.
pub fn simplificar(pontos: &[PontoTinta], tolerancia: f32) -> Vec<PontoTinta> {
    let pts: Vec<Ponto> = pontos.iter().map(|p| p.ponto()).collect();
    simplificar_indices(&pts, tolerancia).into_iter().map(|i| pontos[i]).collect()
}

/// Índices mantidos pelo Douglas-Peucker, em ordem.
pub(crate) fn simplificar_indices(pts: &[Ponto], tolerancia: f32) -> Vec<usize> {
    let n = pts.len();
    if n <= 2 {
        return (0..n).collect();
    }
    let tol = if tolerancia.is_finite() && tolerancia > 0.0 { tolerancia } else { 0.0 };
    let mut manter = vec![false; n];
    manter[0] = true;
    manter[n - 1] = true;
    let mut pilha = vec![(0usize, n - 1)];
    while let Some((i, j)) = pilha.pop() {
        if j <= i + 1 {
            continue;
        }
        let (mut maior, mut indice) = (-1.0f32, i);
        for k in i + 1..j {
            let d = distancia_segmento(pts[k], pts[i], pts[j]);
            if d > maior {
                maior = d;
                indice = k;
            }
        }
        if maior > tol {
            manter[indice] = true;
            pilha.push((i, indice));
            pilha.push((indice, j));
        }
    }
    (0..n).filter(|&k| manter[k]).collect()
}

/// Ponto dentro do polígono (regra par-ímpar, raio horizontal).
pub fn ponto_no_poligono(p: Ponto, poligono: &[Ponto]) -> bool {
    let n = poligono.len();
    if n < 3 {
        return false;
    }
    let mut dentro = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poligono[i], poligono[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                dentro = !dentro;
            }
        }
        j = i;
    }
    dentro
}

// ---------------------------------------------------------------- auxiliares

pub(crate) fn distancia(a: Ponto, b: Ponto) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

pub(crate) fn meio(a: Ponto, b: Ponto) -> Ponto {
    Ponto::novo((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

pub(crate) fn distancia_segmento(p: Ponto, a: Ponto, b: Ponto) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    if l2 <= f32::EPSILON {
        return distancia(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0);
    distancia(p, Ponto::novo(a.x + t * dx, a.y + t * dy))
}

pub(crate) fn limites_de(pontos: impl Iterator<Item = Ponto>) -> Option<Retangulo> {
    let mut caixa: Option<(f32, f32, f32, f32)> = None;
    for p in pontos {
        let c = caixa.get_or_insert((p.x, p.y, p.x, p.y));
        c.0 = c.0.min(p.x);
        c.1 = c.1.min(p.y);
        c.2 = c.2.max(p.x);
        c.3 = c.3.max(p.y);
    }
    caixa.map(|(x0, y0, x1, y1)| Retangulo::novo(x0, y0, x1 - x0, y1 - y0))
}

fn interpolar(a: PontoTinta, b: PontoTinta, t: f32) -> PontoTinta {
    PontoTinta {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        pressao: match (a.pressao, b.pressao) {
            (Some(pa), Some(pb)) => Some(pa + (pb - pa) * t),
            (p, None) | (None, p) => p,
        },
        tempo: match (a.tempo, b.tempo) {
            (Some(ta), Some(tb)) => Some((ta as f32 + (tb as f32 - ta as f32) * t).round() as u32),
            (p, None) | (None, p) => p,
        },
    }
}

/// Amostras do caminho da borracha a cada meio raio (arrasto rápido não pula traço).
fn amostrar_caminho(caminho: &[Ponto], raio: f32) -> Vec<Ponto> {
    let passo = (raio / 2.0).max(0.5);
    let mut saida = Vec::new();
    for (i, &p) in caminho.iter().enumerate() {
        if i == 0 {
            saida.push(p);
            continue;
        }
        let a = caminho[i - 1];
        let d = distancia(a, p);
        let n = (d / passo).ceil().max(1.0) as usize;
        for k in 1..=n.min(10_000) {
            let t = k as f32 / n as f32;
            saida.push(Ponto::novo(a.x + (p.x - a.x) * t, a.y + (p.y - a.y) * t));
        }
    }
    saida
}

/// Intervalo `[t0, t1]` (dentro de `[0,1]`) do segmento a→b que fica no disco.
fn trecho_no_disco(a: Ponto, b: Ponto, c: Ponto, r: f32) -> Option<(f32, f32)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (fx, fy) = (a.x - c.x, a.y - c.y);
    let qa = dx * dx + dy * dy;
    let qc = fx * fx + fy * fy - r * r;
    if qa <= f32::EPSILON {
        return (qc <= 0.0).then_some((0.0, 1.0));
    }
    let qb = 2.0 * (fx * dx + fy * dy);
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let raiz = disc.sqrt();
    let t0 = ((-qb - raiz) / (2.0 * qa)).max(0.0);
    let t1 = ((-qb + raiz) / (2.0 * qa)).min(1.0);
    (t0 < t1 || (t0 <= t1 && qc <= 0.0)).then_some((t0, t1))
}

/// `None` se o disco não toca o traço; senão os pedaços que sobram
/// (cada um com ao menos 2 pontos, ou um toque isolado fora do disco).
fn cortar_no_disco(traco: &Traco, c: Ponto, r: f32) -> Option<Vec<Vec<PontoTinta>>> {
    let pts = &traco.pontos;
    if pts.len() == 1 {
        return (distancia(pts[0].ponto(), c) <= r).then(Vec::new);
    }
    let mut tocou = false;
    let mut pedacos: Vec<Vec<PontoTinta>> = Vec::new();
    let mut atual: Vec<PontoTinta> = Vec::new();
    for s in pts.windows(2) {
        let (a, b) = (s[0], s[1]);
        match trecho_no_disco(a.ponto(), b.ponto(), c, r) {
            None => {
                if atual.is_empty() {
                    atual.push(a);
                }
                atual.push(b);
            }
            Some((t0, t1)) => {
                tocou = true;
                if t0 > 0.0 {
                    if atual.is_empty() {
                        atual.push(a);
                    }
                    atual.push(interpolar(a, b, t0));
                }
                if !atual.is_empty() {
                    pedacos.push(std::mem::take(&mut atual));
                }
                if t1 < 1.0 {
                    atual.push(interpolar(a, b, t1));
                    atual.push(b);
                }
            }
        }
    }
    if !atual.is_empty() {
        pedacos.push(atual);
    }
    if !tocou {
        return None;
    }
    // Pedaço degenerado (comprimento quase nulo) é só a borda do disco.
    pedacos.retain(|p| {
        p.len() >= 2 && p.windows(2).map(|s| distancia(s[0].ponto(), s[1].ponto())).sum::<f32>() > r * 0.05
    });
    Some(pedacos)
}

#[cfg(test)]
mod testes;

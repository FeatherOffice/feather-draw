//! Pressão da caneta: curva de resposta, pressão sintética pela velocidade
//! (quando não há mesa) e o **contorno de largura variável** — o polígono
//! preenchido que tela, PDF, SVG e DrawingML desenham igual.
//!
//! A largura num ponto é `espessura × (mínimo + (1 − mínimo) × pressão^gama)`.
//! Ponto sem pressão vale fator 1 (a espessura cheia), então um traço sem
//! pressão nenhuma tem a mesma largura do traço comum.
use super::{distancia, Ferramenta, PontoTinta, Traco};
use crate::{Caminho, Comando, EstiloVetor, Figura, Ponto, Preenchimento};

/// Curva de resposta: quanto a pressão afina o traço.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurvaPressao {
    /// Fração da espessura com a pressão mínima (`0..=1`).
    pub minimo: f32,
    /// Expoente aplicado à pressão: `< 1` engrossa com pouca força, `> 1` pede mais força.
    pub gama: f32,
}

impl CurvaPressao {
    /// A curva com que os traços guardados são **desenhados** (tela, PDF,
    /// SVG, DrawingML). A pressão guardada já é a "pressão de resposta"
    /// (o aplicativo aplica a curva da pessoa ao capturar), por isso gama 1.
    pub const PADRAO: Self = Self { minimo: 0.2, gama: 1.0 };
    pub const GAMA_MIN: f32 = 0.1;
    pub const GAMA_MAX: f32 = 10.0;

    /// Curva com valores limitados (NaN volta ao padrão).
    pub fn nova(minimo: f32, gama: f32) -> Self {
        Self {
            minimo: if minimo.is_finite() { minimo.clamp(0.0, 1.0) } else { Self::PADRAO.minimo },
            gama: if gama.is_finite() { gama.clamp(Self::GAMA_MIN, Self::GAMA_MAX) } else { 1.0 },
        }
    }

    /// `pressão^gama`, em `0..=1`.
    pub fn resposta(self, pressao: f32) -> f32 {
        if !pressao.is_finite() {
            return 1.0;
        }
        pressao.clamp(0.0, 1.0).powf(self.gama.max(Self::GAMA_MIN))
    }

    /// Fração da espessura: `mínimo + (1 − mínimo) × resposta`. Sem pressão = 1.
    pub fn fator(self, pressao: Option<f32>) -> f32 {
        match pressao {
            None => 1.0,
            Some(p) => {
                let m = self.minimo.clamp(0.0, 1.0);
                m + (1.0 - m) * self.resposta(p)
            }
        }
    }

    pub fn largura(self, espessura: f32, pressao: Option<f32>) -> f32 {
        espessura * self.fator(pressao)
    }
}

impl Default for CurvaPressao {
    fn default() -> Self {
        Self::PADRAO
    }
}

/// Velocidade (unidades do documento por milissegundo) em que a pressão
/// sintética chega ao mínimo. 2 px/ms = 2000 px/s, um risco rápido de mouse.
pub const VELOCIDADE_FINA: f32 = 2.0;
/// Pressão sintética do traço mais rápido (o mais lento vale 1).
pub const PRESSAO_SINTETICA_MIN: f32 = 0.3;

/// Pressão sintética a partir da velocidade já suavizada:
/// devagar = 1 (traço cheio), [`VELOCIDADE_FINA`] ou mais = [`PRESSAO_SINTETICA_MIN`].
pub fn pressao_da_velocidade(velocidade: f32, velocidade_fina: f32) -> f32 {
    let vf = if velocidade_fina.is_finite() && velocidade_fina > 0.0 { velocidade_fina } else { VELOCIDADE_FINA };
    let v = if velocidade.is_finite() { velocidade.max(0.0) } else { 0.0 };
    1.0 - (v / vf).min(1.0) * (1.0 - PRESSAO_SINTETICA_MIN)
}

/// Pressão sintética de cada ponto pela velocidade do traço (rápido =
/// mais fino). Precisa do `tempo` dos pontos; `None` se faltar tempo ou
/// houver menos de dois pontos. A velocidade é suavizada (média móvel
/// exponencial) para o traço não tremer de largura a cada evento.
pub fn pressoes_por_velocidade(pontos: &[PontoTinta], velocidade_fina: f32) -> Option<Vec<f32>> {
    if pontos.len() < 2 || pontos.iter().any(|p| p.tempo.is_none()) {
        return None;
    }
    let mut saida = Vec::with_capacity(pontos.len());
    let mut suave: Option<f32> = None;
    for s in pontos.windows(2) {
        let dt = s[1].tempo.unwrap_or(0).saturating_sub(s[0].tempo.unwrap_or(0)).max(1) as f32;
        let v = distancia(s[0].ponto(), s[1].ponto()) / dt;
        let atual = match suave {
            None => v,
            Some(anterior) => anterior + 0.35 * (v - anterior),
        };
        suave = Some(atual);
        saida.push(pressao_da_velocidade(atual, velocidade_fina));
    }
    // O primeiro ponto herda a do primeiro trecho.
    saida.insert(0, saida[0]);
    Some(saida)
}

impl Traco {
    /// Algum ponto tem pressão (real ou sintética)?
    pub fn tem_pressao(&self) -> bool {
        self.pontos.iter().any(|p| p.pressao.is_some())
    }

    /// Desenhado pelo contorno de largura variável: caneta com pressão.
    /// O marca-texto continua de largura constante.
    pub fn largura_variavel(&self) -> bool {
        self.ferramenta == Ferramenta::Caneta && self.tem_pressao()
    }

    /// Sem mesa: põe pressão sintética pela velocidade e marca o traço
    /// ([`Traco::pressao_sintetica`]). Não faz nada se o traço já tiver
    /// pressão (real) ou se faltar o tempo dos pontos. Devolve se aplicou.
    pub fn aplicar_pressao_por_velocidade(&mut self, velocidade_fina: f32) -> bool {
        if self.tem_pressao() {
            return false;
        }
        let Some(pressoes) = pressoes_por_velocidade(&self.pontos, velocidade_fina) else { return false };
        for (p, pr) in self.pontos.iter_mut().zip(pressoes) {
            p.pressao = Some(pr);
        }
        self.pressao_sintetica = true;
        true
    }

    /// Polígonos fechados do contorno de largura variável (ver
    /// [`contorno_largura_variavel`]).
    pub fn contorno(&self, curva: CurvaPressao) -> Vec<Vec<Ponto>> {
        contorno_largura_variavel(&self.pontos, self.espessura, self.reto, curva)
    }

    /// Caminho preenchido (regra não-zero) do contorno.
    pub fn caminho_contorno(&self, curva: CurvaPressao) -> Caminho {
        caminho_de_poligonos(&self.contorno(curva))
    }

    /// Figura pronta: o contorno preenchido quando o traço tem largura
    /// variável; senão a mesma de [`Traco::figura`].
    pub fn figura_com_pressao(&self, curva: CurvaPressao) -> Figura {
        if !self.largura_variavel() {
            return self.figura();
        }
        Figura::Caminho {
            caminho: self.caminho_contorno(curva),
            estilo: EstiloVetor {
                preenchimento: Some(Preenchimento::Solido(self.cor)),
                stroke: None,
                opacidade: self.opacidade.clamp(0.0, 1.0),
                texto: None,
            },
        }
    }
}

/// Caminho com um subcaminho fechado por polígono (regra não-zero: os
/// pedaços se somam, nunca abrem buraco).
pub fn caminho_de_poligonos(poligonos: &[Vec<Ponto>]) -> Caminho {
    let mut caminho = Caminho::novo();
    for pol in poligonos.iter().filter(|p| p.len() >= 3) {
        caminho.adicionar(Comando::Mover(pol[0]));
        for &p in &pol[1..] {
            caminho.adicionar(Comando::Linha(p));
        }
        caminho.adicionar(Comando::Fechar);
    }
    caminho
}

/// Ângulo (radianos) a partir do qual o contorno é dividido em pedaços:
/// numa quina viva o deslocamento lateral se cruzaria.
const QUINA: f32 = 1.3;

/// Contorno de um traço de largura variável: polígonos fechados, todos com
/// a mesma orientação (anti-horária na conta matemática), para preencher
/// com a regra não-zero. Segue a mesma linha central de [`Traco::caminho`]
/// (curvas quadráticas pelos pontos médios, ou segmentos retos com `reto`),
/// com pontas redondas. A largura de cada ponto vem de `curva` e da pressão
/// (sem pressão = `espessura`). Um ponto só vira um disco.
pub fn contorno_largura_variavel(pontos: &[PontoTinta], espessura: f32, reto: bool, curva: CurvaPressao) -> Vec<Vec<Ponto>> {
    if !(espessura.is_finite() && espessura > 0.0) {
        return Vec::new();
    }
    let amostras = linha_central(pontos, espessura, reto, curva);
    let Some(&(c0, r0)) = amostras.first() else { return Vec::new() };
    if amostras.len() == 1 {
        return vec![disco(c0, r0)];
    }
    let mut saida = Vec::new();
    let mut inicio = 0;
    for i in 1..amostras.len() - 1 {
        let (a, b, c) = (amostras[i - 1].0, amostras[i].0, amostras[i + 1].0);
        if angulo_de_virada(a, b, c) > QUINA {
            saida.push(orientar(pedaco(&amostras[inicio..=i])));
            inicio = i;
        }
    }
    saida.push(orientar(pedaco(&amostras[inicio..])));
    saida
}

/// Linha central amostrada: (centro, raio). Quadráticas achatadas em
/// passos de ~1 unidade; a pressão anda pela mesma curva (Bézier escalar).
fn linha_central(pontos: &[PontoTinta], espessura: f32, reto: bool, curva: CurvaPressao) -> Vec<(Ponto, f32)> {
    let validos: Vec<PontoTinta> = pontos.iter().copied().filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
    let raio = |p: Option<f32>| (curva.largura(espessura, p) / 2.0).max(0.01);
    let mut saida: Vec<(Ponto, f32)> = Vec::with_capacity(validos.len() * 4);
    let empurrar = |p: Ponto, r: f32, saida: &mut Vec<(Ponto, f32)>| {
        if let Some(&(ultimo, r_ultimo)) = saida.last() {
            if distancia(ultimo, p) < 1e-4 {
                // Mesmo lugar: fica o maior raio.
                if r > r_ultimo {
                    saida.last_mut().expect("existe").1 = r;
                }
                return;
            }
        }
        saida.push((p, r));
    };
    match validos.as_slice() {
        [] => {}
        [unico] => empurrar(unico.ponto(), raio(unico.pressao), &mut saida),
        pts if reto || pts.len() == 2 => {
            for p in pts {
                empurrar(p.ponto(), raio(p.pressao), &mut saida);
            }
        }
        pts => {
            let pr = |p: &PontoTinta| p.pressao;
            let media = |a: Option<f32>, b: Option<f32>| match (a, b) {
                (Some(a), Some(b)) => Some((a + b) / 2.0),
                (a, None) | (None, a) => a,
            };
            let mut inicio = (pts[0].ponto(), pr(&pts[0]));
            empurrar(inicio.0, raio(inicio.1), &mut saida);
            for i in 1..pts.len() - 1 {
                let controle = (pts[i].ponto(), pr(&pts[i]));
                let fim = (
                    Ponto::novo((pts[i].x + pts[i + 1].x) / 2.0, (pts[i].y + pts[i + 1].y) / 2.0),
                    media(pr(&pts[i]), pr(&pts[i + 1])),
                );
                let comprimento = distancia(inicio.0, controle.0) + distancia(controle.0, fim.0);
                let passos = (comprimento.ceil() as usize).clamp(1, 32);
                for k in 1..=passos {
                    let t = k as f32 / passos as f32;
                    let u = 1.0 - t;
                    let (a, b, c) = (u * u, 2.0 * u * t, t * t);
                    let p = Ponto::novo(
                        a * inicio.0.x + b * controle.0.x + c * fim.0.x,
                        a * inicio.0.y + b * controle.0.y + c * fim.0.y,
                    );
                    let pressao = match (inicio.1, controle.1, fim.1) {
                        (Some(p0), Some(p1), Some(p2)) => Some(a * p0 + b * p1 + c * p2),
                        _ => media(media(inicio.1, controle.1), fim.1),
                    };
                    empurrar(p, raio(pressao), &mut saida);
                }
                inicio = fim;
            }
            let ultimo = &pts[pts.len() - 1];
            empurrar(ultimo.ponto(), raio(ultimo.pressao), &mut saida);
        }
    }
    saida
}

fn normalizar(x: f32, y: f32) -> (f32, f32) {
    let l = x.hypot(y);
    if l > f32::EPSILON { (x / l, y / l) } else { (0.0, 0.0) }
}

fn angulo_de_virada(a: Ponto, b: Ponto, c: Ponto) -> f32 {
    let (ux, uy) = normalizar(b.x - a.x, b.y - a.y);
    let (vx, vy) = normalizar(c.x - b.x, c.y - b.y);
    (ux * vx + uy * vy).clamp(-1.0, 1.0).acos()
}

/// Segmentos do arco de raio `r` (mais pontos em traço grosso).
fn segmentos_arco(r: f32, meia_volta: bool) -> usize {
    let n = ((r * 2.0).sqrt() * 4.0).ceil() as usize;
    let n = n.clamp(6, 32);
    if meia_volta { n.div_ceil(2).max(4) } else { n }
}

fn disco(c: Ponto, r: f32) -> Vec<Ponto> {
    let n = segmentos_arco(r, false).max(12);
    (0..n)
        .map(|k| {
            let a = std::f32::consts::TAU * k as f32 / n as f32;
            Ponto::novo(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// Meia volta em torno de `c`, do ângulo `de` diminuindo π (sem o ponto inicial).
fn meia_volta(saida: &mut Vec<Ponto>, c: Ponto, r: f32, de: f32) {
    let n = segmentos_arco(r, true);
    for k in 1..n {
        let a = de - std::f32::consts::PI * k as f32 / n as f32;
        saida.push(Ponto::novo(c.x + r * a.cos(), c.y + r * a.sin()));
    }
}

/// Um pedaço sem quina viva: lado esquerdo na ida, ponta redonda, lado
/// direito na volta, ponta redonda.
fn pedaco(amostras: &[(Ponto, f32)]) -> Vec<Ponto> {
    let n = amostras.len();
    if n == 1 {
        return disco(amostras[0].0, amostras[0].1);
    }
    let dir = |i: usize, j: usize| normalizar(amostras[j].0.x - amostras[i].0.x, amostras[j].0.y - amostras[i].0.y);
    // Normal à esquerda de cada amostra (média das direções vizinhas),
    // alongada como uma junção em esquadria limitada a 2×.
    let normais: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let (tx, ty) = if i == 0 {
                dir(0, 1)
            } else if i == n - 1 {
                dir(n - 2, n - 1)
            } else {
                let (ax, ay) = dir(i - 1, i);
                let (bx, by) = dir(i, i + 1);
                let (mx, my) = normalizar(ax + bx, ay + by);
                if mx == 0.0 && my == 0.0 { (ax, ay) } else { (mx, my) }
            };
            let (nx, ny) = (-ty, tx);
            let escala = if i == 0 || i == n - 1 {
                1.0
            } else {
                let (ax, ay) = dir(i - 1, i);
                let cos = (nx * -ay + ny * ax).abs();
                (1.0 / cos.max(0.5)).min(2.0)
            };
            (nx * escala, ny * escala)
        })
        .collect();
    let lado = |i: usize, s: f32| {
        let (c, r) = amostras[i];
        Ponto::novo(c.x + s * normais[i].0 * r, c.y + s * normais[i].1 * r)
    };
    let mut pol = Vec::with_capacity(n * 2 + 32);
    for i in 0..n {
        pol.push(lado(i, 1.0));
    }
    let (cf, rf) = amostras[n - 1];
    let ang_fim = normais[n - 1].1.atan2(normais[n - 1].0);
    meia_volta(&mut pol, cf, rf, ang_fim);
    for i in (0..n).rev() {
        pol.push(lado(i, -1.0));
    }
    let (ci, ri) = amostras[0];
    let ang_ini = (-normais[0].1).atan2(-normais[0].0);
    meia_volta(&mut pol, ci, ri, ang_ini);
    pol
}

/// Área com sinal (positiva = anti-horária na conta com y para cima).
pub(crate) fn area_com_sinal(pol: &[Ponto]) -> f32 {
    let n = pol.len();
    (0..n).map(|i| {
        let (a, b) = (pol[i], pol[(i + 1) % n]);
        a.x * b.y - b.x * a.y
    }).sum::<f32>() / 2.0
}

fn orientar(mut pol: Vec<Ponto>) -> Vec<Ponto> {
    if area_com_sinal(&pol) < 0.0 {
        pol.reverse();
    }
    pol
}

/// Douglas-Peucker com a pressão como terceira coordenada (`pressão ×
/// escala`): um trecho reto em que a força muda continua com pontos
/// suficientes para a largura variar. Pontas sempre ficam.
pub fn simplificar_com_pressao(pontos: &[PontoTinta], tolerancia: f32, escala: f32) -> Vec<PontoTinta> {
    let n = pontos.len();
    if n <= 2 {
        return pontos.to_vec();
    }
    let tol = if tolerancia.is_finite() && tolerancia > 0.0 { tolerancia } else { 0.0 };
    let z = |p: &PontoTinta| p.pressao.unwrap_or(1.0) * escala;
    let dist = |k: usize, i: usize, j: usize| {
        let (p, a, b) = (&pontos[k], &pontos[i], &pontos[j]);
        let (ax, ay, az) = (a.x, a.y, z(a));
        let (dx, dy, dz) = (b.x - ax, b.y - ay, z(b) - az);
        let l2 = dx * dx + dy * dy + dz * dz;
        let t = if l2 <= f32::EPSILON { 0.0 } else { (((p.x - ax) * dx + (p.y - ay) * dy + (z(p) - az) * dz) / l2).clamp(0.0, 1.0) };
        let (ex, ey, ez) = (p.x - (ax + t * dx), p.y - (ay + t * dy), z(p) - (az + t * dz));
        (ex * ex + ey * ey + ez * ez).sqrt()
    };
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
            let d = dist(k, i, j);
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
    (0..n).filter(|&k| manter[k]).map(|k| pontos[k]).collect()
}

//! "Tinta para forma": reconhece no traço à mão uma linha, seta,
//! retângulo, triângulo, elipse ou círculo e devolve a forma ideal.
//! Tolerâncias relativas ao tamanho do traço (unidade neutra).
use super::{distancia, distancia_segmento, limites_de, simplificar_indices};
use crate::{Caminho, Comando, Ponto};
use std::f32::consts::{PI, TAU};

/// Forma reconhecida, em coordenadas do traço.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Forma {
    Linha { de: Ponto, ate: Ponto },
    /// Seta de `de` até a ponta `ate`; `ponta` é o comprimento das abas.
    Seta { de: Ponto, ate: Ponto, ponta: f32 },
    /// Cantos em ordem (pode estar girado).
    Retangulo { cantos: [Ponto; 4] },
    Triangulo { vertices: [Ponto; 3] },
    /// `rotacao` em radianos, do eixo X para o eixo de `raio_x`.
    Elipse { centro: Ponto, raio_x: f32, raio_y: f32, rotacao: f32 },
    Circulo { centro: Ponto, raio: f32 },
}

/// Pontos usados para desenhar a elipse/círculo como traço.
const AMOSTRAS_ELIPSE: usize = 72;

impl Forma {
    pub fn nome(&self) -> &'static str {
        match self {
            Self::Linha { .. } => "linha",
            Self::Seta { .. } => "seta",
            Self::Retangulo { .. } => "retângulo",
            Self::Triangulo { .. } => "triângulo",
            Self::Elipse { .. } => "elipse",
            Self::Circulo { .. } => "círculo",
        }
    }

    /// Segmentos retos (tudo menos elipse e círculo).
    pub fn poligonal(&self) -> bool {
        !matches!(self, Self::Elipse { .. } | Self::Circulo { .. })
    }

    /// Polilinha que desenha a forma; as fechadas repetem o primeiro ponto.
    pub fn pontos(&self) -> Vec<Ponto> {
        match *self {
            Self::Linha { de, ate } => vec![de, ate],
            Self::Seta { de, ate, ponta } => {
                let ang = (de.y - ate.y).atan2(de.x - ate.x);
                let aba = |d: f32| {
                    Ponto::novo(ate.x + ponta * (ang + d).cos(), ate.y + ponta * (ang + d).sin())
                };
                let abertura = 28f32.to_radians();
                vec![de, ate, aba(abertura), ate, aba(-abertura)]
            }
            Self::Retangulo { cantos } => {
                let mut v = cantos.to_vec();
                v.push(cantos[0]);
                v
            }
            Self::Triangulo { vertices } => {
                let mut v = vertices.to_vec();
                v.push(vertices[0]);
                v
            }
            Self::Elipse { centro, raio_x, raio_y, rotacao } => {
                pontos_elipse(centro, raio_x, raio_y, rotacao)
            }
            Self::Circulo { centro, raio } => pontos_elipse(centro, raio, raio, 0.0),
        }
    }

    /// Caminho da forma (contorno).
    pub fn caminho(&self) -> Caminho {
        let mut c = Caminho::novo();
        let pts = self.pontos();
        if let Some(&p0) = pts.first() {
            c.adicionar(Comando::Mover(p0));
            if let Self::Seta { .. } = self {
                // Haste e abas: dois subcaminhos, sem passar duas vezes na ponta.
                c.adicionar(Comando::Linha(pts[1]));
                c.adicionar(Comando::Mover(pts[2]));
                c.adicionar(Comando::Linha(pts[3]));
                c.adicionar(Comando::Linha(pts[4]));
                return c;
            }
            for &p in &pts[1..] {
                c.adicionar(Comando::Linha(p));
            }
            if !matches!(self, Self::Linha { .. }) {
                c.adicionar(Comando::Fechar);
            }
        }
        c
    }
}

fn pontos_elipse(c: Ponto, rx: f32, ry: f32, rot: f32) -> Vec<Ponto> {
    let (s, co) = rot.sin_cos();
    (0..=AMOSTRAS_ELIPSE)
        .map(|i| {
            let t = TAU * i as f32 / AMOSTRAS_ELIPSE as f32;
            let (u, v) = (rx * t.cos(), ry * t.sin());
            Ponto::novo(c.x + u * co - v * s, c.y + u * s + v * co)
        })
        .collect()
}

/// Reconhece a forma de um traço. `None` quando nenhuma se encaixa.
pub fn reconhecer_forma(pontos: &[Ponto]) -> Option<Forma> {
    let mut pts: Vec<Ponto> = Vec::with_capacity(pontos.len());
    for &p in pontos {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return None;
        }
        if pts.last().map_or(true, |&q| distancia(p, q) > 0.0) {
            pts.push(p);
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let caixa = limites_de(pts.iter().copied())?;
    let diag = caixa.largura.hypot(caixa.altura);
    if diag <= 1e-4 {
        return None;
    }
    let comprimento: f32 = pts.windows(2).map(|s| distancia(s[0], s[1])).sum();
    // Quem fecha a forma à mão costuma passar do ponto inicial: corta no
    // ponto da segunda metade mais perto do início.
    let metade = pts.len() / 2;
    let volta = (metade..pts.len())
        .min_by(|&i, &j| distancia(pts[i], pts[0]).total_cmp(&distancia(pts[j], pts[0])))
        .unwrap_or(pts.len() - 1);
    let fechado = distancia(pts[0], pts[volta]) <= 0.25 * diag && comprimento >= 1.8 * diag;
    if !fechado {
        return linha(&pts, comprimento).or_else(|| seta(&pts, diag));
    }
    pts.truncate(volta + 1);
    if let Some(f) = elipse(&pts) {
        return Some(f);
    }
    let cantos = cantos_fechados(&pts, diag);
    match cantos.len() {
        3 => Some(Forma::Triangulo { vertices: [cantos[0], cantos[1], cantos[2]] }),
        4 => retangulo(&cantos),
        _ => None,
    }
}

fn linha(pts: &[Ponto], comprimento: f32) -> Option<Forma> {
    let (de, ate) = (pts[0], pts[pts.len() - 1]);
    let corda = distancia(de, ate);
    if corda <= 0.0 || comprimento > 1.15 * corda {
        return None;
    }
    let desvio = pts.iter().map(|&p| distancia_segmento(p, de, ate)).fold(0.0, f32::max);
    (desvio <= 0.08 * corda).then_some(Forma::Linha { de, ate })
}

/// Seta num traço só: haste até a ponta e depois as abas (ex.: haste,
/// aba 1, volta à ponta, aba 2). As abas apontam para trás, uma de cada lado.
fn seta(pts: &[Ponto], diag: f32) -> Option<Forma> {
    let idx = simplificar_indices(pts, 0.06 * diag);
    let v: Vec<Ponto> = idx.iter().map(|&i| pts[i]).collect();
    if v.len() < 4 {
        return None;
    }
    let (de, ponta) = (v[0], v[1]);
    let haste = distancia(de, ponta);
    // A haste é o trecho mais longo e precisa ser reta.
    if v.windows(2).skip(1).any(|s| distancia(s[0], s[1]) > haste) {
        return None;
    }
    if pts[..=idx[1]].iter().any(|&p| distancia_segmento(p, de, ponta) > 0.08 * haste) {
        return None;
    }
    let (hx, hy) = ((de.x - ponta.x) / haste, (de.y - ponta.y) / haste);
    // Tamanho da cabeça: a aba mais longe da ponta. Pontos bem mais perto
    // que isso são a volta à ponta entre uma aba e outra.
    let cabeca = v[2..].iter().map(|&w| distancia(w, ponta)).fold(0.0f32, f32::max);
    if cabeca < 0.05 * haste || cabeca > 0.5 * haste {
        return None;
    }
    let mut lados = [false; 2];
    let mut abas = Vec::new();
    for &w in &v[2..] {
        let d = distancia(w, ponta);
        if d < 0.4 * cabeca {
            continue; // de volta à ponta
        }
        let (wx, wy) = ((w.x - ponta.x) / d, (w.y - ponta.y) / d);
        let cos = wx * hx + wy * hy;
        if cos < (70f32).to_radians().cos() {
            return None; // aba não aponta para trás
        }
        let lado = hx * wy - hy * wx;
        lados[usize::from(lado > 0.0)] = true;
        abas.push(d);
    }
    if !(lados[0] && lados[1]) {
        return None;
    }
    let ponta_media = abas.iter().sum::<f32>() / abas.len() as f32;
    Some(Forma::Seta { de, ate: ponta, ponta: ponta_media })
}

/// Reamostra o contorno fechado em `n` pontos igualmente espaçados.
fn reamostrar_fechado(pts: &[Ponto], n: usize) -> Vec<Ponto> {
    let mut anel = pts.to_vec();
    anel.push(pts[0]);
    let total: f32 = anel.windows(2).map(|s| distancia(s[0], s[1])).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let passo = total / n as f32;
    let mut saida = Vec::with_capacity(n);
    let (mut seg, mut acumulado) = (0usize, 0.0f32);
    for k in 0..n {
        let alvo = k as f32 * passo;
        while seg < anel.len() - 2 && acumulado + distancia(anel[seg], anel[seg + 1]) < alvo {
            acumulado += distancia(anel[seg], anel[seg + 1]);
            seg += 1;
        }
        let (a, b) = (anel[seg], anel[seg + 1]);
        let l = distancia(a, b);
        let t = if l > 0.0 { ((alvo - acumulado) / l).clamp(0.0, 1.0) } else { 0.0 };
        saida.push(Ponto::novo(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
    }
    saida
}

/// Elipse: centro e eixos pela covariância; raios por mínimos quadrados
/// (`A·u² + B·v² = 1`). Aceita só se o contorno cola na elipse.
fn elipse(pts: &[Ponto]) -> Option<Forma> {
    let amostras = reamostrar_fechado(pts, 96);
    if amostras.len() < 8 {
        return None;
    }
    let n = amostras.len() as f32;
    let cx = amostras.iter().map(|p| p.x).sum::<f32>() / n;
    let cy = amostras.iter().map(|p| p.y).sum::<f32>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
    for p in &amostras {
        let (dx, dy) = (p.x - cx, p.y - cy);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let mut rot = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (s, c) = rot.sin_cos();
    let uv: Vec<(f32, f32)> = amostras
        .iter()
        .map(|p| {
            let (dx, dy) = (p.x - cx, p.y - cy);
            (dx * c + dy * s, -dx * s + dy * c)
        })
        .collect();
    // Normais de min Σ(A u² + B v² − 1)².
    let (mut s_uu, mut s_vv, mut s_uv, mut s_u, mut s_v) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for &(u, v) in &uv {
        let (u2, v2) = (f64::from(u * u), f64::from(v * v));
        s_uu += u2 * u2;
        s_vv += v2 * v2;
        s_uv += u2 * v2;
        s_u += u2;
        s_v += v2;
    }
    let det = s_uu * s_vv - s_uv * s_uv;
    if det.abs() <= f64::EPSILON {
        return None;
    }
    let a = (s_u * s_vv - s_v * s_uv) / det;
    let b = (s_v * s_uu - s_u * s_uv) / det;
    if !(a > 0.0 && b > 0.0) {
        return None;
    }
    let (mut rx, mut ry) = ((1.0 / a).sqrt() as f32, (1.0 / b).sqrt() as f32);
    let erros: Vec<f32> = uv
        .iter()
        .map(|&(u, v)| ((f64::from(u * u) * a + f64::from(v * v) * b).sqrt() as f32 - 1.0).abs())
        .collect();
    let medio = erros.iter().sum::<f32>() / erros.len() as f32;
    let maximo = erros.iter().fold(0.0f32, |m, &e| m.max(e));
    if medio > 0.07 || maximo > 0.16 || rx.min(ry) < 0.15 * rx.max(ry) {
        return None;
    }
    let centro = Ponto::novo(cx, cy);
    if rx.min(ry) >= 0.85 * rx.max(ry) {
        return Some(Forma::Circulo { centro, raio: (rx + ry) / 2.0 });
    }
    // Eixo maior em X; quase alinhado aos eixos vira alinhado.
    if ry > rx {
        std::mem::swap(&mut rx, &mut ry);
        rot += PI / 2.0;
    }
    rot = normalizar_meia_volta(rot);
    if rot.abs() < 8f32.to_radians() {
        rot = 0.0;
    }
    Some(Forma::Elipse { centro, raio_x: rx, raio_y: ry, rotacao: rot })
}

/// Ângulo em `(-π/2, π/2]` (eixo de elipse não tem sentido).
fn normalizar_meia_volta(mut a: f32) -> f32 {
    while a > PI / 2.0 {
        a -= PI;
    }
    while a <= -PI / 2.0 {
        a += PI;
    }
    a
}

/// Cantos de um contorno fechado: Douglas-Peucker nas duas metades
/// (início → ponto mais distante → início), depois tira vértices quase
/// retos e vértices colados.
fn cantos_fechados(pts: &[Ponto], diag: f32) -> Vec<Ponto> {
    let mut anel = pts.to_vec();
    anel.push(pts[0]);
    let longe = (0..anel.len())
        .max_by(|&i, &j| distancia(anel[i], anel[0]).total_cmp(&distancia(anel[j], anel[0])))
        .unwrap_or(0);
    let tol = 0.07 * diag;
    let mut v: Vec<Ponto> = simplificar_indices(&anel[..=longe], tol)
        .into_iter()
        .map(|i| anel[i])
        .collect();
    v.pop();
    v.extend(simplificar_indices(&anel[longe..], tol).into_iter().map(|i| anel[longe + i]));
    v.pop(); // fecha no início
    loop {
        let n = v.len();
        if n < 3 {
            return v;
        }
        let mut remover = None;
        for i in 0..n {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            if distancia(a, b) < 0.1 * diag || giro(a, b, c) < 35f32.to_radians() {
                remover = Some(i);
                break;
            }
        }
        match remover {
            Some(i) => {
                v.remove(i);
            }
            None => return v,
        }
    }
}

/// Quanto a direção muda em b (0 = segue reto, π = volta).
fn giro(a: Ponto, b: Ponto, c: Ponto) -> f32 {
    let d1 = (b.y - a.y).atan2(b.x - a.x);
    let d2 = (c.y - b.y).atan2(c.x - b.x);
    let mut d = (d2 - d1).abs() % TAU;
    if d > PI {
        d = TAU - d;
    }
    d
}

fn retangulo(c: &[Ponto]) -> Option<Forma> {
    // Ângulos internos perto de 90°.
    for i in 0..4 {
        let g = giro(c[(i + 3) % 4], c[i], c[(i + 1) % 4]);
        if (g - PI / 2.0).abs() > 25f32.to_radians() {
            return None;
        }
    }
    // Orientação média dos lados (ângulo × 4 junta lados perpendiculares).
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    for i in 0..4 {
        let (a, b) = (c[i], c[(i + 1) % 4]);
        let ang = (b.y - a.y).atan2(b.x - a.x);
        let l = distancia(a, b);
        sx += l * (4.0 * ang).cos();
        sy += l * (4.0 * ang).sin();
    }
    let mut theta = sy.atan2(sx) / 4.0;
    if theta.abs() < 8f32.to_radians() {
        theta = 0.0;
    }
    let centro = Ponto::novo(
        c.iter().map(|p| p.x).sum::<f32>() / 4.0,
        c.iter().map(|p| p.y).sum::<f32>() / 4.0,
    );
    let (s, co) = theta.sin_cos();
    let local: Vec<(f32, f32)> = c
        .iter()
        .map(|p| {
            let (dx, dy) = (p.x - centro.x, p.y - centro.y);
            (dx * co + dy * s, -dx * s + dy * co)
        })
        .collect();
    let mut xs: Vec<f32> = local.iter().map(|p| p.0).collect();
    let mut ys: Vec<f32> = local.iter().map(|p| p.1).collect();
    xs.sort_by(f32::total_cmp);
    ys.sort_by(f32::total_cmp);
    let (x0, x1) = ((xs[0] + xs[1]) / 2.0, (xs[2] + xs[3]) / 2.0);
    let (y0, y1) = ((ys[0] + ys[1]) / 2.0, (ys[2] + ys[3]) / 2.0);
    if x1 - x0 <= 0.0 || y1 - y0 <= 0.0 {
        return None;
    }
    let volta = |u: f32, v: f32| Ponto::novo(centro.x + u * co - v * s, centro.y + u * s + v * co);
    Some(Forma::Retangulo { cantos: [volta(x0, y0), volta(x1, y0), volta(x1, y1), volta(x0, y1)] })
}

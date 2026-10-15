//! Geometria de formas e cenas. Todo hit-test, snap, bounds e alinhamento
//! funciona em coordenadas de documento, SEM Iced e SEM acessar arquivos.
use crate::caminhos::{arco_eliptico, Caminho, Comando, RegraPreenchimento, Subcaminho};
use crate::estilos::{EstiloVetor, Stroke};
use crate::{transformar, Cena, Figura, Ponto, Retangulo, Transformacao};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroGeometria {
    IndiceInvalido,
    SelecaoInsuficiente,
    CoordenadaInvalida,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alinhar {
    Esquerda,
    CentroHorizontal,
    Direita,
    Topo,
    CentroVertical,
    Base,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eixo {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PontoSnap {
    pub indice: usize,
    pub ponto: Ponto,
    pub distancia: f32,
}

fn caminho_poligono(pontos: &[Ponto], fechado: bool) -> Caminho {
    let mut c = Caminho::novo();
    if let Some(&p) = pontos.first() {
        c.adicionar(Comando::Mover(p));
        for &p in &pontos[1..] {
            c.adicionar(Comando::Linha(p));
        }
        if fechado {
            c.adicionar(Comando::Fechar);
        }
    }
    c
}
fn caminho_retangulo(r: Retangulo) -> Caminho {
    caminho_poligono(
        &[
            Ponto::novo(r.x, r.y),
            Ponto::novo(r.x + r.largura, r.y),
            Ponto::novo(r.x + r.largura, r.y + r.altura),
            Ponto::novo(r.x, r.y + r.altura),
        ],
        true,
    )
}
fn caminho_seta(de: Ponto, ate: Ponto, ponta: f32, preenchida: bool) -> Caminho {
    let (dx, dy) = (ate.x - de.x, ate.y - de.y);
    let d = dx.hypot(dy);
    if !ponta.is_finite() || ponta < 0. || !d.is_finite() || d < 1e-6 {
        return Caminho::novo();
    }
    let (ux, uy) = (dx / d, dy / d);
    let tam = ponta.min(d * 0.8);
    let base = Ponto::novo(ate.x - ux * tam, ate.y - uy * tam);
    let lateral = |p: Ponto, distancia: f32| {
        Ponto::novo(p.x - uy * distancia, p.y + ux * distancia)
    };
    let abertura = tam * 0.45;

    if preenchida && tam > 0. {
        // Uma seta preenchida precisa de corpo fechado. Antes apenas a
        // cabeça (triângulo) tinha área, deixando invisível a haste quando
        // o estilo não possuía stroke (inclusive EstiloVetor::default).
        let meia_haste = tam * 0.10;
        return caminho_poligono(
            &[
                lateral(de, meia_haste),
                lateral(base, meia_haste),
                lateral(base, abertura),
                ate,
                lateral(base, -abertura),
                lateral(base, -meia_haste),
                lateral(de, -meia_haste),
            ],
            true,
        );
    }

    // Seta somente com stroke: haste e ponta aberta, sem o traço extra
    // entre os extremos da cabeça; não cria preenchimento implícito.
    let mut c = caminho_poligono(&[de, ate], false);
    c.adicionar(Comando::Mover(lateral(base, abertura)));
    c.adicionar(Comando::Linha(ate));
    c.adicionar(Comando::Linha(lateral(base, -abertura)));
    c
}
impl Figura {
    /// Estilo efetivo das primitivas novas; as formas antigas continuam com
    /// os mesmos campos para não quebrar Charts nem seus arquivos.
    pub fn estilo_vetor(&self) -> Option<EstiloVetor> {
        match self {
            Self::Retangulo { estilo, .. } | Self::Polilinha { estilo, .. } => {
                Some(EstiloVetor::from(estilo))
            }
            Self::Elipse { estilo, .. }
            | Self::Arco { estilo, .. }
            | Self::Bezier { estilo, .. }
            | Self::Seta { estilo, .. }
            | Self::Caminho { estilo, .. } => Some(estilo.clone()),
            Self::Recorte { figura, .. } => figura.estilo_vetor(),
        }
    }
    pub fn caminho(&self, tolerancia: f32) -> Caminho {
        match self {
            Self::Retangulo { area, .. } => caminho_retangulo(*area),
            Self::Polilinha {
                pontos, fechada, ..
            } => caminho_poligono(pontos, *fechada),
            Self::Elipse {
                centro,
                raio_x,
                raio_y,
                ..
            } => {
                let mut c = arco_eliptico(
                    *centro,
                    *raio_x,
                    *raio_y,
                    0.,
                    std::f32::consts::TAU,
                    tolerancia,
                );
                c.adicionar(Comando::Fechar);
                c
            }
            Self::Arco {
                centro,
                raio_x,
                raio_y,
                inicio,
                varredura,
                ..
            } => arco_eliptico(*centro, *raio_x, *raio_y, *inicio, *varredura, tolerancia),
            Self::Bezier {
                inicio,
                controle1,
                controle2,
                fim,
                ..
            } => Caminho {
                comandos: vec![
                    Comando::Mover(*inicio),
                    Comando::Cubica {
                        controle1: *controle1,
                        controle2: *controle2,
                        fim: *fim,
                    },
                ],
                regra: RegraPreenchimento::NaoZero,
            },
            Self::Seta {
                de,
                ate,
                tamanho_ponta,
                estilo,
            } => caminho_seta(
                *de,
                *ate,
                *tamanho_ponta,
                estilo.preenchimento.is_some(),
            ),
            Self::Caminho { caminho, .. } => caminho.clone(),
            Self::Recorte { figura, .. } => figura.caminho(tolerancia),
        }
    }
    pub fn bounding_box(&self) -> Option<Retangulo> {
        if let Self::Recorte { figura, recorte } = self {
            let a = figura.bounding_box()?;
            let b = limites_subcaminhos(&recorte.aproximar(0.05))?;
            return intersecao(a, b);
        }
        let caminho = self.caminho(0.05);
        let mut caixa = limites_subcaminhos(&caminho.aproximar(0.05))?;
        let espessura = self
            .estilo_vetor()
            .and_then(|e| e.stroke.map(|s| s.largura))
            .unwrap_or(0.)
            .max(0.)
            / 2.;
        if espessura.is_finite() {
            caixa.x -= espessura;
            caixa.y -= espessura;
            caixa.largura += 2. * espessura;
            caixa.altura += 2. * espessura;
        }
        Some(caixa)
    }
    /// Testa pintura: fill winding ou par/ímpar; stroke inclui fase/dash.
    /// Recortes restringem o resultado ao preenchimento do caminho de clip.
    pub fn atinge(&self, p: Ponto, tolerancia: f32) -> bool {
        if !p.x.is_finite() || !p.y.is_finite() {
            return false;
        }
        if let Self::Recorte { figura, recorte } = self {
            return contem_preenchimento(&recorte.aproximar(0.15), recorte.regra, p)
                && figura.atinge(p, tolerancia);
        }
        let Some(e) = self.estilo_vetor() else {
            return false;
        };
        // Estilos inválidos ou completamente transparentes não pintam nada.
        if !e.valido() || e.opacidade <= 0. {
            return false;
        }
        let c = self.caminho(0.15);
        let sub = c.aproximar(0.15);
        if e.preenchimento
            .as_ref()
            .is_some_and(|fill| e.alfa(fill.amostrar(p)) > 0)
            && contem_preenchimento(&sub, c.regra, p)
        {
            return true;
        }
        if let Some(s) = e.stroke.as_ref() {
            if e.alfa(s.cor) > 0 && s.largura > 0.0 {
                return atinge_stroke(&sub, p, s, tolerancia.max(0.));
            }
        }
        false
    }
}
fn limites_subcaminhos(sub: &[Subcaminho]) -> Option<Retangulo> {
    let mut minx = f32::INFINITY;
    let mut miny = f32::INFINITY;
    let mut maxx = f32::NEG_INFINITY;
    let mut maxy = f32::NEG_INFINITY;
    for p in sub.iter().flat_map(|c| c.pontos.iter()) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        minx = minx.min(p.x);
        miny = miny.min(p.y);
        maxx = maxx.max(p.x);
        maxy = maxy.max(p.y);
    }
    if !minx.is_finite() {
        None
    } else {
        Some(Retangulo::novo(minx, miny, maxx - minx, maxy - miny))
    }
}
fn intersecao(a: Retangulo, b: Retangulo) -> Option<Retangulo> {
    let (x, y) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = (
        (a.x + a.largura).min(b.x + b.largura),
        (a.y + a.altura).min(b.y + b.altura),
    );
    (x1 >= x && y1 >= y).then(|| Retangulo::novo(x, y, x1 - x, y1 - y))
}
fn contem_preenchimento(sub: &[Subcaminho], regra: RegraPreenchimento, p: Ponto) -> bool {
    let mut winding = 0i32;
    // Caminhos abertos são fechados implicitamente para fill/clip, mas
    // continuam abertos para stroke (sem segmento de contorno artificial).
    // Isso corresponde à semântica de preenchimento SVG.
    for c in sub.iter().filter(|s| s.pontos.len() > 2) {
        for i in 0..c.pontos.len() {
            let a = c.pontos[i];
            let b = c.pontos[(i + 1) % c.pontos.len()];
            if a.y <= p.y && b.y > p.y || a.y > p.y && b.y <= p.y {
                let xx = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if xx > p.x {
                    if b.y > a.y {
                        winding += 1;
                    } else {
                        winding -= 1;
                    }
                }
            }
        }
    }
    match regra {
        RegraPreenchimento::ParImpar => winding.abs() % 2 == 1,
        RegraPreenchimento::NaoZero => winding != 0,
    }
}
fn atinge_stroke(sub: &[Subcaminho], p: Ponto, s: &Stroke, tol: f32) -> bool {
    if !s.valido() {
        return false;
    }
    let limite = s.largura / 2. + tol;
    for c in sub {
        let n = c.pontos.len();
        if n < 2 {
            continue;
        }
        let mut percorrido = 0.;
        let qtd = if c.fechado { n } else { n - 1 };
        for i in 0..qtd {
            let a = c.pontos[i];
            let b = c.pontos[(i + 1) % n];
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let comprimento = dx.hypot(dy);
            if comprimento > 0. {
                let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (comprimento * comprimento))
                    .clamp(0., 1.);
                let dist = (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy);
                if dist <= limite && s.visivel(percorrido + t * comprimento) {
                    return true;
                }
                percorrido += comprimento;
            }
        }
    }
    false
}

impl Cena {
    /// Renderizadores desenham na ordem das figuras; a última está na frente.
    pub fn trazer_frente(&mut self, indice: usize) -> Result<(), ErroGeometria> {
        if indice >= self.figuras.len() {
            return Err(ErroGeometria::IndiceInvalido);
        }
        let f = self.figuras.remove(indice);
        self.figuras.push(f);
        Ok(())
    }
    pub fn enviar_fundo(&mut self, indice: usize) -> Result<(), ErroGeometria> {
        if indice >= self.figuras.len() {
            return Err(ErroGeometria::IndiceInvalido);
        }
        let f = self.figuras.remove(indice);
        self.figuras.insert(0, f);
        Ok(())
    }
    pub fn ordem(&mut self, de: usize, para: usize) -> Result<(), ErroGeometria> {
        if de >= self.figuras.len() || para >= self.figuras.len() {
            return Err(ErroGeometria::IndiceInvalido);
        }
        let f = self.figuras.remove(de);
        self.figuras.insert(para, f);
        Ok(())
    }
    pub fn hit_test(&self, p: Ponto, tolerancia: f32) -> Option<usize> {
        self.figuras.iter().rposition(|f| f.atinge(p, tolerancia))
    }
    pub fn bounding_box(&self) -> Option<Retangulo> {
        let caixas = self
            .figuras
            .iter()
            .filter_map(Figura::bounding_box)
            .collect::<Vec<_>>();
        agrupar_caixas(&caixas)
    }
    /// Seleciona a âncora geométrica mais próxima dentro do raio pedido.
    pub fn snap(&self, p: Ponto, tolerancia: f32) -> Option<PontoSnap> {
        if !tolerancia.is_finite() || tolerancia < 0. {
            return None;
        }
        let mut melhor: Option<PontoSnap> = None;
        for (i, f) in self.figuras.iter().enumerate().rev() {
            let Some(b) = f.bounding_box() else { continue };
            let meio = Ponto::novo(b.x + b.largura / 2., b.y + b.altura / 2.);
            let candidatos = [
                meio,
                Ponto::novo(b.x, b.y),
                Ponto::novo(b.x + b.largura, b.y),
                Ponto::novo(b.x, b.y + b.altura),
                Ponto::novo(b.x + b.largura, b.y + b.altura),
            ];
            for q in candidatos {
                let d = (p.x - q.x).hypot(p.y - q.y);
                if d <= tolerancia && melhor.map_or(true, |at| d < at.distancia) {
                    melhor = Some(PontoSnap {
                        indice: i,
                        ponto: q,
                        distancia: d,
                    });
                }
            }
            for sub in f.caminho(0.25).aproximar(0.25) {
                for q in sub
                    .pontos
                    .iter()
                    .take(1)
                    .chain(sub.pontos.iter().rev().take(1))
                {
                    let d = (p.x - q.x).hypot(p.y - q.y);
                    if d <= tolerancia && melhor.map_or(true, |at| d < at.distancia) {
                        melhor = Some(PontoSnap {
                            indice: i,
                            ponto: *q,
                            distancia: d,
                        });
                    }
                }
            }
        }
        melhor
    }
    pub fn alinhar(&mut self, indices: &[usize], modo: Alinhar) -> Result<(), ErroGeometria> {
        let caixas = self.caixas(indices)?;
        if caixas.len() < 2 {
            return Err(ErroGeometria::SelecaoInsuficiente);
        }
        let referencia = agrupar_caixas(&caixas).ok_or(ErroGeometria::CoordenadaInvalida)?;
        let alvo = match modo {
            Alinhar::Esquerda => referencia.x,
            Alinhar::CentroHorizontal => referencia.x + referencia.largura / 2.,
            Alinhar::Direita => referencia.x + referencia.largura,
            Alinhar::Topo => referencia.y,
            Alinhar::CentroVertical => referencia.y + referencia.altura / 2.,
            Alinhar::Base => referencia.y + referencia.altura,
        };
        for (&indice, b) in indices.iter().zip(caixas) {
            let atual = match modo {
                Alinhar::Esquerda => b.x,
                Alinhar::CentroHorizontal => b.x + b.largura / 2.,
                Alinhar::Direita => b.x + b.largura,
                Alinhar::Topo => b.y,
                Alinhar::CentroVertical => b.y + b.altura / 2.,
                Alinhar::Base => b.y + b.altura,
            };
            let distancia = alvo - atual;
            let (dx, dy) = match modo {
                Alinhar::Esquerda | Alinhar::CentroHorizontal | Alinhar::Direita => (distancia, 0.),
                _ => (0., distancia),
            };
            self.figuras[indice] =
                transformar(&self.figuras[indice], Transformacao::transladar(dx, dy));
        }
        Ok(())
    }
    /// Mantém os extremos da seleção e distribui espaço igual ENTRE figuras.
    pub fn distribuir(&mut self, indices: &[usize], eixo: Eixo) -> Result<(), ErroGeometria> {
        let caixas = self.caixas(indices)?;
        if caixas.len() < 3 {
            return Err(ErroGeometria::SelecaoInsuficiente);
        }
        let mut ordenado: Vec<(usize, Retangulo)> = indices.iter().copied().zip(caixas).collect();
        let pos = |r: &Retangulo| match eixo {
            Eixo::Horizontal => r.x,
            Eixo::Vertical => r.y,
        };
        let tam = |r: &Retangulo| match eixo {
            Eixo::Horizontal => r.largura,
            Eixo::Vertical => r.altura,
        };
        ordenado.sort_by(|a, b| pos(&a.1).total_cmp(&pos(&b.1)));
        let ultimo = ordenado.len() - 1;
        let inicio = pos(&ordenado[0].1);
        let final_ = pos(&ordenado[ultimo].1) + tam(&ordenado[ultimo].1);
        let largura: f32 = ordenado.iter().map(|(_, r)| tam(r)).sum();
        let passo = (final_ - inicio - largura) / (ultimo as f32);
        let mut cursor = inicio + tam(&ordenado[0].1) + passo;
        for (indice, caixa) in ordenado.iter().skip(1).take(ultimo - 1) {
            let delta = cursor - pos(caixa);
            let (dx, dy) = match eixo {
                Eixo::Horizontal => (delta, 0.),
                Eixo::Vertical => (0., delta),
            };
            self.figuras[*indice] =
                transformar(&self.figuras[*indice], Transformacao::transladar(dx, dy));
            cursor += tam(caixa) + passo;
        }
        Ok(())
    }
    fn caixas(&self, indices: &[usize]) -> Result<Vec<Retangulo>, ErroGeometria> {
        let mut caixas = Vec::with_capacity(indices.len());
        let mut vistos = std::collections::HashSet::new();
        for &i in indices {
            if !vistos.insert(i) || i >= self.figuras.len() {
                return Err(ErroGeometria::IndiceInvalido);
            }
            caixas.push(
                self.figuras[i]
                    .bounding_box()
                    .ok_or(ErroGeometria::CoordenadaInvalida)?,
            );
        }
        Ok(caixas)
    }
}
fn agrupar_caixas(caixas: &[Retangulo]) -> Option<Retangulo> {
    let r = caixas.first()?;
    let (mut x, mut y, mut max_x, mut max_y) = (r.x, r.y, r.x + r.largura, r.y + r.altura);
    for r in &caixas[1..] {
        x = x.min(r.x);
        y = y.min(r.y);
        max_x = max_x.max(r.x + r.largura);
        max_y = max_y.max(r.y + r.altura);
    }
    Some(Retangulo::novo(x, y, max_x - x, max_y - y))
}
#[cfg(test)]
mod testes {
    use super::*;
    use crate::{Cor, Estilo};
    fn ret(x: f32, y: f32, w: f32, h: f32) -> Figura {
        Figura::Retangulo {
            area: Retangulo::novo(x, y, w, h),
            estilo: Estilo::default(),
        }
    }
    fn perto(a: f32, b: f32) {
        assert!((a - b).abs() < 0.1, "{a} vs {b}");
    }
    #[test]
    fn hit_test_zorder_clip_e_bounds() {
        let mut c = Cena::nova(200., 200.);
        c.adicionar(ret(0., 0., 100., 100.));
        c.adicionar(ret(20., 20., 30., 30.));
        assert_eq!(c.hit_test(Ponto::novo(25., 25.), 0.), Some(1));
        c.enviar_fundo(1).unwrap();
        assert_eq!(c.hit_test(Ponto::novo(25., 25.), 0.), Some(1));
        c.trazer_frente(0).unwrap();
        assert_eq!(c.hit_test(Ponto::novo(25., 25.), 0.), Some(1));
        let clip = caminho_retangulo(Retangulo::novo(0., 0., 15., 15.));
        c.adicionar(Figura::Recorte {
            figura: Box::new(ret(0., 0., 100., 100.)),
            recorte: clip,
        });
        assert_eq!(c.hit_test(Ponto::novo(10., 10.), 0.), Some(2));
        assert_eq!(c.hit_test(Ponto::novo(20., 10.), 0.), Some(0));
        assert_eq!(
            c.figuras[2].bounding_box(),
            Some(Retangulo::novo(0., 0., 15., 15.))
        );
    }
    #[test]
    fn bezier_elipse_seta_e_arco_atingem_e_limitam() {
        let linha = EstiloVetor {
            preenchimento: None,
            stroke: Some(Stroke::novo(Cor::rgb(10, 10, 10), 2.)),
            ..Default::default()
        };
        let b = Figura::Bezier {
            inicio: Ponto::novo(0., 0.),
            controle1: Ponto::novo(0., 20.),
            controle2: Ponto::novo(20., 20.),
            fim: Ponto::novo(20., 0.),
            estilo: linha.clone(),
        };
        let bb = b.bounding_box().unwrap();
        assert!(bb.altura > 15.);
        assert!(b.atinge(Ponto::novo(10., 15.), 1.));
        let e = Figura::Elipse {
            centro: Ponto::novo(50., 50.),
            raio_x: 20.,
            raio_y: 10.,
            estilo: EstiloVetor::default(),
        };
        assert!(e.atinge(Ponto::novo(50., 50.), 0.));
        let a = Figura::Arco {
            centro: Ponto::novo(0., 0.),
            raio_x: 10.,
            raio_y: 10.,
            inicio: 0.,
            varredura: std::f32::consts::PI,
            estilo: linha.clone(),
        };
        assert!(a.atinge(Ponto::novo(0., 10.), 2.));
        let s = Figura::Seta {
            de: Ponto::novo(0., 0.),
            ate: Ponto::novo(20., 0.),
            tamanho_ponta: 5.,
            estilo: linha,
        };
        assert!(s.atinge(Ponto::novo(10., 0.), 1.));
    }
    #[test]
    fn seta_preenchida_desenha_haste_e_cabeca_sem_stroke() {
        let seta = Figura::Seta {
            de: Ponto::novo(0., 10.),
            ate: Ponto::novo(50., 10.),
            tamanho_ponta: 10.,
            estilo: EstiloVetor::default(),
        };
        let partes = seta.caminho(0.1).aproximar(0.1);
        assert_eq!(partes.len(), 1);
        assert!(partes[0].fechado);
        assert_eq!(partes[0].pontos.len(), 7);
        assert!(seta.atinge(Ponto::novo(10., 10.), 0.));
        assert!(seta.atinge(Ponto::novo(43., 10.), 0.));
        assert!(!seta.atinge(Ponto::novo(10., 15.), 0.));
    }

    #[test]
    fn seta_stroke_tem_ponta_aberta_sem_base_artificial() {
        let seta = Figura::Seta {
            de: Ponto::novo(0., 0.),
            ate: Ponto::novo(30., 0.),
            tamanho_ponta: 6.,
            estilo: EstiloVetor {
                preenchimento: None,
                stroke: Some(Stroke::novo(Cor::rgb(0, 0, 0), 2.)),
                ..Default::default()
            },
        };
        let partes = seta.caminho(0.1).aproximar(0.1);
        assert_eq!(partes.len(), 2);
        assert!(partes.iter().all(|s| !s.fechado));
        assert!(seta.atinge(Ponto::novo(15., 0.), 0.));
        assert!(seta
            .caminho(0.1)
            .comandos
            .iter()
            .all(|comando| !matches!(comando, Comando::Fechar)));
    }

    #[test]
    fn par_impar_cria_furo_e_nao_zero_preenche() {
        let mut caminho = Caminho::novo();
        for r in [
            Retangulo::novo(0., 0., 30., 30.),
            Retangulo::novo(10., 10., 10., 10.),
        ] {
            caminho.comandos.extend(caminho_retangulo(r).comandos);
        }
        caminho.regra = RegraPreenchimento::ParImpar;
        let mut f = Figura::Caminho {
            caminho: caminho.clone(),
            estilo: EstiloVetor::default(),
        };
        assert!(f.atinge(Ponto::novo(5., 5.), 0.));
        assert!(!f.atinge(Ponto::novo(15., 15.), 0.));
        caminho.regra = RegraPreenchimento::NaoZero;
        f = Figura::Caminho {
            caminho,
            estilo: EstiloVetor::default(),
        };
        assert!(f.atinge(Ponto::novo(15., 15.), 0.));
    }
    #[test]
    fn dash_e_transformacao_preservam_hit_test() {
        let mut traco = Stroke::novo(Cor::rgb(0, 0, 0), 2.);
        traco.dash = vec![5., 5.];
        let f = Figura::Polilinha {
            pontos: vec![Ponto::novo(0., 0.), Ponto::novo(20., 0.)],
            fechada: false,
            estilo: Estilo {
                preenchimento: None,
                contorno: Some(Cor::rgb(0, 0, 0)),
                espessura: 2.,
            },
        };
        // O caminho antigo continua convertível para o estilo vetorial novo.
        let linha = Figura::Caminho {
            caminho: f.caminho(0.1),
            estilo: EstiloVetor {
                preenchimento: None,
                stroke: Some(traco),
                ..Default::default()
            },
        };
        assert!(linha.atinge(Ponto::novo(3., 0.), 0.));
        assert!(!linha.atinge(Ponto::novo(7., 0.), 0.));
        let movida = transformar(&linha, Transformacao::transladar(10., 10.));
        assert!(movida.atinge(Ponto::novo(13., 10.), 0.));
        assert!(!movida.atinge(Ponto::novo(17., 10.), 0.));
    }

    #[test]
    fn preenchimento_de_caminho_aberto_fecha_implicitamente() {
        let poligono = Figura::Polilinha {
            pontos: vec![
                Ponto::novo(0., 0.),
                Ponto::novo(20., 0.),
                Ponto::novo(10., 20.),
            ],
            fechada: false,
            estilo: Estilo::default(),
        };
        assert!(poligono.atinge(Ponto::novo(10., 5.), 0.));
        assert!(!poligono.atinge(Ponto::novo(30., 5.), 0.));
    }
    #[test]
    fn formas_transparentes_nao_interceptam_cliques() {
        let invisivel = Figura::Retangulo {
            area: Retangulo::novo(0., 0., 20., 20.),
            estilo: Estilo {
                preenchimento: Some(Cor::rgba(255, 0, 0, 0)),
                contorno: Some(Cor::rgba(0, 0, 255, 0)),
                espessura: 3.,
            },
        };
        assert!(!invisivel.atinge(Ponto::novo(10., 10.), 0.));
        assert!(!invisivel.atinge(Ponto::novo(0., 10.), 1.));
        let mut cena = Cena::nova(30., 30.);
        cena.adicionar(ret(0., 0., 20., 20.));
        cena.adicionar(invisivel);
        assert_eq!(cena.hit_test(Ponto::novo(10., 10.), 0.), Some(0));
        // A combinação de alfa da cor e opacidade do objeto também pode
        // arredondar para zero antes de chegar ao renderizador.
        let quase_invisivel = Figura::Caminho {
            caminho: caminho_retangulo(Retangulo::novo(0., 0., 20., 20.)),
            estilo: EstiloVetor {
                opacidade: 0.1,
                preenchimento: Some(crate::Preenchimento::Solido(Cor::rgba(255, 0, 0, 1))),
                stroke: None,
                texto: None,
            },
        };
        assert!(!quase_invisivel.atinge(Ponto::novo(10., 10.), 0.));
    }
    #[test]
    fn clip_aberto_fecha_implicitamente_para_hit_test() {
        let clip = caminho_poligono(
            &[
                Ponto::novo(0., 0.),
                Ponto::novo(20., 0.),
                Ponto::novo(10., 20.),
            ],
            false,
        );
        let recortada = Figura::Recorte {
            figura: Box::new(ret(0., 0., 20., 20.)),
            recorte: clip,
        };
        assert!(recortada.atinge(Ponto::novo(10., 5.), 0.));
        assert!(!recortada.atinge(Ponto::novo(19., 15.), 0.));
    }

    #[test]
    fn alinhamento_distribuicao_e_snap() {
        let mut c = Cena::nova(200., 100.);
        c.adicionar(ret(0., 0., 10., 10.));
        c.adicionar(ret(40., 5., 10., 10.));
        c.adicionar(ret(100., 10., 10., 10.));
        c.alinhar(&[0, 1, 2], Alinhar::Topo).unwrap();
        for f in &c.figuras {
            perto(f.bounding_box().unwrap().y, 0.);
        }
        c.distribuir(&[0, 1, 2], Eixo::Horizontal).unwrap();
        perto(c.figuras[1].bounding_box().unwrap().x, 50.);
        let s = c.snap(Ponto::novo(5.2, 5.1), 1.).unwrap();
        assert_eq!(s.indice, 0);
        perto(s.ponto.x, 5.);
    }
}

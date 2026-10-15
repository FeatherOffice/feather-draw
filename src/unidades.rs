//! Coordenadas independentes de GUI: pt (Slide/Docs), px lógicos (Sheet)
//! e px físicos apenas na fronteira de renderização/exportação.
//! Iced usa pixels LÓGICOS: DPR não entra duas vezes no hit-test.

use crate::{Ponto, Retangulo};

pub const PONTOS_POR_POLEGADA: f32 = 72.0;
pub const PIXELS_LOGICOS_POR_POLEGADA: f32 = 96.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unidade {
    Ponto,
    PixelLogico,
}

impl Unidade {
    pub const fn escala_px(self) -> f32 {
        match self {
            Self::Ponto => PIXELS_LOGICOS_POR_POLEGADA / PONTOS_POR_POLEGADA,
            Self::PixelLogico => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroUnidade {
    NaoFinito,
    EscalaInvalida,
    IntervaloInvalido,
}

/// `pan`: deslocamento da origem do documento (em sua própria unidade).
/// `origem`: origem do viewport na janela, em px lógicos do Iced.
/// `zoom`: multiplicador adicional, NÃO inclui DPR físico do monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub unidade: Unidade,
    pub zoom: f32,
    pub pan: Ponto,
    pub origem: Ponto,
    /// Pixel físico / pixel lógico; apenas saída para framebuffer/exportação.
    pub dpr: f32,
}

impl Viewport {
    pub fn novo(
        unidade: Unidade,
        zoom: f32,
        pan: Ponto,
        origem: Ponto,
        dpr: f32,
    ) -> Result<Self, ErroUnidade> {
        if ![zoom, dpr, pan.x, pan.y, origem.x, origem.y]
            .iter()
            .all(|x| x.is_finite())
        {
            return Err(ErroUnidade::NaoFinito);
        }
        if zoom <= 0.0 || dpr <= 0.0 {
            return Err(ErroUnidade::EscalaInvalida);
        }
        Ok(Self {
            unidade,
            zoom,
            pan,
            origem,
            dpr,
        })
    }
    /// Constrói o viewport recebendo diretamente a escala do Canvas em
    /// pixels lógicos por unidade documental, sem recalcular DPI no consumidor.
    /// O `zoom` público continua sendo fator sem dimensões relativo a 96/72.
    pub fn de_escala_logica(
        unidade: Unidade,
        escala_logica: f32,
        pan: Ponto,
        origem: Ponto,
        dpr: f32,
    ) -> Result<Self, ErroUnidade> {
        if !escala_logica.is_finite() {
            return Err(ErroUnidade::NaoFinito);
        }
        if escala_logica <= 0.0 {
            return Err(ErroUnidade::EscalaInvalida);
        }
        let zoom = escala_logica / unidade.escala_px();
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(ErroUnidade::EscalaInvalida);
        }
        Self::novo(unidade, zoom, pan, origem, dpr)
    }

    /// Centraliza um documento num viewport Iced (em pixels **lógicos**).
    ///
    /// `area` pode começar fora de (0, 0); `deslocamento` é pan de UI em
    /// pixels lógicos, ao contrário de `Viewport::pan` (unidades do documento).
    /// `ocupacao` é a fração da área usada no encaixe (ex.: 0.95).
    /// O DPI físico só entra em `fisico` e nunca é multiplicado no hit-test.
    ///
    /// Não pressupõe o tamanho de papel: Slide/Docs informam suas dimensões
    /// em pontos, enquanto Sheet pode usar PixelLogico, sem alterar o storage.
    pub fn centralizado(
        unidade: Unidade,
        area: Retangulo,
        largura_documento: f32,
        altura_documento: f32,
        ocupacao: f32,
        zoom: f32,
        deslocamento: Ponto,
        dpr: f32,
    ) -> Result<Self, ErroUnidade> {
        let dados = [
            area.x, area.y, area.largura, area.altura,
            largura_documento, altura_documento, ocupacao, zoom,
            deslocamento.x, deslocamento.y, dpr,
        ];
        if !dados.iter().all(|v| v.is_finite()) {
            return Err(ErroUnidade::NaoFinito);
        }
        if area.largura <= 0.0 || area.altura <= 0.0
            || largura_documento <= 0.0 || altura_documento <= 0.0
            || ocupacao <= 0.0 || ocupacao > 1.0 || zoom <= 0.0 || dpr <= 0.0
        {
            return Err(ErroUnidade::EscalaInvalida);
        }
        let escala_logica = ((area.largura * ocupacao) / largura_documento)
            .min((area.altura * ocupacao) / altura_documento) * zoom;
        let fator = unidade.escala_px();
        if !escala_logica.is_finite() || escala_logica <= 0.0
            || !(escala_logica / fator).is_finite()
        {
            return Err(ErroUnidade::EscalaInvalida);
        }
        let origem = Ponto::novo(
            area.x + (area.largura - largura_documento * escala_logica) / 2.0
                + deslocamento.x,
            area.y + (area.altura - altura_documento * escala_logica) / 2.0
                + deslocamento.y,
        );
        if !origem.x.is_finite() || !origem.y.is_finite() {
            return Err(ErroUnidade::EscalaInvalida);
        }
        Self::novo(
            unidade,
            escala_logica / fator,
            Ponto::default(),
            origem,
            dpr,
        )
    }

    /// Deslocamento em pixels lógicos do canvas para unidades do documento.
    /// Útil para drag, ajuste de recorte e snapping sem multiplicar DPR.
    pub fn delta_documento(self, delta_logico: Ponto) -> Ponto {
        Ponto::novo(
            delta_logico.x / self.escala(),
            delta_logico.y / self.escala(),
        )
    }

    pub fn escala(self) -> f32 {
        self.unidade.escala_px() * self.zoom
    }
    pub fn logico(self, p: Ponto) -> Ponto {
        let k = self.escala();
        Ponto::novo(
            (p.x - self.pan.x) * k + self.origem.x,
            (p.y - self.pan.y) * k + self.origem.y,
        )
    }
    pub fn documento(self, p: Ponto) -> Ponto {
        let k = self.escala();
        Ponto::novo(
            (p.x - self.origem.x) / k + self.pan.x,
            (p.y - self.origem.y) / k + self.pan.y,
        )
    }
    pub fn fisico(self, p: Ponto) -> Ponto {
        let p = self.logico(p);
        Ponto::novo(p.x * self.dpr, p.y * self.dpr)
    }
    pub fn area_logica(self, r: Retangulo) -> Retangulo {
        let p = self.logico(Ponto::novo(r.x, r.y));
        Retangulo::novo(
            p.x,
            p.y,
            r.largura * self.escala(),
            r.altura * self.escala(),
        )
    }
    pub fn tolerancia_documento(self, px_logicos: f32) -> f32 {
        px_logicos.max(0.0) / self.escala()
    }
}

/// Viewport sobre uma janela de células já materializadas, SEM percorrer as
/// milhões de linhas do armazenamento colunar do Sheet. Offsets são cumulativos
/// e estritamente crescentes; binary_search é O(log n) por coordenada.
#[derive(Debug, Clone, PartialEq)]
pub struct GradeVisivel {
    pub primeira_linha: u32,
    pub primeira_coluna: u32,
    pub x: Vec<f32>,
    pub y: Vec<f32>,
}

impl GradeVisivel {
    pub fn nova(
        primeira_linha: u32,
        primeira_coluna: u32,
        larguras: &[f32],
        alturas: &[f32],
    ) -> Result<Self, ErroUnidade> {
        fn acumulado(valores: &[f32]) -> Result<Vec<f32>, ErroUnidade> {
            let mut fim = vec![0.0_f32];
            for &tamanho in valores {
                if !tamanho.is_finite() {
                    return Err(ErroUnidade::NaoFinito);
                }
                if tamanho <= 0.0 {
                    return Err(ErroUnidade::IntervaloInvalido);
                }
                let soma = fim.last().copied().unwrap_or(0.0) + tamanho;
                if !soma.is_finite() || soma <= *fim.last().unwrap_or(&0.0) {
                    return Err(ErroUnidade::IntervaloInvalido);
                }
                fim.push(soma);
            }
            Ok(fim)
        }
        Ok(Self {
            primeira_linha,
            primeira_coluna,
            x: acumulado(larguras)?,
            y: acumulado(alturas)?,
        })
    }
    pub fn celula(&self, linha: u32, coluna: u32) -> Option<Retangulo> {
        let (r, c) = (
            linha.checked_sub(self.primeira_linha)? as usize,
            coluna.checked_sub(self.primeira_coluna)? as usize,
        );
        Some(Retangulo::novo(
            *self.x.get(c)?,
            *self.y.get(r)?,
            self.x.get(c + 1)? - self.x[c],
            self.y.get(r + 1)? - self.y[r],
        ))
    }
    pub fn endereco(&self, p: Ponto) -> Option<(u32, u32)> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        fn buscar(eixos: &[f32], v: f32) -> Option<usize> {
            if v < 0.0 || v >= *eixos.last()? || eixos.len() < 2 {
                return None;
            }
            let k = eixos.partition_point(|x| *x <= v);
            k.checked_sub(1)
        }
        Some((
            self.primeira_linha
                .checked_add(buscar(&self.y, p.y)? as u32)?,
            self.primeira_coluna
                .checked_add(buscar(&self.x, p.x)? as u32)?,
        ))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    fn perto(a: f32, b: f32) {
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }
    #[test]
    fn escala_logica_docs_nao_duplica_96_sobre_72() {
        let v = Viewport::de_escala_logica(
            Unidade::Ponto, 2.0, Ponto::default(), Ponto::default(), 1.5,
        ).unwrap();
        perto(v.escala(), 2.0);
        let clique = Ponto::novo(144.0, 72.0);
        perto(v.documento(clique).x, 72.0);
        perto(v.documento(clique).y, 36.0);
        perto(v.fisico(Ponto::novo(72.0, 36.0)).x, 216.0);
        let sheet = Viewport::de_escala_logica(
            Unidade::PixelLogico, 1.25, Ponto::default(), Ponto::default(), 2.0,
        ).unwrap();
        perto(sheet.escala(), 1.25);
        perto(sheet.documento(Ponto::novo(125.0, 75.0)).x, 100.0);
    }

    #[test]
    fn centralizar_slide_preserva_encaixe_pontos_e_pan_logico() {
        let area = Retangulo::novo(0.0, 0.0, 1200.0, 750.0);
        let pan = Ponto::novo(28.0, -14.0);
        let v = Viewport::centralizado(
            Unidade::Ponto, area, 960.0, 540.0, 0.95, 1.5, pan, 2.0,
        ).unwrap();
        let esperado = ((1200.0 * 0.95_f32) / 960.0_f32)
            .min((750.0 * 0.95_f32) / 540.0_f32) * 1.5_f32;
        perto(v.escala(), esperado);
        perto(v.origem.x, (1200.0 - 960.0 * esperado) / 2.0 + 28.0);
        perto(v.origem.y, (750.0 - 540.0 * esperado) / 2.0 - 14.0);
        let ponto = Ponto::novo(360.0, 270.0);
        let logico = v.logico(ponto);
        perto(v.documento(logico).x, ponto.x);
        perto(v.documento(logico).y, ponto.y);
        perto(v.fisico(ponto).x, logico.x * 2.0);
        perto(v.delta_documento(Ponto::novo(esperado * 8.0, esperado * -3.0)).x, 8.0);
        perto(v.delta_documento(Ponto::novo(esperado * 8.0, esperado * -3.0)).y, -3.0);
    }

    #[test]
    fn area_iced_com_origem_nao_nula_e_dpr_diferente() {
        let area = Retangulo::novo(20.0, 40.0, 400.0, 300.0);
        let vp = Viewport::centralizado(
            Unidade::PixelLogico, area, 100.0, 100.0, 1.0, 1.0,
            Ponto::default(), 1.5,
        ).unwrap();
        perto(vp.escala(), 3.0);
        perto(vp.origem.x, 70.0);
        perto(vp.origem.y, 40.0);
        perto(vp.documento(vp.logico(Ponto::novo(10.0, 20.0))).x, 10.0);
        perto(vp.tolerancia_documento(6.0), 2.0);
        perto(vp.fisico(Ponto::novo(0.0, 0.0)).x, 105.0);
    }

    #[test]
    fn rejeita_encaixe_zero_infinito_e_overflow() {
        let area = Retangulo::novo(0.0, 0.0, 500.0, 300.0);
        let construir = |a, w, h, oc, z| Viewport::centralizado(
            Unidade::Ponto, a, w, h, oc, z, Ponto::default(), 1.0,
        );
        assert_eq!(
            construir(area, 0.0, 540.0, 0.95, 1.0),
            Err(ErroUnidade::EscalaInvalida)
        );
        assert_eq!(
            construir(area, 960.0, 540.0, 1.1, 1.0),
            Err(ErroUnidade::EscalaInvalida)
        );
        assert_eq!(
            construir(area, f32::NAN, 540.0, 0.95, 1.0),
            Err(ErroUnidade::NaoFinito)
        );
        assert_eq!(
            construir(Retangulo::novo(0.0, 0.0, 0.0, 300.0), 960.0, 540.0, 0.95, 1.0),
            Err(ErroUnidade::EscalaInvalida)
        );
        assert_eq!(
            construir(area, 960.0, 540.0, 0.95, f32::MAX),
            Err(ErroUnidade::EscalaInvalida)
        );
    }

    #[test]
    fn pontos_iced_dpi_e_zoom_sem_dupla_escala() {
        let v = Viewport::novo(
            Unidade::Ponto,
            1.5,
            Ponto::novo(12., 8.),
            Ponto::novo(5., 10.),
            2.,
        )
        .unwrap();
        let p = Ponto::novo(84., 44.);
        let q = v.logico(p);
        perto(q.x, 149.);
        perto(q.y, 82.);
        perto(v.fisico(p).x, 298.);
        perto(v.documento(q).x, p.x);
        perto(v.documento(q).y, p.y);
        perto(v.tolerancia_documento(8.), 4.);
    }
    #[test]
    fn sheet_usa_pixels_logicos_e_busca_logaritmica() {
        let g = GradeVisivel::nova(1000, 20, &[40., 120., 30.], &[20., 25.]).unwrap();
        assert_eq!(g.endereco(Ponto::novo(40., 24.)), Some((1001, 21)));
        assert_eq!(
            g.celula(1001, 21),
            Some(Retangulo::novo(40., 20., 120., 25.))
        );
        assert_eq!(g.endereco(Ponto::novo(190., 1.)), None);
        assert_eq!(g.endereco(Ponto::novo(-1., 0.)), None);
        let v = Viewport::novo(
            Unidade::PixelLogico,
            2.,
            Ponto::novo(0., 0.),
            Ponto::novo(0., 0.),
            1.5,
        )
        .unwrap();
        perto(v.logico(Ponto::novo(40., 20.)).x, 80.);
    }
    #[test]
    fn rejeita_parametros_invalidos() {
        assert_eq!(
            Viewport::novo(Unidade::Ponto, 0., Ponto::default(), Ponto::default(), 1.),
            Err(ErroUnidade::EscalaInvalida)
        );
        assert_eq!(
            GradeVisivel::nova(0, 0, &[0.], &[]),
            Err(ErroUnidade::IntervaloInvalido)
        );
    }
}

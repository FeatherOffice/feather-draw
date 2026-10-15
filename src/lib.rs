//! Geometria neutra: uma Cena pode ser desenhada por Iced, PDF ou SVG.
//! Os consumidores escolhem a unidade (pontos, pixels, etc.) e a mantêm
//! consistente; este crate não depende de GUI nem de formatos de arquivo.

pub mod caminhos;
pub mod estilos;
pub mod geometria;
pub mod pdf;
pub mod svg;
/// Tinta à mão livre da aba Desenhar (traços, borracha, laço, formas).
pub mod tinta;
pub mod unidades;

pub use caminhos::{arco_eliptico, Caminho, Comando, RegraPreenchimento, Subcaminho};
pub use estilos::{EstiloTexto, EstiloVetor, Extremo, Juncao, Parada, Preenchimento, Stroke};
pub use geometria::{Alinhar, Eixo, ErroGeometria, PontoSnap};
pub use pdf::{exportar_pdf, exportar_pdf_com_texto, largura_texto, DocumentoPdf, ErroPdf,
    FontePdf, PaginaPdf, TextoPdf};
pub use svg::{exportar_svg, ErroSvg};
pub use tinta::{Ferramenta, Forma, PontoTinta, Tinta, Traco};
pub use unidades::{GradeVisivel, Unidade, Viewport};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ponto {
    pub x: f32,
    pub y: f32,
}
impl Ponto {
    pub const fn novo(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Retangulo {
    pub x: f32,
    pub y: f32,
    pub largura: f32,
    pub altura: f32,
}
impl Retangulo {
    pub const fn novo(x: f32, y: f32, largura: f32, altura: f32) -> Self {
        Self {
            x,
            y,
            largura,
            altura,
        }
    }
    pub fn contem(self, p: Ponto) -> bool {
        self.largura >= 0.0
            && self.altura >= 0.0
            && p.x >= self.x
            && p.y >= self.y
            && p.x <= self.x + self.largura
            && p.y <= self.y + self.altura
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl Cor {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Estilo {
    pub preenchimento: Option<Cor>,
    pub contorno: Option<Cor>,
    pub espessura: f32,
}
impl Default for Estilo {
    fn default() -> Self {
        Self {
            preenchimento: Some(Cor::rgb(0, 0, 0)),
            contorno: None,
            espessura: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Figura {
    /// Contratos legados continuam intactos (incluindo Charts).
    Retangulo { area: Retangulo, estilo: Estilo },
    Polilinha {
        pontos: Vec<Ponto>,
        fechada: bool,
        estilo: Estilo,
    },
    Elipse {
        centro: Ponto,
        raio_x: f32,
        raio_y: f32,
        estilo: EstiloVetor,
    },
    Arco {
        centro: Ponto,
        raio_x: f32,
        raio_y: f32,
        inicio: f32,
        varredura: f32,
        estilo: EstiloVetor,
    },
    Bezier {
        inicio: Ponto,
        controle1: Ponto,
        controle2: Ponto,
        fim: Ponto,
        estilo: EstiloVetor,
    },
    Seta {
        de: Ponto,
        ate: Ponto,
        tamanho_ponta: f32,
        estilo: EstiloVetor,
    },
    Caminho {
        caminho: Caminho,
        estilo: EstiloVetor,
    },
    /// Clip não altera os pontos originais. Os backends devem respeitar o
    /// caminho do recorte ao desenhar a figura aninhada.
    Recorte {
        figura: Box<Figura>,
        recorte: Caminho,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cena {
    pub largura: f32,
    pub altura: f32,
    pub figuras: Vec<Figura>,
}
impl Cena {
    pub fn nova(largura: f32, altura: f32) -> Self {
        Self {
            largura,
            altura,
            figuras: Vec::new(),
        }
    }
    pub fn adicionar(&mut self, figura: Figura) {
        self.figuras.push(figura);
    }
}

/// Matriz afim [a c tx; b d ty; 0 0 1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transformacao {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}
impl Transformacao {
    pub const fn identidade() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub const fn transladar(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::identidade()
        }
    }
    pub const fn escalar(x: f32, y: f32) -> Self {
        Self {
            a: x,
            d: y,
            b: 0.0,
            c: 0.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn girar(radianos: f32) -> Self {
        let (s, c) = radianos.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn aplicar(self, p: Ponto) -> Ponto {
        Ponto::novo(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }
    /// Primeiro aplica outra, depois self.
    pub fn compor(self, outra: Self) -> Self {
        Self {
            a: self.a * outra.a + self.c * outra.b,
            b: self.b * outra.a + self.d * outra.b,
            c: self.a * outra.c + self.c * outra.d,
            d: self.b * outra.c + self.d * outra.d,
            tx: self.a * outra.tx + self.c * outra.ty + self.tx,
            ty: self.b * outra.tx + self.d * outra.ty + self.ty,
        }
    }
}

/// Retângulos transformados viram polígonos para preservar sua rotação.
pub fn transformar(figura: &Figura, t: Transformacao) -> Figura {
    match figura {
        Figura::Polilinha {
            pontos,
            fechada,
            estilo,
        } => Figura::Polilinha {
            pontos: pontos.iter().copied().map(|p| t.aplicar(p)).collect(),
            fechada: *fechada,
            estilo: estilo.clone(),
        },
        Figura::Retangulo { area, estilo } => {
            let (x, y, w, h) = (area.x, area.y, area.largura, area.altura);
            Figura::Polilinha {
                pontos: [
                    Ponto::novo(x, y),
                    Ponto::novo(x + w, y),
                    Ponto::novo(x + w, y + h),
                    Ponto::novo(x, y + h),
                ]
                .iter()
                .copied()
                .map(|p| t.aplicar(p))
                .collect(),
                fechada: true,
                estilo: estilo.clone(),
            }
        }
        Figura::Elipse { estilo, .. }
        | Figura::Arco { estilo, .. }
        | Figura::Bezier { estilo, .. }
        | Figura::Seta { estilo, .. } => Figura::Caminho {
            caminho: figura.caminho(0.1).transformar(t),
            estilo: estilo.clone(),
        },
        Figura::Caminho { caminho, estilo } => Figura::Caminho {
            caminho: caminho.transformar(t),
            estilo: estilo.clone(),
        },
        Figura::Recorte { figura, recorte } => Figura::Recorte {
            figura: Box::new(transformar(figura, t)),
            recorte: recorte.transformar(t),
        },
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn composicao_e_rotacao() {
        let t = Transformacao::transladar(10.0, 20.0)
            .compor(Transformacao::girar(std::f32::consts::FRAC_PI_2));
        let p = t.aplicar(Ponto::novo(2.0, 0.0));
        assert!((p.x - 10.0).abs() < 0.001);
        assert!((p.y - 22.0).abs() < 0.001);
    }
    #[test]
    fn retangulo_girado_tem_quatro_vertices() {
        let fig = Figura::Retangulo {
            area: Retangulo::novo(0.0, 0.0, 2.0, 3.0),
            estilo: Estilo::default(),
        };
        match transformar(&fig, Transformacao::girar(0.3)) {
            Figura::Polilinha {
                pontos, fechada, ..
            } => {
                assert_eq!(pontos.len(), 4);
                assert!(fechada);
            }
            _ => panic!("retângulo deve virar polilinha"),
        }
    }
}

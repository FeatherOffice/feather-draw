//! Exportador vetorial determinístico: `Cena` → SVG 1.1.
//!
//! Sem Iced ou IO específico, ordem z = ordem de `Cena::figuras`.
//! Identificadores de gradiente/clip alocados por travessia em profundidade;
//! mesma cena → mesmos bytes (inclusive em plataformas diferentes).
use crate::{
    caminhos::{Caminho, Comando, RegraPreenchimento},
    estilos::{EstiloVetor, Extremo, Juncao, Preenchimento},
    Cena, Cor, Figura, Ponto,
};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErroSvg {
    DimensaoInvalida,
    GeometriaInvalida,
    EstiloInvalido,
}

fn numero(valor: f32) -> Result<String, ErroSvg> {
    if !valor.is_finite() {
        return Err(ErroSvg::GeometriaInvalida);
    }
    // Coordenadas arredondadas para 4 casas para manter o SVG compacto e
    // evitar divergências entre renderizadores por ruído de float.
    if valor.abs() < 0.00005 {
        return Ok("0".into());
    }
    let mut s = format!("{valor:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    Ok(s)
}

fn xy(p: Ponto) -> Result<String, ErroSvg> {
    Ok(format!("{} {}", numero(p.x)?, numero(p.y)?))
}

fn d(c: &Caminho) -> Result<String, ErroSvg> {
    let mut partes = Vec::with_capacity(c.comandos.len());
    let mut iniciou = false;
    for comando in &c.comandos {
        partes.push(match *comando {
            Comando::Mover(p) => {
                iniciou = true;
                format!("M {}", xy(p)?)
            }
            Comando::Linha(p) => {
                if !iniciou {
                    return Err(ErroSvg::GeometriaInvalida);
                }
                format!("L {}", xy(p)?)
            }
            Comando::Quadratica { controle, fim } => {
                if !iniciou {
                    return Err(ErroSvg::GeometriaInvalida);
                }
                format!("Q {} {}", xy(controle)?, xy(fim)?)
            }
            Comando::Cubica {
                controle1,
                controle2,
                fim,
            } => {
                if !iniciou {
                    return Err(ErroSvg::GeometriaInvalida);
                }
                format!("C {} {} {}", xy(controle1)?, xy(controle2)?, xy(fim)?)
            }
            Comando::Fechar => {
                if !iniciou {
                    return Err(ErroSvg::GeometriaInvalida);
                }
                "Z".into()
            }
        });
    }
    if !partes.iter().any(|x| x.starts_with("M ")) {
        return Err(ErroSvg::GeometriaInvalida);
    }
    Ok(partes.join(" "))
}

fn hex(c: Cor) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}
fn alfa(c: Cor) -> String {
    numero(c.a as f32 / 255.).unwrap_or_else(|_| "1".into())
}
fn regra(r: RegraPreenchimento) -> &'static str {
    match r {
        RegraPreenchimento::NaoZero => "nonzero",
        RegraPreenchimento::ParImpar => "evenodd",
    }
}
struct Emissor {
    defs: String,
    corpo: String,
    serial: usize,
}
impl Emissor {
    fn proximo(&mut self, prefixo: &str) -> String {
        let id = format!("{prefixo}{}", self.serial);
        self.serial += 1;
        id
    }
    fn gradiente(&mut self, p: &Preenchimento) -> Result<String, ErroSvg> {
        let id = self.proximo("grad");
        match p {
            Preenchimento::Solido(_) => return Err(ErroSvg::EstiloInvalido),
            Preenchimento::Linear { de, ate, paradas } => {
                writeln!(&mut self.defs,
                    "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">",
                    numero(de.x)?,numero(de.y)?,numero(ate.x)?,numero(ate.y)?).unwrap();
                self.paradas(paradas)?;
                self.defs.push_str("</linearGradient>\n");
            }
            Preenchimento::Radial {
                centro,
                raio,
                paradas,
            } => {
                writeln!(&mut self.defs,
                    "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">",
                    numero(centro.x)?,numero(centro.y)?,numero(*raio)?).unwrap();
                self.paradas(paradas)?;
                self.defs.push_str("</radialGradient>\n");
            }
        }
        Ok(id)
    }
    fn paradas(&mut self, p: &[crate::estilos::Parada]) -> Result<(), ErroSvg> {
        for parada in p {
            writeln!(
                &mut self.defs,
                "<stop offset=\"{}\" stop-color=\"{}\" stop-opacity=\"{}\" />",
                numero(parada.posicao)?,
                hex(parada.cor),
                alfa(parada.cor)
            )
            .unwrap();
        }
        Ok(())
    }
    fn atributos_estilo(&mut self, e: &EstiloVetor) -> Result<String, ErroSvg> {
        if !e.valido() {
            return Err(ErroSvg::EstiloInvalido);
        }
        let mut attrs = String::new();
        match &e.preenchimento {
            None => attrs.push_str(" fill=\"none\""),
            Some(Preenchimento::Solido(c)) => {
                write!(
                    &mut attrs,
                    " fill=\"{}\" fill-opacity=\"{}\"",
                    hex(*c),
                    alfa(*c)
                )
                .unwrap();
            }
            Some(g) => {
                let id = self.gradiente(g)?;
                write!(&mut attrs, " fill=\"url(#{id})\"").unwrap();
            }
        }
        if let Some(s) = &e.stroke {
            write!(
                &mut attrs,
                " stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\"",
                hex(s.cor),
                alfa(s.cor),
                numero(s.largura)?
            )
            .unwrap();
            write!(
                &mut attrs,
                " stroke-linecap=\"{}\"",
                match s.extremo {
                    Extremo::Plano => "butt",
                    Extremo::Redondo => "round",
                    Extremo::Quadrado => "square",
                }
            )
            .unwrap();
            write!(
                &mut attrs,
                " stroke-linejoin=\"{}\" stroke-miterlimit=\"{}\"",
                match s.juncao {
                    Juncao::Miter => "miter",
                    Juncao::Redonda => "round",
                    Juncao::Bevel => "bevel",
                },
                numero(s.limite_miter)?
            )
            .unwrap();
            if !s.dash.is_empty() {
                let dash = s
                    .dash
                    .iter()
                    .map(|&v| numero(v))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(" ");
                write!(
                    &mut attrs,
                    " stroke-dasharray=\"{dash}\" stroke-dashoffset=\"{}\"",
                    numero(s.fase)?
                )
                .unwrap();
            }
        } else {
            attrs.push_str(" stroke=\"none\"");
        }
        write!(&mut attrs, " opacity=\"{}\"", numero(e.opacidade)?).unwrap();
        Ok(attrs)
    }
    fn figura(&mut self, fig: &Figura) -> Result<(), ErroSvg> {
        if let Figura::Recorte { figura, recorte } = fig {
            let clip = d(recorte)?;
            let id = self.proximo("clip");
            writeln!(&mut self.defs,
                "<clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{clip}\" clip-rule=\"{}\" /></clipPath>",
                regra(recorte.regra)).unwrap();
            writeln!(&mut self.corpo, "<g clip-path=\"url(#{id})\">").unwrap();
            self.figura(figura)?;
            self.corpo.push_str("</g>\n");
            return Ok(());
        }
        let caminho = fig.caminho(0.05);
        let path = d(&caminho)?;
        let estilo = fig.estilo_vetor().ok_or(ErroSvg::EstiloInvalido)?;
        let attrs = self.atributos_estilo(&estilo)?;
        writeln!(
            &mut self.corpo,
            "<path d=\"{path}\" fill-rule=\"{}\"{attrs} />",
            regra(caminho.regra)
        )
        .unwrap();
        Ok(())
    }
}

/// Exporta um SVG vetorial com o mesmo sistema de coordenadas da Cena.
/// `viewBox` não converte automaticamente ponto em pixel: cabe ao consumidor
/// definir unidade/tamanho com `Viewport` ao incorporar o SVG no app.
pub fn exportar_svg(cena: &Cena) -> Result<String, ErroSvg> {
    if !cena.largura.is_finite()
        || !cena.altura.is_finite()
        || cena.largura <= 0.
        || cena.altura <= 0.
    {
        return Err(ErroSvg::DimensaoInvalida);
    }
    let mut e = Emissor {
        defs: String::new(),
        corpo: String::new(),
        serial: 0,
    };
    for figura in &cena.figuras {
        e.figura(figura)?;
    }
    let mut out = String::new();
    writeln!(&mut out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        numero(cena.largura)?,numero(cena.altura)?,
        numero(cena.largura)?,numero(cena.altura)?).unwrap();
    if !e.defs.is_empty() {
        out.push_str("<defs>\n");
        out.push_str(&e.defs);
        out.push_str("</defs>\n");
    }
    out.push_str(&e.corpo);
    out.push_str("</svg>\n");
    Ok(out)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{
        estilos::{Parada, Stroke},
        transformar, Estilo, Retangulo, Transformacao,
    };
    #[test]
    fn snapshot_exato_retangulo_estavel() {
        let mut c = Cena::nova(100., 50.);
        c.adicionar(Figura::Retangulo {
            area: Retangulo::novo(5., 10., 20., 15.),
            estilo: Estilo {
                preenchimento: Some(Cor::rgba(255, 0, 128, 128)),
                contorno: None,
                espessura: 1.,
            },
        });
        let svg = exportar_svg(&c).unwrap();
        assert_eq!(svg,concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"100\" height=\"50\" viewBox=\"0 0 100 50\">\n",
            "<path d=\"M 5 10 L 25 10 L 25 25 L 5 25 Z\" fill-rule=\"nonzero\" fill=\"#ff0080\" fill-opacity=\"0.502\" stroke=\"none\" opacity=\"1\" />\n",
            "</svg>\n"
        ));
        assert_eq!(exportar_svg(&c).unwrap().as_bytes(), svg.as_bytes());
    }
    #[test]
    fn gradiente_dash_clip_e_z_order() {
        let mut c = Cena::nova(80., 60.);
        let grad = Preenchimento::Linear {
            de: Ponto::novo(0., 0.),
            ate: Ponto::novo(80., 0.),
            paradas: vec![
                Parada {
                    posicao: 0.,
                    cor: Cor::rgb(255, 0, 0),
                },
                Parada {
                    posicao: 1.,
                    cor: Cor::rgba(0, 0, 255, 128),
                },
            ],
        };
        let mut s = Stroke::novo(Cor::rgb(0, 0, 0), 3.);
        s.dash = vec![4., 2.];
        c.adicionar(Figura::Retangulo {
            area: Retangulo::novo(0., 0., 80., 60.),
            estilo: Estilo::default(),
        });
        let fig = Figura::Elipse {
            centro: Ponto::novo(40., 30.),
            raio_x: 20.,
            raio_y: 10.,
            estilo: EstiloVetor {
                preenchimento: Some(grad),
                stroke: Some(s),
                opacidade: 0.75,
                texto: None,
            },
        };
        let mut clip = Caminho::novo();
        clip.comandos = vec![
            Comando::Mover(Ponto::novo(0., 0.)),
            Comando::Linha(Ponto::novo(40., 0.)),
            Comando::Linha(Ponto::novo(40., 50.)),
            Comando::Fechar,
        ];
        c.adicionar(Figura::Recorte {
            figura: Box::new(fig),
            recorte: clip,
        });
        let svg = exportar_svg(&c).unwrap();
        assert!(svg.contains("<clipPath id=\"clip0\""));
        assert!(svg.contains("<linearGradient id=\"grad1\""));
        assert!(svg.contains("fill=\"url(#grad1)\""));
        assert!(svg.contains("stroke-dasharray=\"4 2\""));
        assert!(svg.contains("opacity=\"0.75\""));
        assert_eq!(svg.matches("<path d=").count(), 3);
        assert_eq!(svg.matches("</g>").count(), 1);
        assert_eq!(svg, exportar_svg(&c).unwrap());
    }
    #[test]
    fn seta_preenchida_exporta_silhueta_com_haste_e_seta_stroke_e_aberta() {
        let mut c = Cena::nova(60., 30.);
        c.adicionar(Figura::Seta {
            de: Ponto::novo(0., 10.),
            ate: Ponto::novo(40., 10.),
            tamanho_ponta: 8.,
            estilo: EstiloVetor::default(),
        });
        // Ponta de 8 unidades em uma seta de 40: base x = 40 - 8 = 32;
        // abertura = 8 * 0.45 = 3.6; meia haste = 8 * 0.10 = 0.8.
        let preenchida = exportar_svg(&c).unwrap();
        assert!(preenchida.contains(
            "M 0 10.8 L 32 10.8 L 32 13.6 L 40 10 L 32 6.4 L 32 9.2 L 0 9.2 Z"
        ));
        c.figuras[0] = Figura::Seta {
            de: Ponto::novo(0., 10.),
            ate: Ponto::novo(40., 10.),
            tamanho_ponta: 8.,
            estilo: EstiloVetor {
                preenchimento: None,
                stroke: Some(Stroke::novo(Cor::rgb(0, 0, 0), 2.)),
                ..Default::default()
            },
        };
        let somente_stroke = exportar_svg(&c).unwrap();
        assert!(somente_stroke.contains(
            "M 0 10 L 40 10 M 32 13.6 L 40 10 L 32 6.4"
        ));
        assert!(!somente_stroke.contains("L 32 6.4 Z"));
    }

    #[test]
    fn geometria_mista_clipping_composto_e_reordenacao_svg() {
        let mut c = Cena::nova(100., 80.);
        let seta = Figura::Seta {
            de: Ponto::novo(10., 40.),
            ate: Ponto::novo(90., 40.),
            tamanho_ponta: 10.,
            estilo: EstiloVetor::default(),
        };
        let curva = Figura::Bezier {
            inicio: Ponto::novo(10., 20.),
            controle1: Ponto::novo(30., 60.),
            controle2: Ponto::novo(70., 60.),
            fim: Ponto::novo(90., 20.),
            estilo: EstiloVetor {
                preenchimento: None,
                stroke: Some(Stroke::novo(Cor::rgb(255, 0, 0), 2.)),
                ..Default::default()
            },
        };
        let mut clip = Caminho::novo();
        clip.comandos.extend([
            Comando::Mover(Ponto::novo(0., 0.)),
            Comando::Linha(Ponto::novo(100., 0.)),
            Comando::Linha(Ponto::novo(100., 80.)),
            Comando::Linha(Ponto::novo(0., 80.)),
            Comando::Fechar,
        ]);
        c.adicionar(seta);
        c.adicionar(Figura::Recorte {
            figura: Box::new(curva),
            recorte: clip,
        });
        let antes = exportar_svg(&c).unwrap();
        assert!(antes.contains("<clipPath id=\"clip0\""));
        let inicio_seta = antes.find("M 10 41").unwrap();
        let inicio_curva = antes.find("M 10 20").unwrap();
        assert!(inicio_seta < inicio_curva);

        c.ordem(0, 1).unwrap();
        let depois = exportar_svg(&c).unwrap();
        assert!(depois.find("M 10 41").unwrap() > depois.find("M 10 20").unwrap());
        assert!(depois.contains("clip-rule=\"nonzero\""));
        assert_eq!(depois, exportar_svg(&c).unwrap());
    }

    #[test]
    fn segmentos_apos_fechar_preservam_subcaminho_svg() {
        let mut caminho = Caminho::novo();
        caminho.comandos = vec![
            Comando::Mover(Ponto::novo(1., 2.)),
            Comando::Linha(Ponto::novo(3., 4.)),
            Comando::Fechar,
            Comando::Linha(Ponto::novo(5., 6.)),
            Comando::Fechar,
        ];
        assert_eq!(d(&caminho).unwrap(), "M 1 2 L 3 4 Z L 5 6 Z");
    }
    #[test]
    fn exporta_duas_paradas_na_mesma_posicao_stroke_e_opacidade() {
        let mut c = Cena::nova(100.0, 70.0);
        let mut stroke = Stroke::novo(Cor::rgba(0, 0, 0, 128), 2.0);
        stroke.dash = vec![4.0, 2.0];
        stroke.extremo = Extremo::Redondo;
        let g = Preenchimento::Linear {
            de: Ponto::novo(0.0, 0.0),
            ate: Ponto::novo(100.0, 0.0),
            paradas: vec![
                Parada { posicao: 0.0, cor: Cor::rgba(255, 0, 0, 200) },
                Parada { posicao: 0.5, cor: Cor::rgba(255, 0, 0, 200) },
                Parada { posicao: 0.5, cor: Cor::rgba(0, 0, 255, 200) },
                Parada { posicao: 1.0, cor: Cor::rgba(0, 0, 255, 200) },
            ],
        };
        c.adicionar(Figura::Elipse {
            centro: Ponto::novo(50.0, 35.0),
            raio_x: 35.0,
            raio_y: 25.0,
            estilo: EstiloVetor {
                preenchimento: Some(g),
                stroke: Some(stroke),
                opacidade: 0.75,
                texto: None,
            },
        });
        let svg = exportar_svg(&c).unwrap();
        assert_eq!(svg.matches(r#"stop offset="0.5""#).count(), 2);
        assert!(svg.contains(r##"stop-color="#ff0000""##));
        assert!(svg.contains(r##"stop-color="#0000ff""##));
        assert!(svg.contains(r#"stroke-dasharray="4 2""#));
        assert!(svg.contains(r#"stroke-linecap="round""#));
        assert!(svg.contains(r#"opacity="0.75""#));
        assert_eq!(svg, exportar_svg(&c).unwrap());
    }

    #[test]
    fn preserva_curva_transformada_e_rejeita_nan() {
        let figura = Figura::Bezier {
            inicio: Ponto::novo(0., 0.),
            controle1: Ponto::novo(2., 3.),
            controle2: Ponto::novo(4., 3.),
            fim: Ponto::novo(6., 0.),
            estilo: EstiloVetor::default(),
        };
        let mut c = Cena::nova(20., 20.);
        c.adicionar(transformar(&figura, Transformacao::transladar(5., 4.)));
        assert!(exportar_svg(&c).unwrap().contains("C 7 7 9 7 11 4"));
        c.largura = f32::NAN;
        assert_eq!(exportar_svg(&c), Err(ErroSvg::DimensaoInvalida));
    }
}

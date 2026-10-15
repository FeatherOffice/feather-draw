//! Caminhos vetoriais, curvas de Bézier e aproximação controlada para
//! hit-test e geometrias dos renderizadores. Coordenadas na unidade da Cena.
use crate::{Ponto, Transformacao};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegraPreenchimento {
    ParImpar,
    NaoZero,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comando {
    Mover(Ponto),
    Linha(Ponto),
    Quadratica {
        controle: Ponto,
        fim: Ponto,
    },
    Cubica {
        controle1: Ponto,
        controle2: Ponto,
        fim: Ponto,
    },
    Fechar,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Caminho {
    pub comandos: Vec<Comando>,
    pub regra: RegraPreenchimento,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Subcaminho {
    pub pontos: Vec<Ponto>,
    pub fechado: bool,
}

impl Caminho {
    pub fn novo() -> Self {
        Self {
            comandos: Vec::new(),
            regra: RegraPreenchimento::NaoZero,
        }
    }
    pub fn adicionar(&mut self, comando: Comando) {
        self.comandos.push(comando);
    }
    pub fn transformar(&self, t: Transformacao) -> Self {
        let comandos = self
            .comandos
            .iter()
            .map(|c| match *c {
                Comando::Mover(p) => Comando::Mover(t.aplicar(p)),
                Comando::Linha(p) => Comando::Linha(t.aplicar(p)),
                Comando::Quadratica { controle, fim } => Comando::Quadratica {
                    controle: t.aplicar(controle),
                    fim: t.aplicar(fim),
                },
                Comando::Cubica {
                    controle1,
                    controle2,
                    fim,
                } => Comando::Cubica {
                    controle1: t.aplicar(controle1),
                    controle2: t.aplicar(controle2),
                    fim: t.aplicar(fim),
                },
                Comando::Fechar => Comando::Fechar,
            })
            .collect();
        Self {
            comandos,
            regra: self.regra,
        }
    }
    /// Erro máximo aproximado no espaço geométrico, com subdivisão limitada
    /// a 14 níveis por segmento. Preserva retornos e laços colineares.
    pub fn aproximar(&self, tolerancia: f32) -> Vec<Subcaminho> {
        let tolerancia = if tolerancia.is_finite() && tolerancia > 0.0 {
            tolerancia
        } else {
            0.25
        };
        let mut saida = Vec::new();
        let mut atual = Vec::<Ponto>::new();
        let mut cursor = None;
        let mut inicio = None;
        fn finalizar(saida: &mut Vec<Subcaminho>, atual: &mut Vec<Ponto>, fechado: bool) {
            if !atual.is_empty() {
                saida.push(Subcaminho {
                    pontos: std::mem::take(atual),
                    fechado,
                });
            }
        }
        for comando in &self.comandos {
            match *comando {
                Comando::Mover(p) => {
                    finalizar(&mut saida, &mut atual, false);
                    atual.push(p);
                    cursor = Some(p);
                    inicio = Some(p);
                }
                Comando::Linha(p) => {
                    if let Some(a) = cursor {
                        // Depois de Fechar, o ponto atual volta ao início do
                        // subcaminho. Um próximo segmento começa ali (SVG).
                        if atual.is_empty() {
                            atual.push(a);
                            inicio = Some(a);
                        }
                        atual.push(p);
                        cursor = Some(p);
                    }
                }
                Comando::Quadratica { controle, fim } => {
                    if let Some(a) = cursor {
                        if atual.is_empty() {
                            atual.push(a);
                            inicio = Some(a);
                        }
                        let c1 = interpolar(a, controle, 2.0 / 3.0);
                        let c2 = interpolar(fim, controle, 2.0 / 3.0);
                        subdividir(a, c1, c2, fim, tolerancia, 0, &mut atual);
                        cursor = Some(fim);
                    }
                }
                Comando::Cubica {
                    controle1,
                    controle2,
                    fim,
                } => {
                    if let Some(a) = cursor {
                        if atual.is_empty() {
                            atual.push(a);
                            inicio = Some(a);
                        }
                        subdividir(a, controle1, controle2, fim, tolerancia, 0, &mut atual);
                        cursor = Some(fim);
                    }
                }
                Comando::Fechar => {
                    if inicio.is_some() {
                        finalizar(&mut saida, &mut atual, true);
                    }
                    cursor = inicio;
                    inicio = None;
                }
            }
        }
        finalizar(&mut saida, &mut atual, false);
        saida
    }
}
impl Default for Caminho {
    fn default() -> Self {
        Self::novo()
    }
}

fn interpolar(a: Ponto, b: Ponto, t: f32) -> Ponto {
    Ponto::novo(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
fn distancia_linha(p: Ponto, a: Ponto, b: Ponto) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let den = dx.hypot(dy);
    if den < 1e-9 {
        return (p.x - a.x).hypot(p.y - a.y);
    }
    ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / den
}
fn subdividir(a: Ponto, b: Ponto, c: Ponto, d: Ponto, tol: f32, n: u32, out: &mut Vec<Ponto>) {
    // A distância à reta suporte isoladamente NÃO é critério suficiente:
    // controles colineares fora do segmento podem produzir um laço Bézier
    // grande, apesar de ambos terem distância zero da linha. O teste da
    // caixa garante que a curva (dentro do convex hull dos controles) não
    // extrapola a corda por mais que a tolerância.
    let minx = a.x.min(d.x) - tol;
    let maxx = a.x.max(d.x) + tol;
    let miny = a.y.min(d.y) - tol;
    let maxy = a.y.max(d.y) + tol;
    let controles_contidos = [b, c].iter().all(|p| {
        p.x >= minx && p.x <= maxx && p.y >= miny && p.y <= maxy
    });
    let plano = distancia_linha(b, a, d) <= tol
        && distancia_linha(c, a, d) <= tol
        && controles_contidos;
    if n >= 14 || plano {
        out.push(d);
        return;
    }
    let (ab, bc, cd) = (
        interpolar(a, b, 0.5),
        interpolar(b, c, 0.5),
        interpolar(c, d, 0.5),
    );
    let (abc, bcd) = (interpolar(ab, bc, 0.5), interpolar(bc, cd, 0.5));
    let meio = interpolar(abc, bcd, 0.5);
    subdividir(a, ab, abc, meio, tol, n + 1, out);
    subdividir(meio, bcd, cd, d, tol, n + 1, out);
}

/// Constrói uma elipse ou arco com subdivisão por erro de flecha.
/// Ângulo em radianos, varredura assinada (horário no sistema y-para-baixo).
pub fn arco_eliptico(
    centro: Ponto,
    rx: f32,
    ry: f32,
    inicio: f32,
    varredura: f32,
    tolerancia: f32,
) -> Caminho {
    let mut caminho = Caminho::novo();
    if ![centro.x, centro.y, rx, ry, inicio, varredura]
        .iter()
        .all(|n| n.is_finite())
        || rx <= 0.
        || ry <= 0.
        || varredura == 0.
    {
        return caminho;
    }
    let tolerancia = if tolerancia.is_finite() && tolerancia > 0. {
        tolerancia
    } else {
        0.25
    };
    let r = rx.max(ry);
    let angulo = if tolerancia >= r {
        std::f32::consts::FRAC_PI_2
    } else {
        (2.0 * (1.0 - tolerancia / r).acos()).clamp(0.01, std::f32::consts::FRAC_PI_2)
    };
    let passos = ((varredura.abs() / angulo).ceil() as usize).clamp(2, 4096);
    for i in 0..=passos {
        let a = inicio + varredura * (i as f32 / passos as f32);
        let p = Ponto::novo(centro.x + rx * a.cos(), centro.y + ry * a.sin());
        caminho.adicionar(if i == 0 {
            Comando::Mover(p)
        } else {
            Comando::Linha(p)
        });
    }
    caminho
}
#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn cubica_conserva_extremos_e_curvatura() {
        let mut c = Caminho::novo();
        c.adicionar(Comando::Mover(Ponto::novo(0., 0.)));
        c.adicionar(Comando::Cubica {
            controle1: Ponto::novo(0., 100.),
            controle2: Ponto::novo(100., 100.),
            fim: Ponto::novo(100., 0.),
        });
        let t = c.aproximar(0.1);
        assert!(t[0].pontos.len() > 10);
        assert_eq!(t[0].pontos.last(), Some(&Ponto::novo(100., 0.)));
        assert!(t[0].pontos.iter().any(|p| p.y > 70.));
    }
    #[test]
    fn cubica_colinear_com_retorno_nao_perde_laco_ou_limites() {
        // Todos os controles estão sobre y=0. A antiga checagem de
        // distância à reta reduzia incorretamente a curva a 0 -> 10.
        let mut c = Caminho::novo();
        c.adicionar(Comando::Mover(Ponto::novo(0., 0.)));
        c.adicionar(Comando::Cubica {
            controle1: Ponto::novo(100., 0.),
            controle2: Ponto::novo(-100., 0.),
            fim: Ponto::novo(10., 0.),
        });
        let p = c.aproximar(0.1);
        assert_eq!(p.len(), 1);
        assert!(p[0].pontos.len() > 4);
        assert!(p[0].pontos.iter().any(|p| p.x > 25.));
        assert!(p[0].pontos.iter().any(|p| p.x < -20.));
        assert_eq!(p[0].pontos.first(), Some(&Ponto::novo(0., 0.)));
        assert_eq!(p[0].pontos.last(), Some(&Ponto::novo(10., 0.)));
    }

    #[test]
    fn multiplos_subcaminhos_e_fechar() {
        let mut c = Caminho::novo();
        for i in 0..2 {
            c.adicionar(Comando::Mover(Ponto::novo(i as f32 * 10., 0.)));
            c.adicionar(Comando::Linha(Ponto::novo(i as f32 * 10. + 5., 0.)));
            c.adicionar(Comando::Fechar);
        }
        let v = c.aproximar(0.1);
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|s| s.fechado));
    }
    #[test]
    fn segmentos_apos_fechar_recomecam_na_origem_e_podem_fechar_novamente() {
        let mut c = Caminho::novo();
        let origem = Ponto::novo(2., 3.);
        c.adicionar(Comando::Mover(origem));
        c.adicionar(Comando::Linha(Ponto::novo(10., 3.)));
        c.adicionar(Comando::Fechar);
        c.adicionar(Comando::Linha(Ponto::novo(8., 8.)));
        c.adicionar(Comando::Fechar);
        let partes = c.aproximar(0.1);
        assert_eq!(partes.len(), 2);
        assert!(partes.iter().all(|p| p.fechado));
        assert_eq!(partes[1].pontos, vec![origem, Ponto::novo(8., 8.)]);
    }

    #[test]
    fn bezier_apos_fechar_reinicia_cursor_no_inicio() {
        let mut c = Caminho::novo();
        let origem = Ponto::novo(1., 2.);
        c.adicionar(Comando::Mover(origem));
        c.adicionar(Comando::Linha(Ponto::novo(10., 2.)));
        c.adicionar(Comando::Fechar);
        c.adicionar(Comando::Quadratica {
            controle: Ponto::novo(4., 8.),
            fim: Ponto::novo(9., 2.),
        });
        let partes = c.aproximar(0.1);
        assert_eq!(partes.len(), 2);
        assert_eq!(partes[1].pontos.first(), Some(&origem));
        assert_eq!(partes[1].pontos.last(), Some(&Ponto::novo(9., 2.)));
    }

    #[test]
    fn elipse_gera_caminho_no_raio() {
        let c = arco_eliptico(
            Ponto::novo(0., 0.),
            20.,
            10.,
            0.,
            std::f32::consts::TAU,
            0.1,
        );
        let v = c.aproximar(0.1);
        assert!(v[0].pontos.len() > 10);
        assert!((v[0].pontos[0].x - 20.).abs() < 1e-4);
    }
}

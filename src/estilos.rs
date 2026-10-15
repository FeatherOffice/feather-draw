//! Aparência vetorial neutra, sem dependência de Iced, SVG ou PDF.
//! O `Estilo` legado continua válido; `EstiloVetor::from` cria a versão
//! avançada para formas novas, sem alterar os contratos de Charts.
use crate::Cor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extremo {
    Plano,
    Redondo,
    Quadrado,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Juncao {
    Miter,
    Redonda,
    Bevel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub cor: Cor,
    pub largura: f32,
    pub extremo: Extremo,
    pub juncao: Juncao,
    pub limite_miter: f32,
    /// Valores alternados de tinta/espaço, unidades da cena. Vazio = sólido.
    pub dash: Vec<f32>,
    pub fase: f32,
}
impl Stroke {
    pub fn novo(cor: Cor, largura: f32) -> Self {
        Self {
            cor,
            largura,
            extremo: Extremo::Plano,
            juncao: Juncao::Miter,
            limite_miter: 4.0,
            dash: Vec::new(),
            fase: 0.0,
        }
    }
    pub fn valido(&self) -> bool {
        self.largura.is_finite()
            && self.largura >= 0.0
            && self.limite_miter.is_finite()
            && self.limite_miter >= 1.0
            && self.fase.is_finite()
            && self.dash.iter().all(|x| x.is_finite() && *x > 0.0)
    }
    /// Se o comprimento percorrido corresponde à região pintada do traço.
    /// Ciclo ímpar repete duas vezes, como SVG/CSS.
    pub fn visivel(&self, distancia: f32) -> bool {
        // O padrão usa f64 internamente: a soma de dashs f32 finitos pode
        // ultrapassar f32::MAX sem que o padrão seja semanticamente inválido.
        if !self.valido() || !distancia.is_finite() {
            return false;
        }
        if self.dash.is_empty() {
            return true;
        }
        let ciclo: f64 = self.dash.iter().map(|&n| f64::from(n)).sum();
        let total = if self.dash.len() % 2 == 0 {
            ciclo
        } else {
            2.0 * ciclo
        };
        let mut t = (f64::from(distancia) + f64::from(self.fase)).rem_euclid(total);
        for i in 0..(self.dash.len() * if self.dash.len() % 2 == 0 { 1 } else { 2 }) {
            let comprimento = f64::from(self.dash[i % self.dash.len()]);
            if t < comprimento {
                return i % 2 == 0;
            }
            t -= comprimento;
        }
        // Evita pintar segmentos inexistentes em caso de arredondamento.
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parada {
    pub posicao: f32,
    pub cor: Cor,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Preenchimento {
    Solido(Cor),
    /// Fração t=0 no ponto inicial, t=1 no final. Paradas ordenadas em `[0,1]`.
    Linear {
        de: crate::Ponto,
        ate: crate::Ponto,
        paradas: Vec<Parada>,
    },
    /// Distância radial normalizada de 0 a 1.
    Radial {
        centro: crate::Ponto,
        raio: f32,
        paradas: Vec<Parada>,
    },
}
impl Preenchimento {
    pub fn valido(&self) -> bool {
        fn paradas_ok(p: &[Parada]) -> bool {
            p.len() >= 2
                && p.first().is_some_and(|x| x.posicao == 0.0)
                && p.last().is_some_and(|x| x.posicao == 1.0)
                && p.windows(2).all(|w| {
                    w[0].posicao.is_finite()
                        && w[1].posicao.is_finite()
                        && w[0].posicao <= w[1].posicao
                })
        }
        match self {
            Self::Solido(_) => true,
            Self::Linear { de, ate, paradas } => {
                [de.x, de.y, ate.x, ate.y].iter().all(|x| x.is_finite())
                    && (*de != *ate)
                    && paradas_ok(paradas)
            }
            Self::Radial {
                centro,
                raio,
                paradas,
            } => {
                centro.x.is_finite()
                    && centro.y.is_finite()
                    && raio.is_finite()
                    && *raio > 0.0
                    && paradas_ok(paradas)
            }
        }
    }
    /// Amostragem de gradiente em posição normalizada `[0,1]`.
    /// A última parada em posições duplicadas define a cor da borda dura.
    /// Usa o mesmo algoritmo para hit-test, SVG (amostra de teste) e PDF.
    pub fn amostrar_normalizada(&self, t: f32) -> Cor {
        fn mistura(a: Cor, b: Cor, t: f32) -> Cor {
            let ch = |x: u8, y: u8| {
                ((x as f32 + (y as f32 - x as f32) * t)
                    .round()
                    .clamp(0.0, 255.0)) as u8
            };
            Cor::rgba(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b), ch(a.a, b.a))
        }

        let paradas = match self {
            Self::Solido(c) => return *c,
            Self::Linear { paradas, .. } | Self::Radial { paradas, .. } => paradas,
        };
        let Some(first) = paradas.first() else {
            return Cor::rgba(0, 0, 0, 0);
        };
        if !t.is_finite() {
            return first.cor;
        }
        let t = t.clamp(0.0, 1.0);
        let idx = paradas.partition_point(|p| p.posicao <= t);
        if idx == 0 {
            return first.cor;
        }
        if idx == paradas.len() {
            return paradas.last().expect("paradas não vazias").cor;
        }
        let a = &paradas[idx - 1];
        let b = &paradas[idx];
        let intervalo = b.posicao - a.posicao;
        if intervalo <= 0.0 {
            return b.cor;
        }
        mistura(a.cor, b.cor, (t - a.posicao) / intervalo)
    }

    /// Amostragem da cor RGBA em coordenadas da Cena.
    /// Intermediários em f64 preservam extremos de f32 sem overflow.
    pub fn amostrar(&self, p: crate::Ponto) -> Cor {
        match self {
            Self::Solido(c) => *c,
            Self::Linear { de, ate, .. } => {
                let (dx, dy) = (
                    f64::from(ate.x) - f64::from(de.x),
                    f64::from(ate.y) - f64::from(de.y),
                );
                let (px, py) = (
                    f64::from(p.x) - f64::from(de.x),
                    f64::from(p.y) - f64::from(de.y),
                );
                let denominador = dx * dx + dy * dy;
                let t = if denominador > 0.0 {
                    ((px * dx + py * dy) / denominador).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };
                self.amostrar_normalizada(t)
            }
            Self::Radial { centro, raio, .. } => {
                let dx = f64::from(p.x) - f64::from(centro.x);
                let dy = f64::from(p.y) - f64::from(centro.y);
                let t = (dx.hypot(dy) / f64::from(*raio))
                    .clamp(0.0, 1.0) as f32;
                self.amostrar_normalizada(t)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlinhamentoTexto {
    Inicio,
    Centro,
    Fim,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EstiloTexto {
    pub familia: String,
    pub tamanho: f32,
    pub peso: u16,
    pub italico: bool,
    pub sublinhado: bool,
    pub alinhamento: AlinhamentoTexto,
    pub entrelinhas: f32,
}
impl Default for EstiloTexto {
    fn default() -> Self {
        Self {
            familia: "sans-serif".into(),
            tamanho: 12.0,
            peso: 400,
            italico: false,
            sublinhado: false,
            alinhamento: AlinhamentoTexto::Inicio,
            entrelinhas: 1.2,
        }
    }
}
impl EstiloTexto {
    pub fn valido(&self) -> bool {
        self.tamanho.is_finite()
            && self.tamanho > 0.
            && self.entrelinhas.is_finite()
            && self.entrelinhas > 0.
            && (1..=1000).contains(&self.peso)
            && !self.familia.trim().is_empty()
            && !self.familia.chars().any(char::is_control)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EstiloVetor {
    pub preenchimento: Option<Preenchimento>,
    pub stroke: Option<Stroke>,
    /// Opacidade adicional de todo o objeto em `[0,1]`, multiplicada pelo alfa das cores.
    pub opacidade: f32,
    pub texto: Option<EstiloTexto>,
}
impl Default for EstiloVetor {
    fn default() -> Self {
        Self {
            preenchimento: Some(Preenchimento::Solido(Cor::rgb(0, 0, 0))),
            stroke: None,
            opacidade: 1.0,
            texto: None,
        }
    }
}
impl EstiloVetor {
    pub fn valido(&self) -> bool {
        self.opacidade.is_finite()
            && (0.0..=1.0).contains(&self.opacidade)
            && self.stroke.as_ref().map_or(true, Stroke::valido)
            && self
                .preenchimento
                .as_ref()
                .map_or(true, Preenchimento::valido)
            && self.texto.as_ref().map_or(true, EstiloTexto::valido)
    }
    pub fn alfa(&self, cor: Cor) -> u8 {
        ((cor.a as f32 * self.opacidade).round().clamp(0.0, 255.0)) as u8
    }
}

impl From<&crate::Estilo> for EstiloVetor {
    fn from(v: &crate::Estilo) -> Self {
        Self {
            preenchimento: v.preenchimento.map(Preenchimento::Solido),
            stroke: v.contorno.map(|c| Stroke::novo(c, v.espessura)),
            opacidade: 1.0,
            texto: None,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn gradiente_alpha_e_paradas() {
        let preto = Cor::rgba(0, 0, 0, 100);
        let branco = Cor::rgba(255, 255, 255, 200);
        let g = Preenchimento::Linear {
            de: crate::Ponto::novo(0., 0.),
            ate: crate::Ponto::novo(10., 0.),
            paradas: vec![
                Parada {
                    posicao: 0.,
                    cor: preto,
                },
                Parada {
                    posicao: 1.,
                    cor: branco,
                },
            ],
        };
        assert!(g.valido());
        assert_eq!(
            g.amostrar(crate::Ponto::novo(5., 0.)),
            Cor::rgba(128, 128, 128, 150)
        );
        let e = EstiloVetor {
            preenchimento: Some(g),
            opacidade: 0.5,
            ..Default::default()
        };
        assert!(e.valido());
        assert_eq!(e.alfa(branco), 100);
    }
    #[test]
    fn stroke_dash_repetido() {
        let mut s = Stroke::novo(Cor::rgb(0, 0, 0), 2.0);
        s.dash = vec![3., 2., 1.];
        assert!(s.valido());
        assert!(s.visivel(2.));
        assert!(!s.visivel(4.));
        assert!(!s.visivel(7.));
        assert!(s.visivel(12.));
    }
    #[test]
    fn dash_grande_e_fase_negativa_sem_overflow() {
        let mut s = Stroke::novo(Cor::rgb(1, 2, 3), 2.0);
        s.dash = vec![f32::MAX, f32::MAX];
        assert!(s.valido());
        assert!(s.visivel(1.0));
        assert!(!s.visivel(-f32::MAX)); // segunda metade do ciclo
        s.dash = vec![2.0, 1.0, 3.0];
        s.fase = -1.0;
        assert!(s.valido());
        assert!(!s.visivel(0.0));
        assert!(s.visivel(1.0));
        assert!(!s.visivel(f32::NAN));
        s.dash = vec![1.0, 0.0];
        assert!(!s.valido());
        assert!(!s.visivel(0.0));
    }

    #[test]
    fn gradiente_colinear_extremo_nao_perde_precisao() {
        let preto = Cor::rgba(0, 0, 0, 0);
        let branco = Cor::rgba(255, 255, 255, 255);
        let linear = Preenchimento::Linear {
            de: crate::Ponto::novo(-f32::MAX, 0.0),
            ate: crate::Ponto::novo(f32::MAX, 0.0),
            paradas: vec![
                Parada { posicao: 0.0, cor: preto },
                Parada { posicao: 1.0, cor: branco },
            ],
        };
        assert!(linear.valido());
        assert_eq!(linear.amostrar(crate::Ponto::novo(0.0, 0.0)),
            Cor::rgba(128, 128, 128, 128));
        assert_eq!(linear.amostrar(crate::Ponto::novo(-f32::MAX, 0.0)), preto);
        assert_eq!(linear.amostrar(crate::Ponto::novo(f32::MAX, 0.0)), branco);
    }

    #[test]
    fn gradiente_paradas_duplicadas_formam_borda_dura() {
        let vermelho = Cor::rgba(255, 0, 0, 50);
        let azul = Cor::rgba(0, 0, 255, 200);
        let gradiente = Preenchimento::Linear {
            de: crate::Ponto::novo(0.0, 0.0),
            ate: crate::Ponto::novo(10.0, 0.0),
            paradas: vec![
                Parada { posicao: 0.0, cor: vermelho },
                Parada { posicao: 0.5, cor: vermelho },
                Parada { posicao: 0.5, cor: azul },
                Parada { posicao: 1.0, cor: azul },
            ],
        };
        assert!(gradiente.valido());
        assert_eq!(gradiente.amostrar_normalizada(0.49), vermelho);
        assert_eq!(gradiente.amostrar_normalizada(0.5), azul);
        assert_eq!(gradiente.amostrar_normalizada(0.51), azul);
        assert_eq!(gradiente.amostrar(crate::Ponto::novo(5.0, 0.0)), azul);
        assert_eq!(gradiente.amostrar_normalizada(f32::NAN), vermelho);
    }

    #[test]
    fn gradiente_radial_distancias_extremas() {
        let a = Cor::rgb(0, 10, 20);
        let b = Cor::rgb(255, 100, 200);
        let g = Preenchimento::Radial {
            centro: crate::Ponto::novo(-f32::MAX, 0.0),
            raio: f32::MAX,
            paradas: vec![
                Parada { posicao: 0.0, cor: a },
                Parada { posicao: 1.0, cor: b },
            ],
        };
        assert!(g.valido());
        assert_eq!(g.amostrar(crate::Ponto::novo(-f32::MAX, 0.0)), a);
        assert_eq!(g.amostrar(crate::Ponto::novo(f32::MAX, 0.0)), b);
    }

    #[test]
    fn textos_e_tracos_invalidos_nao_entram_no_modelo() {
        let mut texto = EstiloTexto::default();
        assert!(texto.valido());
        texto.familia = "  ".into();
        assert!(!texto.valido());
        texto.familia = format!("Fonte{}Invalida", char::from(10));
        assert!(!texto.valido());
        texto.familia = "Inter".into();
        texto.peso = 0;
        assert!(!texto.valido());

        let mut estilo = EstiloVetor::default();
        estilo.texto = Some(texto);
        assert!(!estilo.valido());
        estilo.texto = None;
        estilo.opacidade = f32::NAN;
        assert!(!estilo.valido());
        estilo.opacidade = 0.5;
        let mut traco = Stroke::novo(Cor::rgb(1, 2, 3), 1.0);
        traco.limite_miter = 0.5;
        estilo.stroke = Some(traco);
        assert!(!estilo.valido());
    }

    #[test]
    fn legado_preservado() {
        let l = crate::Estilo {
            preenchimento: None,
            contorno: Some(Cor::rgb(1, 2, 3)),
            espessura: 5.,
        };
        let e = EstiloVetor::from(&l);
        assert_eq!(e.stroke.unwrap().largura, 5.);
    }
}

//! Formato de texto da tinta (o crate não usa serde). Uma linha por traço:
//!
//! ```text
//! FTINTA 1
//! T <id> <c|m> <RRGGBBAA> <espessura> <opacidade> <r|s> <x,y[,p][,t]> <x,y…> …
//! ```
//!
//! `c` = caneta, `m` = marca-texto; `r` = segmentos retos, `s` = suave.
//! Logo depois do modo pode vir `v`: a pressão dos pontos foi estimada
//! pela velocidade (sem mesa), não medida pela caneta.
//! Cada ponto é `x,y`, com pressão `p` (0–1) e tempo `t` (ms) opcionais:
//! `x,y,p`, `x,y,p,t` ou `x,y,,t` (sem pressão). Números com ponto decimal:
//! coordenadas com 2 casas, espessura, opacidade e pressão com 3. Linhas
//! vazias e linhas começando por `#` são ignoradas.
//! Versão desconhecida é recusada (nunca ler pela metade).
use super::{Ferramenta, PontoTinta, Tinta, Traco};
use crate::Cor;
use std::fmt::Write as _;

pub const CABECALHO: &str = "FTINTA 1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroTinta {
    /// Cabeçalho ausente ou de versão desconhecida.
    Cabecalho(String),
    /// Linha `n` (contando de 1) mal formada.
    Linha(usize, String),
    IdRepetido(u64),
    TracoInvalido(u64),
}

impl std::fmt::Display for ErroTinta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cabecalho(c) => write!(f, "cabeçalho de tinta desconhecido: {c:?}"),
            Self::Linha(n, m) => write!(f, "linha {n} da tinta: {m}"),
            Self::IdRepetido(id) => write!(f, "traço com id repetido ou zero: {id}"),
            Self::TracoInvalido(id) => write!(f, "traço {id} inválido"),
        }
    }
}

impl std::error::Error for ErroTinta {}

fn numero(saida: &mut String, v: f32, casas: usize) {
    let mut s = format!("{v:.casas$}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".into();
    }
    saida.push_str(&s);
}

impl Traco {
    /// Uma linha `T …` (sem quebra de linha no fim).
    pub fn para_linha(&self) -> String {
        let mut s = String::with_capacity(32 + self.pontos.len() * 14);
        let _ = write!(
            s,
            "T {} {} {:02X}{:02X}{:02X}{:02X} ",
            self.id,
            match self.ferramenta {
                Ferramenta::Caneta => 'c',
                Ferramenta::MarcaTexto => 'm',
            },
            self.cor.r,
            self.cor.g,
            self.cor.b,
            self.cor.a
        );
        numero(&mut s, self.espessura, 3);
        s.push(' ');
        numero(&mut s, self.opacidade, 3);
        s.push_str(if self.reto { " r" } else { " s" });
        if self.pressao_sintetica {
            s.push_str(" v");
        }
        for p in &self.pontos {
            s.push(' ');
            numero(&mut s, p.x, 2);
            s.push(',');
            numero(&mut s, p.y, 2);
            match (p.pressao, p.tempo) {
                (None, None) => {}
                (Some(pr), None) => {
                    s.push(',');
                    numero(&mut s, pr, 3);
                }
                (pr, Some(t)) => {
                    s.push(',');
                    if let Some(pr) = pr {
                        numero(&mut s, pr, 3);
                    }
                    let _ = write!(s, ",{t}");
                }
            }
        }
        s
    }

    /// Lê uma linha `T …`.
    pub fn de_linha(linha: &str) -> Result<Self, String> {
        let mut partes = linha.split_ascii_whitespace();
        if partes.next() != Some("T") {
            return Err("esperado 'T'".into());
        }
        let mut campo = |nome: &str| partes.next().ok_or_else(|| format!("falta {nome}"));
        let id: u64 = campo("id")?.parse().map_err(|_| "id inválido".to_string())?;
        let ferramenta = match campo("ferramenta")? {
            "c" => Ferramenta::Caneta,
            "m" => Ferramenta::MarcaTexto,
            f => return Err(format!("ferramenta desconhecida {f:?}")),
        };
        let cor = cor_hex(campo("cor")?).ok_or("cor inválida")?;
        let espessura = real(campo("espessura")?)?;
        let opacidade = real(campo("opacidade")?)?;
        let reto = match campo("modo")? {
            "r" => true,
            "s" => false,
            m => return Err(format!("modo desconhecido {m:?}")),
        };
        let mut pontos = Vec::new();
        let mut pressao_sintetica = false;
        for token in partes {
            if token == "v" && pontos.is_empty() && !pressao_sintetica {
                pressao_sintetica = true;
                continue;
            }
            let mut c = token.split(',');
            let x = real(c.next().unwrap_or(""))?;
            let y = real(c.next().ok_or("ponto sem y")?)?;
            let mut p = PontoTinta::novo(x, y);
            if let Some(pr) = c.next() {
                if !pr.is_empty() {
                    p.pressao = Some(real(pr)?);
                }
            }
            if let Some(t) = c.next() {
                p.tempo = Some(t.parse().map_err(|_| format!("tempo inválido {t:?}"))?);
            }
            if c.next().is_some() {
                return Err(format!("ponto com campos demais {token:?}"));
            }
            pontos.push(p);
        }
        let traco = Traco { id, pontos, cor, espessura, ferramenta, opacidade, reto, pressao_sintetica };
        if traco.valido() {
            Ok(traco)
        } else {
            Err("traço sem pontos ou com valores fora do intervalo".into())
        }
    }
}

fn real(s: &str) -> Result<f32, String> {
    s.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("número inválido {s:?}"))
}

fn cor_hex(s: &str) -> Option<Cor> {
    if s.len() != 8 || !s.is_ascii() {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(Cor::rgba((n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8))
}

impl Tinta {
    /// Texto completo (cabeçalho + uma linha por traço, na ordem de desenho).
    pub fn para_texto(&self) -> String {
        let mut s = String::from(CABECALHO);
        for t in self.tracos() {
            s.push('\n');
            s.push_str(&t.para_linha());
        }
        s.push('\n');
        s
    }

    /// Lê o texto de [`Tinta::para_texto`]. Erro na primeira linha ruim.
    pub fn de_texto(texto: &str) -> Result<Self, ErroTinta> {
        let mut linhas = texto.lines().enumerate();
        let cab = linhas.next().map(|(_, l)| l.trim()).unwrap_or("");
        if cab != CABECALHO {
            return Err(ErroTinta::Cabecalho(cab.chars().take(40).collect()));
        }
        let mut tinta = Tinta::nova();
        for (i, linha) in linhas {
            let linha = linha.trim();
            if linha.is_empty() || linha.starts_with('#') {
                continue;
            }
            let traco = Traco::de_linha(linha).map_err(|m| ErroTinta::Linha(i + 1, m))?;
            tinta.inserir_com_id(traco)?;
        }
        Ok(tinta)
    }
}

//! Exportador PDF 1.4 vetorial e determinístico, independente de Iced.
//! Coordenadas da Cena usam origem superior esquerda e pontos PDF.
//! Fontes base Helvetica usam codificação WinAnsi; caracteres não suportados
//! causam erro explícito, nunca substituição silenciosa.
use crate::{caminhos::{Caminho, Comando, RegraPreenchimento},
    estilos::{Extremo, Juncao, Preenchimento}, Cena, Cor, Figura};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroPdf {
    DimensaoInvalida,
    GeometriaInvalida,
    EstiloInvalido,
    AlfaGradienteVariavel,
    CaractereNaoSuportado,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontePdf { Helvetica, Negrito, Italico, NegritoItalico }
impl FontePdf {
    fn indice(self) -> usize {
        match self { Self::Helvetica => 1, Self::Negrito => 2,
            Self::Italico => 3, Self::NegritoItalico => 4 }
    }
}

/// Texto adicionado após o desenho, posicionado pelo baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct TextoPdf {
    pub texto: String,
    pub x: f32,
    pub y: f32,
    pub tamanho: f32,
    pub cor: Cor,
    pub fonte: FontePdf,
}

fn num(v: f32) -> Result<String, ErroPdf> {
    if !v.is_finite() || v.abs() > 10_000_000.0 { return Err(ErroPdf::GeometriaInvalida); }
    let mut s = format!("{:.4}", if v.abs() < 0.00005 { 0.0 } else { v });
    while s.contains('.') && s.ends_with('0') { s.pop(); }
    if s.ends_with('.') { s.pop(); }
    Ok(s)
}
fn cor(c: Cor, operacao: &str) -> String {
    format!("{:.4} {:.4} {:.4} {operacao}\n",
        c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0)
}
fn path(caminho: &Caminho) -> Result<String, ErroPdf> {
    let mut out = String::new();
    let (mut atual, mut inicio) = (None, None);
    let mut iniciado = false;
    for comando in &caminho.comandos {
        match *comando {
            Comando::Mover(p) => {
                writeln!(out, "{} {} m", num(p.x)?, num(p.y)?).unwrap();
                atual = Some(p); inicio = Some(p); iniciado = true;
            }
            Comando::Linha(p) => {
                if atual.is_none() { return Err(ErroPdf::GeometriaInvalida); }
                writeln!(out, "{} {} l", num(p.x)?, num(p.y)?).unwrap();
                atual = Some(p);
            }
            Comando::Quadratica { controle, fim } => {
                let a = atual.ok_or(ErroPdf::GeometriaInvalida)?;
                let t = 2.0 / 3.0;
                let c1 = crate::Ponto::novo(a.x + (controle.x - a.x) * t,
                    a.y + (controle.y - a.y) * t);
                let c2 = crate::Ponto::novo(fim.x + (controle.x - fim.x) * t,
                    fim.y + (controle.y - fim.y) * t);
                writeln!(out, "{} {} {} {} {} {} c",
                    num(c1.x)?, num(c1.y)?, num(c2.x)?, num(c2.y)?,
                    num(fim.x)?, num(fim.y)?).unwrap();
                atual = Some(fim);
            }
            Comando::Cubica { controle1, controle2, fim } => {
                if atual.is_none() { return Err(ErroPdf::GeometriaInvalida); }
                writeln!(out, "{} {} {} {} {} {} c", num(controle1.x)?, num(controle1.y)?,
                    num(controle2.x)?, num(controle2.y)?, num(fim.x)?, num(fim.y)?).unwrap();
                atual = Some(fim);
            }
            Comando::Fechar => {
                let p = inicio.ok_or(ErroPdf::GeometriaInvalida)?;
                out.push_str("h\n");
                atual = Some(p);
                inicio = None;
            }
        }
    }
    if !iniciado { return Err(ErroPdf::GeometriaInvalida); }
    Ok(out)
}

struct Documentos { objetos: Vec<Vec<u8>> }
impl Documentos {
    fn novo() -> Self { Self { objetos: Vec::new() } }
    fn adicionar(&mut self, bytes: impl Into<Vec<u8>>) -> usize {
        self.objetos.push(bytes.into()); self.objetos.len()
    }
    fn gerar(self, root: usize) -> Vec<u8> {
        let total = self.objetos.len() + 1;
        let mut saida = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(total);
        offsets.push(0usize);
        for (i, item) in self.objetos.iter().enumerate() {
            offsets.push(saida.len());
            saida.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            saida.extend_from_slice(item);
            saida.extend_from_slice(b"\nendobj\n");
        }
        let xref = saida.len();
        saida.extend_from_slice(format!("xref\n0 {total}\n0000000000 65535 f \n").as_bytes());
        for offset in offsets.into_iter().skip(1) {
            saida.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        saida.extend_from_slice(format!(
            "trailer\n<< /Size {total} /Root {root} 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ).as_bytes());
        saida
    }
}

struct Emissor {
    doc: Documentos,
    conteudo: String,
    alfas: Vec<(u16, u16, usize)>,
    sombras: Vec<usize>,
}
impl Emissor {
    fn estado(&mut self, fill: f32, stroke: f32) -> String {
        let escala = |v: f32| (v.clamp(0.0, 1.0) * 10000.0).round() as u16;
        let (a, b) = (escala(fill), escala(stroke));
        let i = if let Some(i) = self.alfas.iter().position(|e| e.0 == a && e.1 == b) {
            i
        } else {
            let id = self.doc.adicionar(format!(
                "<< /Type /ExtGState /ca {:.4} /CA {:.4} >>",
                a as f32 / 10000.0, b as f32 / 10000.0
            ));
            self.alfas.push((a, b, id)); self.alfas.len() - 1
        };
        format!("/GS{} gs\n", i + 1)
    }
    fn sombreado(&mut self, grad: &Preenchimento) -> Result<String, ErroPdf> {
        let (tipo, coords, paradas) = match grad {
            Preenchimento::Linear { de, ate, paradas } =>
                (2, format!("{} {} {} {}", num(de.x)?, num(de.y)?,
                    num(ate.x)?, num(ate.y)?), paradas),
            Preenchimento::Radial { centro, raio, paradas } =>
                (3, format!("{} {} 0 {} {} {}", num(centro.x)?, num(centro.y)?,
                    num(centro.x)?, num(centro.y)?, num(*raio)?), paradas),
            Preenchimento::Solido(_) => return Err(ErroPdf::EstiloInvalido),
        };
        if paradas.len() < 2 { return Err(ErroPdf::EstiloInvalido); }
        if paradas.iter().any(|p| p.cor.a != paradas[0].cor.a) {
            return Err(ErroPdf::AlfaGradienteVariavel);
        }
        // Função sampled PDF Type 0 representa paradas duplicadas sem
        // criar Bounds coincidentes inválidos de uma função Type 3.
        let mut tabela = Vec::with_capacity(768);
        for i in 0..256 {
            let t = i as f32 / 255.0;
            // Mesmo algoritmo de interpolação do motor e do hit-test,
            // inclusive nas paradas duplicadas (borda dura).
            let valor = grad.amostrar_normalizada(t);
            tabela.extend_from_slice(&[valor.r, valor.g, valor.b]);
        }
        let mut funcao = format!(
            "<< /FunctionType 0 /Domain [0 1] /Range [0 1 0 1 0 1] /Size [256] /BitsPerSample 8 /Order 1 /Length {} >>\nstream\n",
            tabela.len()
        ).into_bytes();
        funcao.extend_from_slice(&tabela);
        funcao.extend_from_slice(b"\nendstream");
        let func = self.doc.adicionar(funcao);
        let sombra = self.doc.adicionar(format!(
            "<< /ShadingType {tipo} /ColorSpace /DeviceRGB /Coords [{coords}] /Function {func} 0 R /Extend [true true] >>"
        ));
        self.sombras.push(sombra);
        Ok(format!("Sh{}", self.sombras.len()))
    }
    fn figura(&mut self, figura: &Figura) -> Result<(), ErroPdf> {
        if let Figura::Recorte { figura, recorte } = figura {
            let clip = path(recorte)?;
            self.conteudo.push_str("q\n");
            self.conteudo.push_str(&clip);
            self.conteudo.push_str(match recorte.regra {
                RegraPreenchimento::NaoZero => "W n\n",
                RegraPreenchimento::ParImpar => "W* n\n",
            });
            self.figura(figura)?;
            self.conteudo.push_str("Q\n");
            return Ok(());
        }
        let caminho = figura.caminho(0.05);
        let comandos = path(&caminho)?;
        let estilo = figura.estilo_vetor().ok_or(ErroPdf::EstiloInvalido)?;
        if !estilo.valido() { return Err(ErroPdf::EstiloInvalido); }
        let a_fill = match &estilo.preenchimento {
            Some(Preenchimento::Solido(c)) => c.a,
            Some(Preenchimento::Linear { paradas, .. }) |
            Some(Preenchimento::Radial { paradas, .. }) => paradas[0].cor.a,
            None => 255,
        };
        let a_stroke = estilo.stroke.as_ref().map_or(255, |s| s.cor.a);
        self.conteudo.push_str("q\n");
        let gs = self.estado(a_fill as f32 / 255.0 * estilo.opacidade,
            a_stroke as f32 / 255.0 * estilo.opacidade);
        self.conteudo.push_str(&gs);
        if let Some(s) = &estilo.stroke {
            self.conteudo.push_str(&cor(s.cor, "RG"));
            writeln!(self.conteudo, "{} w", num(s.largura)?).unwrap();
            writeln!(self.conteudo, "{} J", match s.extremo {
                Extremo::Plano => 0, Extremo::Redondo => 1, Extremo::Quadrado => 2,
            }).unwrap();
            writeln!(self.conteudo, "{} j", match s.juncao {
                Juncao::Miter => 0, Juncao::Redonda => 1, Juncao::Bevel => 2,
            }).unwrap();
            writeln!(self.conteudo, "{} M", num(s.limite_miter)?).unwrap();
            let dash = s.dash.iter().map(|v| num(*v))
                .collect::<Result<Vec<_>, _>>()?.join(" ");
            writeln!(self.conteudo, "[{dash}] {} d", num(s.fase)?).unwrap();
        }
        match &estilo.preenchimento {
            Some(Preenchimento::Solido(c)) => {
                self.conteudo.push_str(&cor(*c, "rg"));
                self.conteudo.push_str(&comandos);
                self.conteudo.push_str(match (estilo.stroke.is_some(), caminho.regra) {
                    (true, RegraPreenchimento::NaoZero) => "B\n",
                    (true, RegraPreenchimento::ParImpar) => "B*\n",
                    (false, RegraPreenchimento::NaoZero) => "f\n",
                    (false, RegraPreenchimento::ParImpar) => "f*\n",
                });
            }
            Some(grad @ (Preenchimento::Linear { .. } | Preenchimento::Radial { .. })) => {
                let nome = self.sombreado(grad)?;
                self.conteudo.push_str("q\n");
                self.conteudo.push_str(&comandos);
                self.conteudo.push_str(match caminho.regra {
                    RegraPreenchimento::NaoZero => "W n\n",
                    RegraPreenchimento::ParImpar => "W* n\n",
                });
                writeln!(self.conteudo, "/{nome} sh").unwrap();
                self.conteudo.push_str("Q\n");
                if estilo.stroke.is_some() {
                    self.conteudo.push_str(&comandos);
                    self.conteudo.push_str("S\n");
                }
            }
            None => {
                self.conteudo.push_str(&comandos);
                self.conteudo.push_str(if estilo.stroke.is_some() { "S\n" } else { "n\n" });
            }
        }
        self.conteudo.push_str("Q\n");
        Ok(())
    }
    fn texto(&mut self, t: &TextoPdf) -> Result<(), ErroPdf> {
        if !t.tamanho.is_finite() || t.tamanho <= 0.0 {
            return Err(ErroPdf::EstiloInvalido);
        }
        let x = num(t.x)?;
        let y = num(t.y)?;
        let tamanho = num(t.tamanho)?;
        let bytes = codificar_winansi(&t.texto)?;
        self.conteudo.push_str("q\n");
        let gs = self.estado(t.cor.a as f32 / 255.0, 1.0);
        self.conteudo.push_str(&gs);
        self.conteudo.push_str(&cor(t.cor, "rg"));
        write!(self.conteudo, "BT /F{} {tamanho} Tf 1 0 0 -1 {x} {y} Tm <",
            t.fonte.indice()).unwrap();
        for b in bytes { write!(self.conteudo, "{b:02X}").unwrap(); }
        self.conteudo.push_str("> Tj ET\nQ\n");
        Ok(())
    }
    fn finalizar(mut self, largura: f32, altura: f32) -> Vec<u8> {
        self.conteudo.push_str("Q\n");
        let bytes = self.conteudo.as_bytes();
        let mut conteudo = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
        conteudo.extend_from_slice(bytes);
        conteudo.extend_from_slice(b"endstream");
        let stream_id = self.doc.adicionar(conteudo);
        let fontes = ["Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique"];
        let ids: Vec<_> = fontes.iter().map(|nome| self.doc.adicionar(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{nome} /Encoding /WinAnsiEncoding >>"
        ))).collect();
        let paginas = self.doc.adicionar(Vec::new());
        let mut recursos = String::from("<< /Font << ");
        for (i, id) in ids.iter().enumerate() {
            write!(recursos, "/F{} {id} 0 R ", i + 1).unwrap();
        }
        recursos.push_str(">> /ExtGState << ");
        for (i, (_, _, id)) in self.alfas.iter().enumerate() {
            write!(recursos, "/GS{} {id} 0 R ", i + 1).unwrap();
        }
        recursos.push_str(">> /Shading << ");
        for (i, id) in self.sombras.iter().enumerate() {
            write!(recursos, "/Sh{} {id} 0 R ", i + 1).unwrap();
        }
        recursos.push_str(">> >>");
        let pagina = self.doc.adicionar(format!(
            "<< /Type /Page /Parent {paginas} 0 R /MediaBox [0 0 {} {}] /Resources {recursos} /Contents {stream_id} 0 R >>",
            num(largura).unwrap(), num(altura).unwrap()
        ));
        self.doc.objetos[paginas - 1] =
            format!("<< /Type /Pages /Count 1 /Kids [{pagina} 0 R] >>").into_bytes();
        let catalogo = self.doc.adicionar(format!("<< /Type /Catalog /Pages {paginas} 0 R >>"));
        self.doc.gerar(catalogo)
    }
}

fn winansi_char(c: char) -> Option<u8> {
    Some(match c {
        '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => c as u8,
        '\u{20ac}' => 0x80, '\u{201a}' => 0x82, '\u{192}' => 0x83,
        '\u{201e}' => 0x84, '\u{2026}' => 0x85, '\u{2020}' => 0x86,
        '\u{2021}' => 0x87, '\u{2c6}' => 0x88, '\u{2030}' => 0x89,
        '\u{160}' => 0x8a, '\u{2039}' => 0x8b, '\u{152}' => 0x8c,
        '\u{17d}' => 0x8e, '\u{2018}' => 0x91, '\u{2019}' => 0x92,
        '\u{201c}' => 0x93, '\u{201d}' => 0x94, '\u{2022}' => 0x95,
        '\u{2013}' => 0x96, '\u{2014}' => 0x97, '\u{2dc}' => 0x98,
        '\u{2122}' => 0x99, '\u{161}' => 0x9a, '\u{203a}' => 0x9b,
        '\u{153}' => 0x9c, '\u{17e}' => 0x9e, '\u{178}' => 0x9f,
        _ => return None,
    })
}

fn codificar_winansi(t: &str) -> Result<Vec<u8>, ErroPdf> {
    codificar_winansi_modo(t, false)
}

/// Com `substituir`, caractere fora de WinAnsi vira '?' em vez de erro.
fn codificar_winansi_modo(t: &str, substituir: bool) -> Result<Vec<u8>, ErroPdf> {
    let mut saida = Vec::with_capacity(t.len());
    for c in t.chars() {
        match winansi_char(c) {
            Some(b) => saida.push(b),
            None if substituir => saida.push(b'?'),
            None => return Err(ErroPdf::CaractereNaoSuportado),
        }
    }
    Ok(saida)
}

// ---------------------------------------------------------------------
// Métricas Helvetica (AFM padrão, 1/1000 em), ASCII 32..=126.
const LARG_REGULAR: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
const LARG_NEGRITO: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// Letra ASCII base de uma vogal/consoante acentuada Latin-1 (0xC0..=0xFF).
fn base_latin1(b: u8) -> Option<char> {
    Some(match b {
        0xC0..=0xC5 => 'A', 0xC7 => 'C', 0xC8..=0xCB => 'E', 0xCC..=0xCF => 'I',
        0xD1 => 'N', 0xD2..=0xD6 => 'O', 0xD9..=0xDC => 'U', 0xDD => 'Y',
        0xE0..=0xE5 => 'a', 0xE7 => 'c', 0xE8..=0xEB => 'e', 0xEC..=0xEF => 'i',
        0xF1 => 'n', 0xF2..=0xF6 => 'o', 0xF9..=0xFC => 'u', 0xFD | 0xFF => 'y',
        _ => return None,
    })
}

fn largura_byte(fonte: FontePdf, b: u8) -> u16 {
    let negrito = matches!(fonte, FontePdf::Negrito | FontePdf::NegritoItalico);
    let tab = if negrito { &LARG_NEGRITO } else { &LARG_REGULAR };
    if negrito {
        // Divergências do Helvetica-Bold.afm oficial em relação ao regular.
        match b {
            0xA6 => return 280,
            0xB6 => return 556,
            0xB5 | 0xF0 | 0xFE => return 611,
            0x91 | 0x92 => return 278,
            0x93 | 0x94 => return 500,
            _ => {}
        }
    }
    match b {
        0x20..=0x7e => tab[(b - 0x20) as usize],
        0xA0 => 278,
        0xA1 | 0xA8 | 0xAF | 0xB4 | 0xB8 | 0xAD | 0xB2 | 0xB3 | 0xB9 => 333,
        0xA6 => 260, 0xA9 | 0xAE => 737, 0xAA => 370, 0xB0 => 400,
        0xB6 => 537, 0xB7 => 278, 0xBA => 365, 0xBC..=0xBE => 834,
        0xAB | 0xBB | 0xA2..=0xA5 | 0xA7 | 0xB5 => 556,
        0xAC | 0xB1 | 0xD7 | 0xF7 => 584,
        0xBF => 611, 0xC6 => 1000, 0xE6 => 889, 0xD0 | 0xDE => if b == 0xD0 { 722 } else { 667 },
        0xD8 => 778, 0xF8 => 611, 0xDF => 611, 0xF0 | 0xFE => 556,
        0x85 | 0x97 | 0x99 => 1000, 0x96 | 0x80 => 556, 0x91 | 0x92 => 222,
        0x93 | 0x94 => 333, 0x95 => 350,
        _ => match base_latin1(b) {
            Some(c) => tab[(c as u8 - 0x20) as usize],
            None => 556,
        },
    }
}

/// Largura do texto em pt (métricas Helvetica AFM). Caractere fora de
/// WinAnsi é medido como '?' (o mesmo que o modo de substituição escreve).
/// Acentuadas Latin-1 usam a largura da letra base; símbolos raros são
/// aproximados. Negrito usa a tabela própria; oblíquo = regular.
pub fn largura_texto(texto: &str, fonte: FontePdf, tamanho: f32) -> f32 {
    let soma: u32 = texto.chars()
        .map(|c| largura_byte(fonte, winansi_char(c).unwrap_or(b'?')) as u32)
        .sum();
    soma as f32 * tamanho / 1000.0
}

/// Documento PDF de várias páginas. Coordenadas das páginas: origem no
/// canto SUPERIOR ESQUERDO, y cresce para baixo, unidade pt (1/72 pol);
/// a conversão para o sistema PDF (origem inferior) é interna. Em `texto`,
/// `y` é a linha de base. Erros (dimensão, número, caractere) são guardados
/// e devolvidos por `finalizar`. Cor: alfa é ignorado.
#[derive(Debug, Clone, Default)]
pub struct DocumentoPdf {
    paginas: Vec<PaginaPdf>,
    substituir: bool,
}

#[derive(Debug, Clone)]
pub struct PaginaPdf {
    largura: f32,
    altura: f32,
    conteudo: String,
    erro: Option<ErroPdf>,
    recortes: u32,
    substituir: bool,
}

impl DocumentoPdf {
    pub fn novo() -> Self { Self::default() }

    /// Se verdadeiro, caractere fora de WinAnsi vira '?' (páginas
    /// adicionadas depois desta chamada). Padrão: erro `CaractereNaoSuportado`.
    pub fn substituir_nao_suportados(&mut self, ativo: bool) -> &mut Self {
        self.substituir = ativo; self
    }

    pub fn adicionar_pagina(&mut self, largura_pt: f32, altura_pt: f32) -> &mut PaginaPdf {
        let valida = largura_pt.is_finite() && altura_pt.is_finite()
            && largura_pt > 0.0 && altura_pt > 0.0
            && largura_pt <= 14_400.0 && altura_pt <= 14_400.0;
        self.paginas.push(PaginaPdf {
            largura: largura_pt, altura: altura_pt, conteudo: String::new(),
            erro: if valida { None } else { Some(ErroPdf::DimensaoInvalida) },
            recortes: 0, substituir: self.substituir,
        });
        self.paginas.last_mut().unwrap()
    }

    pub fn finalizar(self) -> Result<Vec<u8>, ErroPdf> {
        if self.paginas.is_empty() { return Err(ErroPdf::DimensaoInvalida); }
        for p in &self.paginas {
            if let Some(e) = p.erro { return Err(e); }
            if p.recortes != 0 { return Err(ErroPdf::EstiloInvalido); }
        }
        let mut doc = Documentos::novo();
        let nomes = ["Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique"];
        let fontes: Vec<usize> = nomes.iter().map(|n| doc.adicionar(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{n} /Encoding /WinAnsiEncoding >>"
        ))).collect();
        let mut recursos = String::from("<< /Font << ");
        for (i, id) in fontes.iter().enumerate() {
            write!(recursos, "/F{} {id} 0 R ", i + 1).unwrap();
        }
        recursos.push_str(">> >>");
        let pages = doc.adicionar(Vec::new());
        let mut kids = Vec::new();
        for p in &self.paginas {
            let bytes = p.conteudo.as_bytes();
            let mut c = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
            c.extend_from_slice(bytes);
            c.extend_from_slice(b"endstream");
            let cid = doc.adicionar(c);
            kids.push(doc.adicionar(format!(
                "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {} {}] /Resources {recursos} /Contents {cid} 0 R >>",
                num(p.largura)?, num(p.altura)?)));
        }
        let lista = kids.iter().map(|k| format!("{k} 0 R")).collect::<Vec<_>>().join(" ");
        doc.objetos[pages - 1] = format!(
            "<< /Type /Pages /Count {} /Kids [{lista}] >>", kids.len()).into_bytes();
        let cat = doc.adicionar(format!("<< /Type /Catalog /Pages {pages} 0 R >>"));
        Ok(doc.gerar(cat))
    }
}

impl PaginaPdf {
    fn falha(&mut self, e: ErroPdf) { if self.erro.is_none() { self.erro = Some(e); } }
    fn n(&mut self, v: f32) -> String {
        match num(v) { Ok(s) => s, Err(e) => { self.falha(e); "0".into() } }
    }
    /// Converte (x, y, l, a) do topo-esquerdo para `re` do PDF.
    fn re(&mut self, x: f32, y: f32, l: f32, a: f32) -> String {
        let (px, py) = (self.n(x), self.n(self.altura - y - a));
        let (pl, pa) = (self.n(l), self.n(a));
        format!("{px} {py} {pl} {pa} re\n")
    }

    pub fn retangulo_preenchido(&mut self, x: f32, y: f32, l: f32, a: f32, c: Cor) {
        let re = self.re(x, y, l, a);
        write!(self.conteudo, "q\n{}{re}f\nQ\n", cor(c, "rg")).unwrap();
    }

    pub fn retangulo_contorno(&mut self, x: f32, y: f32, l: f32, a: f32, c: Cor, espessura: f32) {
        let re = self.re(x, y, l, a);
        let w = self.n(espessura);
        write!(self.conteudo, "q\n{}{w} w\n{re}S\nQ\n", cor(c, "RG")).unwrap();
    }

    pub fn linha(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Cor,
        espessura: f32, tracejado: bool)
    {
        let (ax, ay) = (self.n(x0), self.n(self.altura - y0));
        let (bx, by) = (self.n(x1), self.n(self.altura - y1));
        let w = self.n(espessura);
        let dash = if tracejado {
            let d = self.n((espessura * 3.0).max(1.0));
            format!("[{d} {d}] 0 d\n")
        } else { String::new() };
        write!(self.conteudo, "q\n{}{w} w\n{dash}{ax} {ay} m\n{bx} {by} l\nS\nQ\n",
            cor(c, "RG")).unwrap();
    }

    /// Abre região de recorte retangular (`q re W n`); feche com `fim_recorte`.
    pub fn recorte(&mut self, x: f32, y: f32, l: f32, a: f32) {
        let re = self.re(x, y, l, a);
        write!(self.conteudo, "q\n{re}W n\n").unwrap();
        self.recortes += 1;
    }

    pub fn fim_recorte(&mut self) {
        if self.recortes == 0 { self.falha(ErroPdf::EstiloInvalido); return; }
        self.recortes -= 1;
        self.conteudo.push_str("Q\n");
    }

    /// Texto com a linha de base em (x, y).
    pub fn texto(&mut self, x: f32, y: f32, texto: &str, fonte: FontePdf, tamanho: f32, c: Cor) {
        if !tamanho.is_finite() || tamanho <= 0.0 { self.falha(ErroPdf::EstiloInvalido); return; }
        let bytes = match codificar_winansi_modo(texto, self.substituir) {
            Ok(b) => b, Err(e) => { self.falha(e); return; }
        };
        let (px, py, t) = (self.n(x), self.n(self.altura - y), self.n(tamanho));
        write!(self.conteudo, "q\n{}BT /F{} {t} Tf {px} {py} Td <", cor(c, "rg"),
            fonte.indice()).unwrap();
        for b in bytes { write!(self.conteudo, "{b:02X}").unwrap(); }
        self.conteudo.push_str("> Tj ET\nQ\n");
    }
}

/// Exporta a Cena para uma página PDF vetorial, sem rasterizar formas.
pub fn exportar_pdf(cena: &Cena) -> Result<Vec<u8>, ErroPdf> {
    exportar_pdf_com_texto(cena, &[])
}
/// Versão com texto adicional. A medição de fontes fica no consumidor.
/// Gradientes com alfa variável exigem soft-mask e retornam erro explícito.
pub fn exportar_pdf_com_texto(cena: &Cena, textos: &[TextoPdf]) -> Result<Vec<u8>, ErroPdf> {
    if !cena.largura.is_finite() || !cena.altura.is_finite()
        || cena.largura <= 0.0 || cena.altura <= 0.0
        || cena.largura > 14_400.0 || cena.altura > 14_400.0
    {
        return Err(ErroPdf::DimensaoInvalida);
    }
    let mut e = Emissor {
        doc: Documentos::novo(),
        conteudo: format!("q\n1 0 0 -1 0 {} cm\n", num(cena.altura)?),
        alfas: Vec::new(), sombras: Vec::new(),
    };
    for figura in &cena.figuras { e.figura(figura)?; }
    for texto in textos { e.texto(texto)?; }
    Ok(e.finalizar(cena.largura, cena.altura))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{estilos::{EstiloVetor, Parada, Stroke}, Estilo, Ponto, Retangulo};

    #[test]
    fn pdf_deterministico_e_coordenadas_fontes_offsets() {
        let mut c = Cena::nova(200.0, 100.0);
        c.adicionar(Figura::Retangulo {
            area: Retangulo::novo(10.0, 20.0, 30.0, 40.0),
            estilo: Estilo::default(),
        });
        let texto = [TextoPdf {
            texto: "São Paulo €".into(), x: 12.0, y: 24.0,
            tamanho: 14.0, cor: Cor::rgb(1, 2, 3), fonte: FontePdf::Negrito,
        }];
        let pdf = exportar_pdf_com_texto(&c, &texto).unwrap();
        assert_eq!(pdf, exportar_pdf_com_texto(&c, &texto).unwrap());
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4\n"));
        assert!(s.contains("/MediaBox [0 0 200 100]"));
        assert!(s.contains("1 0 0 -1 0 100 cm"));
        assert!(s.contains("10 20 m\n40 20 l\n40 60 l\n10 60 l\nh\n"));
        assert!(s.contains("/BaseFont /Helvetica-Bold"));
        assert!(s.contains("BT /F2 14 Tf 1 0 0 -1 12 24 Tm"));
        assert!(s.contains("53E36F205061756C6F2080"));
        let off = s.rfind("startxref\n").unwrap();
        let xref: usize = s[off + 10..].lines().next().unwrap().parse().unwrap();
        assert_eq!(&pdf[xref..xref + 4], b"xref");
        assert!(s.ends_with("%%EOF\n"));
    }

    #[test]
    fn pdf_exporta_curvas_clips_gradiente_alpha_e_dash() {
        let mut c = Cena::nova(300.0, 200.0);
        let mut caminho = Caminho::novo();
        caminho.adicionar(Comando::Mover(Ponto::novo(0.0, 0.0)));
        caminho.adicionar(Comando::Quadratica {
            controle: Ponto::novo(40.0, 50.0), fim: Ponto::novo(100.0, 0.0),
        });
        caminho.adicionar(Comando::Fechar);
        let mut stroke = Stroke::novo(Cor::rgba(10, 20, 30, 128), 2.0);
        stroke.dash = vec![4.0, 2.0];
        let preenchimento = Preenchimento::Linear {
            de: Ponto::novo(0.0, 0.0), ate: Ponto::novo(100.0, 0.0),
            paradas: vec![
                Parada { posicao: 0.0, cor: Cor::rgba(255, 0, 0, 128) },
                Parada { posicao: 1.0, cor: Cor::rgba(0, 0, 255, 128) },
            ],
        };
        let fig = Figura::Caminho {
            caminho, estilo: EstiloVetor {
                preenchimento: Some(preenchimento),
                stroke: Some(stroke), opacidade: 0.5, texto: None,
            },
        };
        let mut recorte = Caminho::novo();
        recorte.regra = RegraPreenchimento::ParImpar;
        recorte.comandos = vec![
            Comando::Mover(Ponto::novo(0.0, 0.0)),
            Comando::Linha(Ponto::novo(80.0, 0.0)),
            Comando::Linha(Ponto::novo(80.0, 80.0)),
            Comando::Fechar,
        ];
        c.adicionar(Figura::Recorte { figura: Box::new(fig), recorte });
        let pdf = exportar_pdf(&c).unwrap();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("W* n\n"));
        assert!(s.contains(" c\n"));
        assert!(s.contains("/ShadingType 2"));
        assert!(s.contains("/FunctionType 0"));
        assert!(s.contains("/Sh1 sh\n"));
        assert!(s.contains("[4 2] 0 d"));
        assert!(s.contains("/ca 0.251"));
        assert!(s.contains("\nS\n"));
    }

    #[test]
    fn amostragem_pdf_respeita_bordas_duras_do_modelo_compartilhado() {
        let mut cena = Cena::nova(100.0, 80.0);
        let vermelho = Cor::rgb(255, 0, 0);
        let azul = Cor::rgb(0, 0, 255);
        let gradiente = Preenchimento::Linear {
            de: Ponto::novo(0.0, 0.0),
            ate: Ponto::novo(100.0, 0.0),
            paradas: vec![
                Parada { posicao: 0.0, cor: vermelho },
                Parada { posicao: 0.5, cor: vermelho },
                Parada { posicao: 0.5, cor: azul },
                Parada { posicao: 1.0, cor: azul },
            ],
        };
        cena.adicionar(Figura::Elipse {
            centro: Ponto::novo(50.0, 40.0),
            raio_x: 30.0,
            raio_y: 20.0,
            estilo: EstiloVetor {
                preenchimento: Some(gradiente),
                stroke: None,
                ..Default::default()
            },
        });
        let dados = exportar_pdf(&cena).unwrap();
        let func = dados.windows(b"/FunctionType 0".len())
            .position(|w| w == b"/FunctionType 0").unwrap();
        let fluxo = dados[func..].windows(b"stream\n".len())
            .position(|w| w == b"stream\n").unwrap();
        let tabela_inicio = func + fluxo + b"stream\n".len();
        let tabela = &dados[tabela_inicio..tabela_inicio + 256 * 3];
        // 127/255 < 0,5; 128/255 > 0,5: não há cor intermediária.
        assert_eq!(&tabela[127 * 3..127 * 3 + 3], &[255, 0, 0]);
        assert_eq!(&tabela[128 * 3..128 * 3 + 3], &[0, 0, 255]);
    }

    #[test]
    fn rejeita_texto_unicode_sem_fonte_e_gradiente_alpha_variavel() {
        let mut cena = Cena::nova(100.0, 100.0);
        let txt = [TextoPdf {
            texto: "東京".into(), x: 0.0, y: 0.0, tamanho: 12.0,
            cor: Cor::rgb(0, 0, 0), fonte: FontePdf::Helvetica,
        }];
        assert_eq!(exportar_pdf_com_texto(&cena, &txt),
            Err(ErroPdf::CaractereNaoSuportado));
        cena.adicionar(Figura::Elipse {
            centro: Ponto::novo(20.0, 20.0), raio_x: 10.0, raio_y: 10.0,
            estilo: EstiloVetor {
                preenchimento: Some(Preenchimento::Radial {
                    centro: Ponto::novo(20.0, 20.0), raio: 10.0,
                    paradas: vec![
                        Parada { posicao: 0.0, cor: Cor::rgba(255, 0, 0, 0) },
                        Parada { posicao: 1.0, cor: Cor::rgba(0, 0, 255, 255) },
                    ],
                }),
                ..Default::default()
            },
        });
        assert_eq!(exportar_pdf(&cena), Err(ErroPdf::AlfaGradienteVariavel));
        cena.largura = f32::NAN;
        assert_eq!(exportar_pdf(&cena), Err(ErroPdf::DimensaoInvalida));
    }

    #[test]
    fn documento_tres_paginas_xref_e_recorte() {
        let mut d = DocumentoPdf::novo();
        for i in 0..3 {
            let p = d.adicionar_pagina(595.0, 842.0);
            p.retangulo_preenchido(10.0, 10.0, 100.0, 20.0, Cor::rgb(200, 200, 200));
            p.retangulo_contorno(10.0, 10.0, 100.0, 20.0, Cor::rgb(0, 0, 0), 0.5);
            p.linha(0.0, 50.0, 100.0, 50.0, Cor::rgb(0, 0, 0), 1.0, i == 1);
            p.recorte(10.0, 10.0, 50.0, 20.0);
            p.texto(12.0, 24.0, "Ação número é única — São Paulo", FontePdf::Negrito, 10.0, Cor::rgb(0, 0, 0));
            p.fim_recorte();
        }
        let pdf = d.finalizar().unwrap();
        let s = String::from_utf8_lossy(&pdf).to_string();
        assert!(s.starts_with("%PDF"));
        assert!(s.contains("/Count 3"));
        assert!(s.contains("10 812 50 20 re\nW n\n"));
        assert!(s.contains("[3 3] 0 d"));
        // acentos: ç(E7) ã(E3) é(E9)
        assert!(s.contains("41E7E36F"));
        // xref: cada offset aponta para "N 0 obj".
        let xref = pdf.windows(6).rposition(|w| w == b"\nxref\n").unwrap() + 1;
        let tab = std::str::from_utf8(&pdf[xref..]).unwrap();
        let mut linhas = tab.lines();
        linhas.next();
        let total: usize = linhas.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
        linhas.next();
        for n in 1..total {
            let off: usize = linhas.next().unwrap()[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{n} 0 obj").as_bytes()), "obj {n}");
        }
    }

    #[test]
    fn documento_erro_e_substituicao_e_largura() {
        let mut d = DocumentoPdf::novo();
        d.adicionar_pagina(100.0, 100.0)
            .texto(0.0, 10.0, "東", FontePdf::Helvetica, 10.0, Cor::rgb(0, 0, 0));
        assert_eq!(d.finalizar(), Err(ErroPdf::CaractereNaoSuportado));
        let mut d = DocumentoPdf::novo();
        d.substituir_nao_suportados(true);
        d.adicionar_pagina(100.0, 100.0)
            .texto(0.0, 10.0, "東", FontePdf::Helvetica, 10.0, Cor::rgb(0, 0, 0));
        assert!(String::from_utf8_lossy(&d.finalizar().unwrap()).contains("<3F> Tj"));
        assert_eq!(largura_texto("Hello", FontePdf::Helvetica, 1000.0), 2278.0);
        assert_eq!(largura_texto("ã", FontePdf::Helvetica, 1000.0), 556.0);
        assert_eq!(DocumentoPdf::novo().finalizar(), Err(ErroPdf::DimensaoInvalida));
    }
}

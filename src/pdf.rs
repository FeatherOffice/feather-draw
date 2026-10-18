//! Exportador PDF 1.4 vetorial e determinístico, independente de Iced.
//! Coordenadas da Cena usam origem superior esquerda e pontos PDF.
//! Fontes base (Helvetica, Times e Courier) usam codificação WinAnsi; caracteres
//! não suportados causam erro explícito, nunca substituição silenciosa.
use crate::{caminhos::{Caminho, Comando, RegraPreenchimento},
    estilos::{Extremo, Juncao, Preenchimento}, Cena, Cor, Figura};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroPdf {
    DimensaoInvalida,
    GeometriaInvalida,
    EstiloInvalido,
    AlfaGradienteVariavel,
    CaractereNaoSuportado,
    /// O arquivo passado a `DocumentoPdf::fonte` não é uma fonte TrueType (com contornos `glyf`).
    FonteInvalida,
}

/// Uma fonte TrueType embutida no documento (devolvida por `DocumentoPdf::fonte`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FonteEmbutida(usize);

/// Para onde um link leva: um endereço ou um ponto de uma página (`y` a partir do topo).
#[derive(Debug, Clone, PartialEq)]
pub enum DestinoPdf {
    Url(String),
    Pagina { indice: usize, y: f32 },
}

/// Os dados do documento que o leitor de PDF mostra em Propriedades (vazio = não grava).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetadadosPdf {
    pub titulo: String,
    pub autor: String,
    pub assunto: String,
    pub palavras_chave: String,
    pub criador: String,
}

#[derive(Debug, Clone)]
struct Embutida {
    dados: Vec<u8>,
    nome: String,
}

/// Um fluxo comprimido (`FlateDecode`) com as chaves `extra` no dicionário.
fn fluxo_comprimido(extra: &str, dados: &[u8]) -> Vec<u8> {
    let z = miniz_oxide::deflate::compress_to_vec_zlib(dados, 6);
    let mut obj = format!("<< {extra}/Filter /FlateDecode /Length {} >>\nstream\n", z.len()).into_bytes();
    obj.extend_from_slice(&z);
    obj.extend_from_slice(b"\nendstream");
    obj
}

/// Texto do PDF em UTF-16BE (`<FEFF…>`): título, autor, marcador.
fn texto_unicode(s: &str) -> String {
    let mut saida = String::from("<FEFF");
    for u in s.encode_utf16() {
        write!(saida, "{u:04X}").unwrap();
    }
    saida.push('>');
    saida
}

/// Endereço como string literal do PDF: ASCII, com `( ) \\` escapados e o resto em `%XX`.
fn url_literal(u: &str) -> String {
    let mut saida = String::from("(");
    for b in u.bytes() {
        match b {
            b'(' | b')' | b'\\' => { saida.push('\\'); saida.push(b as char); }
            0x21..=0x7e => saida.push(b as char),
            _ => write!(saida, "%{b:02X}").unwrap(),
        }
    }
    saida.push(')');
    saida
}

/// Os objetos de uma fonte embutida (Type0 + CIDFontType2, CID = código Unicode) e o id do Type0.
fn objetos_da_fonte(doc: &mut Documentos, f: &Embutida, usados: &BTreeSet<u16>) -> Result<usize, ErroPdf> {
    let face = ttf_parser::Face::parse(&f.dados, 0).map_err(|_| ErroPdf::FonteInvalida)?;
    let upem = f32::from(face.units_per_em());
    let em = |v: f32| (v * 1000.0 / upem).round() as i32;
    let gid = |cid: u16| char::from_u32(u32::from(cid)).and_then(|c| face.glyph_index(c)).map_or(0, |g| g.0);
    // shortcut: a fonte vai inteira (sem subconjunto); fazer o subconjunto se o tamanho do PDF incomodar.
    let arquivo = doc.adicionar(fluxo_comprimido(&format!("/Length1 {} ", f.dados.len()), &f.dados));
    let caixa = face.global_bounding_box();
    let italico = face.is_italic();
    let descritor = doc.adicionar(format!(
        "<< /Type /FontDescriptor /FontName /{} /Flags {} /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile2 {arquivo} 0 R >>",
        f.nome, 32 + if italico { 64 } else { 0 },
        em(caixa.x_min.into()), em(caixa.y_min.into()), em(caixa.x_max.into()), em(caixa.y_max.into()),
        num(face.italic_angle())?, em(face.ascender().into()), em(face.descender().into()),
        em(face.capital_height().unwrap_or(face.ascender()).into()),
    ));
    let maior = usados.last().copied().unwrap_or(0);
    let mut mapa = vec![0u8; 2 * (usize::from(maior) + 1)];
    let mut larguras = String::new();
    for &cid in usados {
        let g = gid(cid);
        mapa[2 * usize::from(cid)..2 * usize::from(cid) + 2].copy_from_slice(&g.to_be_bytes());
        let avanco = face.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0);
        write!(larguras, "{cid} [{}] ", em(avanco.into())).unwrap();
    }
    let mapa = doc.adicionar(fluxo_comprimido("", &mapa));
    let cid = doc.adicionar(format!(
        "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descritor} 0 R /W [{larguras}] /CIDToGIDMap {mapa} 0 R >>",
        f.nome
    ));
    // ToUnicode: o CID já é o código, então o texto copiado do PDF sai igual.
    let mut cmap = String::from("/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n");
    let lista: Vec<u16> = usados.iter().copied().collect();
    for bloco in lista.chunks(100) {
        writeln!(cmap, "{} beginbfchar", bloco.len()).unwrap();
        for c in bloco {
            writeln!(cmap, "<{c:04X}> <{c:04X}>").unwrap();
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend");
    let unicode = doc.adicionar(fluxo_comprimido("", cmap.as_bytes()));
    Ok(doc.adicionar(format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /{} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {unicode} 0 R >>",
        f.nome
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontePdf {
    Helvetica, Negrito, Italico, NegritoItalico,
    /// Times (fonte padrão do PDF; mesmas métricas da Liberation Serif).
    Serifa, SerifaNegrito, SerifaItalico, SerifaNegritoItalico,
    /// Courier: todo caractere mede 600/1000 em.
    Mono, MonoNegrito, MonoItalico, MonoNegritoItalico,
}
impl FontePdf {
    fn indice(self) -> usize {
        self as usize + 1
    }
}

/// Os nomes PDF na ordem de `FontePdf` (`/F1` = Helvetica…).
const NOMES_FONTES: [&str; 12] = [
    "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
    "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
    "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
];

/// As quatro Helveticas sempre (os PDFs que só as usam não mudam um byte);
/// Times e Courier só quando alguma página as usa.
fn fontes_usadas(extras: bool) -> &'static [&'static str] {
    if extras { &NOMES_FONTES } else { &NOMES_FONTES[..4] }
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
    fn gerar(self, root: usize, info: Option<usize>) -> Vec<u8> {
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
            "trailer\n<< /Size {total} /Root {root} 0 R{} >>\nstartxref\n{xref}\n%%EOF\n",
            info.map_or(String::new(), |i| format!(" /Info {i} 0 R"))
        ).as_bytes());
        saida
    }
}

struct Emissor {
    doc: Documentos,
    conteudo: String,
    alfas: Vec<(u16, u16, usize)>,
    sombras: Vec<usize>,
    /// Algum texto usa Times ou Courier.
    extras: bool,
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
        let ids: Vec<_> = fontes_usadas(self.extras).iter().map(|nome| self.doc.adicionar(format!(
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
        self.doc.gerar(catalogo, None)
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
    use FontePdf::*;
    if matches!(fonte, Mono | MonoNegrito | MonoItalico | MonoNegritoItalico) {
        return 600;
    }
    // shortcut: Times é medida com a tabela da Helvetica; ter a tabela Times quando alguém medir texto com serifa aqui.
    let negrito = matches!(fonte, Negrito | NegritoItalico | SerifaNegrito | SerifaNegritoItalico);
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
    embutidas: Vec<Embutida>,
    /// (nível, título, página, y a partir do topo).
    marcadores: Vec<(u8, String, usize, f32)>,
    metadados: Option<MetadadosPdf>,
}

#[derive(Debug, Clone)]
pub struct PaginaPdf {
    largura: f32,
    altura: f32,
    conteudo: String,
    erro: Option<ErroPdf>,
    recortes: u32,
    substituir: bool,
    extras: bool,
    /// Imagens RGB cruas da página: (bytes, largura, altura) em pixels; `/Im1`… na ordem.
    imagens: Vec<(Vec<u8>, u32, u32)>,
    /// Os caracteres usados de cada fonte embutida (para as larguras e o mapa de glifos).
    usados: BTreeMap<usize, BTreeSet<u16>>,
    /// Áreas clicáveis: (x, y, largura, altura) a partir do topo e o destino.
    links: Vec<([f32; 4], DestinoPdf)>,
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
            recortes: 0, substituir: self.substituir, extras: false, imagens: Vec::new(),
            usados: BTreeMap::new(), links: Vec::new(),
        });
        self.paginas.last_mut().unwrap()
    }

    /// Registra uma fonte TrueType (o arquivo `.ttf` inteiro) para `PaginaPdf::texto_embutido`.
    /// Ela só entra no PDF se alguma página a usar.
    pub fn fonte(&mut self, dados: Vec<u8>) -> Result<FonteEmbutida, ErroPdf> {
        let face = ttf_parser::Face::parse(&dados, 0).map_err(|_| ErroPdf::FonteInvalida)?;
        if face.tables().glyf.is_none() {
            return Err(ErroPdf::FonteInvalida);
        }
        // O nome PostScript pode estar só na entrada do Mac (bytes ASCII), que `to_string` não lê.
        let nome: String = face.names().into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .find_map(|n| n.to_string().or_else(|| n.name.is_ascii().then(|| String::from_utf8_lossy(n.name).into_owned())))
            .unwrap_or_default()
            .chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        let nome = if nome.is_empty() { format!("Fonte{}", self.embutidas.len() + 1) } else { nome };
        self.embutidas.push(Embutida { dados, nome });
        Ok(FonteEmbutida(self.embutidas.len() - 1))
    }

    /// Título, autor, assunto e palavras-chave (Propriedades do leitor de PDF).
    pub fn metadados(&mut self, m: MetadadosPdf) -> &mut Self {
        self.metadados = Some(m);
        self
    }

    /// Um marcador (a árvore lateral do leitor): `nivel` 1 é o de cima; leva à `pagina` na altura `y`.
    pub fn marcador(&mut self, nivel: u8, titulo: &str, pagina: usize, y: f32) -> &mut Self {
        self.marcadores.push((nivel.max(1), titulo.to_string(), pagina, y));
        self
    }

    /// A página `i` já adicionada (para pôr links depois de saber onde ficam os destinos).
    pub fn pagina(&mut self, i: usize) -> Option<&mut PaginaPdf> {
        self.paginas.get_mut(i)
    }

    pub fn finalizar(self) -> Result<Vec<u8>, ErroPdf> {
        if self.paginas.is_empty() { return Err(ErroPdf::DimensaoInvalida); }
        for p in &self.paginas {
            if let Some(e) = p.erro { return Err(e); }
            if p.recortes != 0 { return Err(ErroPdf::EstiloInvalido); }
        }
        let mut doc = Documentos::novo();
        let extras = self.paginas.iter().any(|p| p.extras);
        let fontes: Vec<usize> = fontes_usadas(extras).iter().map(|n| doc.adicionar(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{n} /Encoding /WinAnsiEncoding >>"
        ))).collect();
        let mut fontes_dic = String::from("/Font << ");
        for (i, id) in fontes.iter().enumerate() {
            write!(fontes_dic, "/F{} {id} 0 R ", i + 1).unwrap();
        }
        let mut usados: BTreeMap<usize, BTreeSet<u16>> = BTreeMap::new();
        for p in &self.paginas {
            for (f, cs) in &p.usados {
                usados.entry(*f).or_default().extend(cs);
            }
        }
        for (f, cs) in &usados {
            let e = self.embutidas.get(*f).ok_or(ErroPdf::FonteInvalida)?;
            let id = objetos_da_fonte(&mut doc, e, cs)?;
            write!(fontes_dic, "/E{} {id} 0 R ", f + 1).unwrap();
        }
        fontes_dic.push_str(">>");
        let pages = doc.adicionar(Vec::new());
        // Os ids das páginas antes do conteúdo: links e marcadores apontam para elas.
        let kids: Vec<usize> = self.paginas.iter().map(|_| doc.adicionar(Vec::new())).collect();
        let destino = |indice: usize, y: f32| -> Option<String> {
            let p = self.paginas.get(indice)?;
            Some(format!("[{} 0 R /XYZ 0 {} null]", kids[indice], num(p.altura - y).ok()?))
        };
        for (k, p) in self.paginas.iter().enumerate() {
            // Sem imagem, o dicionário de recursos fica igual ao de antes (PDFs antigos não mudam).
            let mut recursos = format!("<< {fontes_dic} ");
            if !p.imagens.is_empty() {
                recursos.push_str("/XObject << ");
                for (i, (dados, w, h)) in p.imagens.iter().enumerate() {
                    let id = doc.adicionar(fluxo_comprimido(&format!("/Type /XObject /Subtype /Image /Width {w} /Height {h} \
/ColorSpace /DeviceRGB /BitsPerComponent 8 "), dados));
                    write!(recursos, "/Im{} {id} 0 R ", i + 1).unwrap();
                }
                recursos.push_str(">> ");
            }
            recursos.push_str(">>");
            let bytes = p.conteudo.as_bytes();
            let mut c = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
            c.extend_from_slice(bytes);
            c.extend_from_slice(b"endstream");
            let cid = doc.adicionar(c);
            let mut anotacoes = String::new();
            for ([x, y, l, a], d) in &p.links {
                let acao = match d {
                    DestinoPdf::Url(u) => format!("/A << /S /URI /URI {} >>", url_literal(u)),
                    DestinoPdf::Pagina { indice, y } => match destino(*indice, *y) {
                        Some(d) => format!("/Dest {d}"),
                        None => continue,
                    },
                };
                let id = doc.adicionar(format!(
                    "<< /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] {acao} >>",
                    num(*x)?, num(p.altura - y - a)?, num(x + l)?, num(p.altura - y)?));
                write!(anotacoes, "{id} 0 R ").unwrap();
            }
            let anotacoes = if anotacoes.is_empty() { String::new() } else { format!(" /Annots [{}]", anotacoes.trim_end()) };
            doc.objetos[kids[k] - 1] = format!(
                "<< /Type /Page /Parent {pages} 0 R /MediaBox [0 0 {} {}] /Resources {recursos} /Contents {cid} 0 R{anotacoes} >>",
                num(p.largura)?, num(p.altura)?).into_bytes();
        }
        let lista = kids.iter().map(|k| format!("{k} 0 R")).collect::<Vec<_>>().join(" ");
        doc.objetos[pages - 1] = format!(
            "<< /Type /Pages /Count {} /Kids [{lista}] >>", kids.len()).into_bytes();
        let contornos = self.arvore_de_marcadores(&mut doc, &destino)?;
        let contornos = contornos.map_or(String::new(), |id| format!(" /Outlines {id} 0 R /PageMode /UseOutlines"));
        let cat = doc.adicionar(format!("<< /Type /Catalog /Pages {pages} 0 R{contornos} >>"));
        let info = self.metadados.as_ref().map(|m| {
            let mut d = String::from("<< ");
            for (chave, v) in [("Title", &m.titulo), ("Author", &m.autor), ("Subject", &m.assunto), ("Keywords", &m.palavras_chave), ("Creator", &m.criador)] {
                if !v.is_empty() {
                    write!(d, "/{chave} {} ", texto_unicode(v)).unwrap();
                }
            }
            d.push_str("/Producer (feather-draw) >>");
            doc.adicionar(d)
        });
        Ok(doc.gerar(cat, info))
    }

    /// Os objetos dos marcadores (`/Outlines`); os de nível 1 aparecem, os de baixo ficam fechados.
    fn arvore_de_marcadores(&self, doc: &mut Documentos, destino: &dyn Fn(usize, f32) -> Option<String>) -> Result<Option<usize>, ErroPdf> {
        let itens: Vec<&(u8, String, usize, f32)> = self.marcadores.iter().filter(|m| destino(m.2, m.3).is_some()).collect();
        if itens.is_empty() {
            return Ok(None);
        }
        let raiz = doc.adicionar(Vec::new());
        let ids: Vec<usize> = itens.iter().map(|_| doc.adicionar(Vec::new())).collect();
        // O pai de cada item: o último antes dele com nível menor.
        let mut pais: Vec<Option<usize>> = Vec::with_capacity(itens.len());
        let mut pilha: Vec<usize> = Vec::new();
        for (i, m) in itens.iter().enumerate() {
            while pilha.last().is_some_and(|&j| itens[j].0 >= m.0) {
                pilha.pop();
            }
            pais.push(pilha.last().copied());
            pilha.push(i);
        }
        let filhos = |p: Option<usize>| -> Vec<usize> { (0..itens.len()).filter(|&i| pais[i] == p).collect() };
        let descendentes = |i: usize| -> usize { (i + 1..itens.len()).take_while(|&j| itens[j].0 > itens[i].0).count() };
        for (i, m) in itens.iter().enumerate() {
            let irmaos = filhos(pais[i]);
            let pos = irmaos.iter().position(|&j| j == i).unwrap_or(0);
            let mut d = format!("<< /Title {} /Parent {} 0 R /Dest {}", texto_unicode(&m.1), pais[i].map_or(raiz, |p| ids[p]), destino(m.2, m.3).unwrap_or_default());
            if pos > 0 {
                write!(d, " /Prev {} 0 R", ids[irmaos[pos - 1]]).unwrap();
            }
            if let Some(&n) = irmaos.get(pos + 1) {
                write!(d, " /Next {} 0 R", ids[n]).unwrap();
            }
            let meus = filhos(Some(i));
            if let (Some(a), Some(b)) = (meus.first(), meus.last()) {
                write!(d, " /First {} 0 R /Last {} 0 R /Count -{}", ids[*a], ids[*b], descendentes(i)).unwrap();
            }
            d.push_str(" >>");
            doc.objetos[ids[i] - 1] = d.into_bytes();
        }
        let topo = filhos(None);
        doc.objetos[raiz - 1] = format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
            ids[topo[0]], ids[*topo.last().unwrap_or(&topo[0])], topo.len()).into_bytes();
        Ok(Some(raiz))
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

    /// Imagem RGB de 8 bits (`largura_px × altura_px × 3` bytes, linha a linha) esticada em
    /// (x, y, l, a), com o canto superior esquerdo em (x, y).
    pub fn imagem_rgb(&mut self, x: f32, y: f32, l: f32, a: f32, rgb: Vec<u8>, largura_px: u32, altura_px: u32) {
        if largura_px == 0 || altura_px == 0
            || rgb.len() as u64 != u64::from(largura_px) * u64::from(altura_px) * 3 {
            self.falha(ErroPdf::DimensaoInvalida);
            return;
        }
        let (px, py, pl, pa) = (self.n(x), self.n(self.altura - y - a), self.n(l), self.n(a));
        self.imagens.push((rgb, largura_px, altura_px));
        write!(self.conteudo, "q\n{pl} 0 0 {pa} {px} {py} cm\n/Im{} Do\nQ\n", self.imagens.len()).unwrap();
    }

    /// Texto com a linha de base começando em (x, y), girado `graus` no sentido anti-horário
    /// (como a marca d'água na diagonal).
    pub fn texto_girado(&mut self, x: f32, y: f32, graus: f32, texto: &str, fonte: FontePdf, tamanho: f32, c: Cor) {
        if !tamanho.is_finite() || tamanho <= 0.0 || !graus.is_finite() { self.falha(ErroPdf::EstiloInvalido); return; }
        let bytes = match codificar_winansi_modo(texto, self.substituir) {
            Ok(b) => b, Err(e) => { self.falha(e); return; }
        };
        self.extras |= fonte.indice() > 4;
        let (sen, cos) = graus.to_radians().sin_cos();
        let (px, py, t) = (self.n(x), self.n(self.altura - y), self.n(tamanho));
        let (a, b, c2) = (self.n(cos), self.n(sen), self.n(-sen));
        write!(self.conteudo, "q\n{}BT /F{} {t} Tf {a} {b} {c2} {a} {px} {py} Tm <", cor(c, "rg"),
            fonte.indice()).unwrap();
        for b in bytes { write!(self.conteudo, "{b:02X}").unwrap(); }
        self.conteudo.push_str("> Tj ET\nQ\n");
    }

    /// Texto numa fonte embutida, com a linha de base em (x, y) e girado `graus` (0 = reto).
    /// Caractere fora do plano básico do Unicode vira '?'.
    pub fn texto_embutido(&mut self, x: f32, y: f32, graus: f32, texto: &str, fonte: FonteEmbutida, tamanho: f32, c: Cor) {
        if !tamanho.is_finite() || tamanho <= 0.0 || !graus.is_finite() { self.falha(ErroPdf::EstiloInvalido); return; }
        let usados = self.usados.entry(fonte.0).or_default();
        let mut hex = String::with_capacity(texto.len() * 4);
        for ch in texto.chars() {
            let cid = u16::try_from(u32::from(ch)).unwrap_or(u16::from(b'?'));
            usados.insert(cid);
            write!(hex, "{cid:04X}").unwrap();
        }
        let (sen, cos) = graus.to_radians().sin_cos();
        let (px, py, t) = (self.n(x), self.n(self.altura - y), self.n(tamanho));
        let (a, b, c2) = (self.n(cos), self.n(sen), self.n(-sen));
        write!(self.conteudo, "q\n{}BT /E{} {t} Tf {a} {b} {c2} {a} {px} {py} Tm <{hex}> Tj ET\nQ\n", cor(c, "rg"), fonte.0 + 1).unwrap();
    }

    /// Um caminho preenchido (coordenadas a partir do topo), como os contornos dos glifos de uma fórmula.
    pub fn caminho_preenchido(&mut self, caminho: &Caminho, c: Cor) {
        let mut d = String::new();
        let mut atual = None;
        for comando in &caminho.comandos {
            let (op, pontos): (&str, Vec<crate::Ponto>) = match *comando {
                Comando::Mover(p) => ("m", vec![p]),
                Comando::Linha(p) => ("l", vec![p]),
                Comando::Cubica { controle1, controle2, fim } => ("c", vec![controle1, controle2, fim]),
                Comando::Quadratica { controle, fim } => {
                    let Some(a): Option<crate::Ponto> = atual else { self.falha(ErroPdf::GeometriaInvalida); return };
                    let t = 2.0 / 3.0;
                    let c1 = crate::Ponto::novo(a.x + (controle.x - a.x) * t, a.y + (controle.y - a.y) * t);
                    let c2 = crate::Ponto::novo(fim.x + (controle.x - fim.x) * t, fim.y + (controle.y - fim.y) * t);
                    ("c", vec![c1, c2, fim])
                }
                Comando::Fechar => { d.push_str("h\n"); continue; }
            };
            for p in &pontos {
                let (x, y) = (self.n(p.x), self.n(self.altura - p.y));
                write!(d, "{x} {y} ").unwrap();
            }
            atual = pontos.last().copied();
            writeln!(d, "{op}").unwrap();
        }
        let regra = if caminho.regra == RegraPreenchimento::ParImpar { "f*" } else { "f" };
        write!(self.conteudo, "q\n{}{d}{regra}\nQ\n", cor(c, "rg")).unwrap();
    }

    /// Uma área clicável (x, y, largura, altura a partir do topo) que leva a `destino`.
    pub fn link(&mut self, x: f32, y: f32, l: f32, a: f32, destino: DestinoPdf) {
        if [x, y, l, a].iter().all(|v| v.is_finite()) && l > 0.0 && a > 0.0 {
            self.links.push(([x, y, l, a], destino));
        }
    }

    /// Texto com a linha de base em (x, y).
    pub fn texto(&mut self, x: f32, y: f32, texto: &str, fonte: FontePdf, tamanho: f32, c: Cor) {
        if !tamanho.is_finite() || tamanho <= 0.0 { self.falha(ErroPdf::EstiloInvalido); return; }
        let bytes = match codificar_winansi_modo(texto, self.substituir) {
            Ok(b) => b, Err(e) => { self.falha(e); return; }
        };
        self.extras |= fonte.indice() > 4;
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
        extras: textos.iter().any(|t| t.fonte.indice() > 4),
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
        assert_eq!(largura_texto("ab", FontePdf::Mono, 10.0), 12.0);
        assert_eq!(largura_texto("ã", FontePdf::Helvetica, 1000.0), 556.0);
        assert_eq!(DocumentoPdf::novo().finalizar(), Err(ErroPdf::DimensaoInvalida));
    }

    #[test]
    fn times_e_courier_so_entram_quando_usadas() {
        let mut so_helvetica = DocumentoPdf::novo();
        so_helvetica.adicionar_pagina(100.0, 100.0).texto(0.0, 10.0, "a", FontePdf::Helvetica, 10.0, Cor::rgb(0, 0, 0));
        let s = String::from_utf8_lossy(&so_helvetica.finalizar().unwrap()).into_owned();
        assert!(!s.contains("Times-Roman") && !s.contains("/F5"));

        let mut com_serifa = DocumentoPdf::novo();
        com_serifa.adicionar_pagina(100.0, 100.0)
            .texto(0.0, 10.0, "ação", FontePdf::Serifa, 10.0, Cor::rgb(0, 0, 0));
        com_serifa.adicionar_pagina(100.0, 100.0)
            .texto(0.0, 10.0, "x", FontePdf::MonoNegritoItalico, 10.0, Cor::rgb(0, 0, 0));
        let s = String::from_utf8_lossy(&com_serifa.finalizar().unwrap()).into_owned();
        assert!(s.contains("/BaseFont /Times-Roman") && s.contains("/BaseFont /Courier-BoldOblique"));
        assert!(s.contains("BT /F5 10 Tf") && s.contains("BT /F12 10 Tf") && s.contains("/F12 "));
    }

    #[test]
    fn links_marcadores_dados_e_fonte_invalida() {
        let mut d = DocumentoPdf::novo();
        d.metadados(MetadadosPdf { titulo: "Relatório".into(), autor: "Equipe".into(), ..Default::default() });
        for _ in 0..2 {
            d.adicionar_pagina(100.0, 200.0);
        }
        let p = d.pagina(0).unwrap();
        p.link(10.0, 20.0, 30.0, 10.0, DestinoPdf::Url("https://exemplo.org/a b".into()));
        p.link(10.0, 40.0, 30.0, 10.0, DestinoPdf::Pagina { indice: 1, y: 50.0 });
        p.link(10.0, 60.0, 30.0, 10.0, DestinoPdf::Pagina { indice: 9, y: 0.0 });
        d.marcador(1, "Introdução", 0, 0.0).marcador(2, "Objetivos", 0, 80.0).marcador(1, "Conclusão", 1, 10.0);
        assert_eq!(d.fonte(vec![0; 10]), Err(ErroPdf::FonteInvalida));
        let pdf = d.finalizar().unwrap();
        let s = String::from_utf8_lossy(&pdf).into_owned();
        assert!(s.contains("/URI (https://exemplo.org/a%20b)"));
        assert!(s.contains("/Rect [10 170 40 180]"));
        // O link para a página que não existe fica de fora.
        assert_eq!(s.matches("/Subtype /Link").count(), 2);
        assert!(s.contains("/XYZ 0 150 null]"));
        assert!(s.contains("/PageMode /UseOutlines") && s.contains("/Count -1") && s.contains("/Type /Outlines"));
        assert!(s.contains("/Title <FEFF00520065006C0061007400F300720069006F>"));
        assert!(s.contains("/Info "));
    }

    #[test]
    fn imagem_e_texto_girado_na_pagina() {
        let mut d = DocumentoPdf::novo();
        let p = d.adicionar_pagina(100.0, 100.0);
        p.imagem_rgb(10.0, 10.0, 20.0, 30.0, vec![255, 0, 0, 0, 255, 0], 2, 1);
        p.texto_girado(50.0, 50.0, 90.0, "M", FontePdf::Helvetica, 10.0, Cor::rgb(0, 0, 0));
        let s = String::from_utf8_lossy(&d.finalizar().unwrap()).into_owned();
        assert!(s.contains("/Subtype /Image /Width 2 /Height 1"));
        assert!(s.contains("/XObject << /Im1 ") && s.contains("20 0 0 30 10 60 cm\n/Im1 Do"));
        assert!(s.contains("0 1 -1 0 50 50 Tm"));

        let mut ruim = DocumentoPdf::novo();
        ruim.adicionar_pagina(10.0, 10.0).imagem_rgb(0.0, 0.0, 1.0, 1.0, vec![0; 5], 2, 1);
        assert_eq!(ruim.finalizar(), Err(ErroPdf::DimensaoInvalida));
    }
}

//! Traço de tinta como forma livre DrawingML (`a:custGeom`), o que o Excel,
//! o PowerPoint e o LibreOffice abrem como desenho editável. O consumidor
//! embrulha o resultado no elemento do seu formato (`xdr:sp`/`xdr:spPr` na
//! planilha, `p:sp`/`p:spPr` no slide) e escolhe a âncora.
//!
//! A importação lê o que esta exportação grava (e formas livres simples de
//! outros programas: `moveTo`/`lnTo`/`quadBezTo`/`cubicBezTo`); não é um
//! leitor DrawingML geral — sem XML completo, sem `a:prstGeom`, sem InkML.
use super::{Ferramenta, PontoTinta, Traco};
use crate::Cor;
use std::fmt::Write as _;

/// EMU por pixel lógico a 96 ppp (914400 EMU por polegada).
pub const EMU_POR_PIXEL: f64 = 9525.0;

/// Posição e geometria em EMU. `geometria` é o conteúdo do `spPr`
/// (`a:xfrm`, `a:custGeom`, `a:noFill`, `a:ln`), sem o elemento envolvente.
#[derive(Debug, Clone, PartialEq)]
pub struct DesenhoDrawingMl {
    pub x: i64,
    pub y: i64,
    pub cx: i64,
    pub cy: i64,
    pub geometria: String,
}

/// Exporta o traço. `emu_por_unidade` converte a unidade dos pontos
/// (ex.: [`EMU_POR_PIXEL`]). `None` se o traço for inválido.
pub fn traco_para_drawingml(traco: &Traco, emu_por_unidade: f64) -> Option<DesenhoDrawingMl> {
    if !traco.valido() || !(emu_por_unidade.is_finite() && emu_por_unidade > 0.0) {
        return None;
    }
    let caixa = traco.limites()?;
    let emu = |v: f32| (f64::from(v) * emu_por_unidade).round() as i64;
    let (x, y) = (emu(caixa.x), emu(caixa.y));
    // Linha horizontal/vertical tem caixa de espessura zero: o path precisa de w/h > 0.
    let (cx, cy) = (emu(caixa.largura).max(1), emu(caixa.altura).max(1));
    let local = |p: &PontoTinta| (emu(p.x) - x, emu(p.y) - y);
    let mut g = String::with_capacity(400 + traco.pontos.len() * 48);
    let _ = write!(
        g,
        "<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/>\
         <a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst>\
         <a:path w=\"{cx}\" h=\"{cy}\" fill=\"none\">"
    );
    let pts: Vec<(i64, i64)> = traco.pontos.iter().map(local).collect();
    let pt = |g: &mut String, (px, py): (i64, i64)| {
        let _ = write!(g, "<a:pt x=\"{px}\" y=\"{py}\"/>");
    };
    g.push_str("<a:moveTo>");
    pt(&mut g, pts[0]);
    g.push_str("</a:moveTo>");
    if pts.len() == 1 {
        g.push_str("<a:lnTo>");
        pt(&mut g, pts[0]);
        g.push_str("</a:lnTo>");
    } else if traco.reto || pts.len() == 2 {
        for &p in &pts[1..] {
            g.push_str("<a:lnTo>");
            pt(&mut g, p);
            g.push_str("</a:lnTo>");
        }
    } else {
        // Igual a `Traco::caminho`: quadráticas pelos pontos médios.
        for i in 1..pts.len() - 1 {
            let fim = ((pts[i].0 + pts[i + 1].0) / 2, (pts[i].1 + pts[i + 1].1) / 2);
            g.push_str("<a:quadBezTo>");
            pt(&mut g, pts[i]);
            pt(&mut g, fim);
            g.push_str("</a:quadBezTo>");
        }
        g.push_str("<a:lnTo>");
        pt(&mut g, pts[pts.len() - 1]);
        g.push_str("</a:lnTo>");
    }
    let alfa = (f64::from(traco.cor.a) / 255.0 * f64::from(traco.opacidade)).clamp(0.0, 1.0);
    let largura = emu(traco.espessura).max(1);
    let cap = match traco.ferramenta {
        Ferramenta::Caneta => "rnd",
        Ferramenta::MarcaTexto => "sq",
    };
    let _ = write!(
        g,
        "</a:path></a:pathLst></a:custGeom><a:noFill/>\
         <a:ln w=\"{largura}\" cap=\"{cap}\"><a:solidFill><a:srgbClr val=\"{:02X}{:02X}{:02X}\">\
         <a:alpha val=\"{}\"/></a:srgbClr></a:solidFill><a:round/></a:ln>",
        traco.cor.r,
        traco.cor.g,
        traco.cor.b,
        (alfa * 100_000.0).round() as i64
    );
    Some(DesenhoDrawingMl { x, y, cx, cy, geometria: g })
}

/// Como [`traco_para_drawingml`], mas a caneta com pressão sai como o
/// **contorno de largura variável preenchido** (o mesmo polígono da tela,
/// do PDF e do SVG, de [`super::contorno_largura_variavel`]) em vez de uma
/// linha de espessura fixa. Traço sem pressão (ou marca-texto) = igual a
/// [`traco_para_drawingml`]. A leitura ([`traco_de_drawingml`]) não
/// reconstrói a linha central desse contorno.
pub fn traco_para_drawingml_com_pressao(
    traco: &Traco,
    emu_por_unidade: f64,
    curva: super::CurvaPressao,
) -> Option<DesenhoDrawingMl> {
    if !traco.largura_variavel() {
        return traco_para_drawingml(traco, emu_por_unidade);
    }
    if !traco.valido() || !(emu_por_unidade.is_finite() && emu_por_unidade > 0.0) {
        return None;
    }
    let poligonos = traco.contorno(curva);
    let caixa = super::limites_de(poligonos.iter().flatten().copied())?;
    let emu = |v: f32| (f64::from(v) * emu_por_unidade).round() as i64;
    let (x, y) = (emu(caixa.x), emu(caixa.y));
    let (cx, cy) = (emu(caixa.largura).max(1), emu(caixa.altura).max(1));
    let total: usize = poligonos.iter().map(Vec::len).sum();
    let mut g = String::with_capacity(400 + total * 40);
    let _ = write!(
        g,
        "<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/>\
         <a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst>"
    );
    // Um `a:path` por pedaço: cada um é preenchido sozinho e os pedaços se
    // somam, sem depender da regra de preenchimento do leitor.
    for pol in poligonos.iter().filter(|p| p.len() >= 3) {
        let _ = write!(g, "<a:path w=\"{cx}\" h=\"{cy}\" stroke=\"0\">");
        for (i, p) in pol.iter().enumerate() {
            let tag = if i == 0 { "moveTo" } else { "lnTo" };
            let _ = write!(g, "<a:{tag}><a:pt x=\"{}\" y=\"{}\"/></a:{tag}>", emu(p.x) - x, emu(p.y) - y);
        }
        g.push_str("<a:close/></a:path>");
    }
    let alfa = (f64::from(traco.cor.a) / 255.0 * f64::from(traco.opacidade)).clamp(0.0, 1.0);
    let _ = write!(
        g,
        "</a:pathLst></a:custGeom><a:solidFill><a:srgbClr val=\"{:02X}{:02X}{:02X}\">\
         <a:alpha val=\"{}\"/></a:srgbClr></a:solidFill><a:ln><a:noFill/></a:ln>",
        traco.cor.r,
        traco.cor.g,
        traco.cor.b,
        (alfa * 100_000.0).round() as i64
    );
    Some(DesenhoDrawingMl { x, y, cx, cy, geometria: g })
}

/// Etiquetas `<a:…>` do trecho, com nome e texto dos atributos.
fn etiquetas(xml: &str) -> impl Iterator<Item = (&str, &str, bool)> {
    xml.split('<').skip(1).filter_map(|parte| {
        let fim = parte.find('>')?;
        let corpo = &parte[..fim];
        let fecha = corpo.starts_with('/');
        let corpo = corpo.trim_start_matches('/').trim_end_matches('/');
        let (nome, atrs) = corpo.split_once(char::is_whitespace).unwrap_or((corpo, ""));
        Some((nome, atrs, fecha))
    })
}

fn atributo<'a>(atrs: &'a str, nome: &str) -> Option<&'a str> {
    let mut resto = atrs;
    while let Some(i) = resto.find(nome) {
        let antes_ok = i == 0 || resto.as_bytes()[i - 1].is_ascii_whitespace();
        let depois = &resto[i + nome.len()..];
        if antes_ok {
            if let Some(v) = depois.strip_prefix("=\"") {
                return v.split('"').next();
            }
        }
        resto = depois;
    }
    None
}

/// Lê de volta um traço exportado por [`traco_para_drawingml`]. A cor, a
/// largura da linha e a opacidade vêm do `a:ln`; marca-texto é reconhecido
/// pela ponta quadrada. `None` sem `a:custGeom` com pontos.
pub fn traco_de_drawingml(xml: &str, emu_por_unidade: f64) -> Option<Traco> {
    if !(emu_por_unidade.is_finite() && emu_por_unidade > 0.0) {
        return None;
    }
    let (mut off, mut ext) = ((0f64, 0f64), (0f64, 0f64));
    let (mut path_w, mut path_h) = (0f64, 0f64);
    let mut comando = "";
    let mut pts_comando: Vec<(f64, f64)> = Vec::new();
    let mut pontos: Vec<(f64, f64)> = Vec::new();
    let mut tem_quad = false;
    let mut dentro_ln = false;
    let mut cor = Cor::rgb(0, 0, 0);
    let mut alfa = 1.0f64;
    let (mut largura, mut cap_quadrado) = (None, false);
    let num = |a: &str, n: &str| atributo(a, n).and_then(|v| v.parse::<f64>().ok());
    for (nome, atrs, fecha) in etiquetas(xml) {
        let nome = nome.rsplit(':').next().unwrap_or(nome);
        match (nome, fecha) {
            ("off", false) => off = (num(atrs, "x")?, num(atrs, "y")?),
            ("ext", false) => {
                // `a:ext` também aparece em `a:extLst` (com `uri`, sem cx/cy).
                if let (Some(cx), Some(cy)) = (num(atrs, "cx"), num(atrs, "cy")) {
                    ext = (cx, cy);
                }
            }
            ("path", false) => {
                path_w = num(atrs, "w").unwrap_or(0.0);
                path_h = num(atrs, "h").unwrap_or(0.0);
            }
            ("moveTo" | "lnTo" | "quadBezTo" | "cubicBezTo", false) => {
                comando = nome;
                pts_comando.clear();
            }
            ("moveTo" | "lnTo" | "quadBezTo" | "cubicBezTo", true) => {
                match comando {
                    "moveTo" | "lnTo" => pontos.extend(pts_comando.first().copied()),
                    // Quadrática pelos pontos médios: o controle é o ponto original.
                    "quadBezTo" => {
                        tem_quad = true;
                        pontos.extend(pts_comando.first().copied());
                    }
                    _ => pontos.extend(pts_comando.last().copied()),
                }
                comando = "";
            }
            ("pt", false) if !comando.is_empty() => {
                pts_comando.push((num(atrs, "x")?, num(atrs, "y")?));
            }
            ("ln", false) => {
                dentro_ln = true;
                largura = num(atrs, "w");
                cap_quadrado = atributo(atrs, "cap") == Some("sq");
            }
            ("ln", true) => dentro_ln = false,
            ("srgbClr", false) if dentro_ln => {
                let v = atributo(atrs, "val")?;
                let n = u32::from_str_radix(v, 16).ok()?;
                cor = Cor::rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
            }
            ("alpha", false) if dentro_ln => {
                alfa = (num(atrs, "val")? / 100_000.0).clamp(0.0, 1.0);
            }
            _ => {}
        }
    }
    // A última lnTo das quadráticas repete o fim: tirar duplicata colada.
    pontos.dedup();
    if pontos.is_empty() {
        return None;
    }
    let (sx, sy) = (
        if path_w > 0.0 { ext.0 / path_w } else { 1.0 },
        if path_h > 0.0 { ext.1 / path_h } else { 1.0 },
    );
    let unidade = |v: f64| (v / emu_por_unidade) as f32;
    let pontos: Vec<PontoTinta> = pontos
        .into_iter()
        .map(|(x, y)| PontoTinta::novo(unidade(off.0 + x * sx), unidade(off.1 + y * sy)))
        .collect();
    let ferramenta = if cap_quadrado { Ferramenta::MarcaTexto } else { Ferramenta::Caneta };
    let traco = Traco {
        id: 0,
        pontos,
        cor,
        espessura: largura.map_or(1.0, unidade).max(0.01),
        ferramenta,
        opacidade: alfa as f32,
        reto: !tem_quad,
        pressao_sintetica: false,
    };
    traco.valido().then_some(traco)
}

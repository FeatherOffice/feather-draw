//! DRW-012: snapshots semânticos determinísticos de uma cena comum.
//! SVG/PDF são confrontados com corpus versionado; Iced usa o mesmo Viewport.
//! Não são screenshots de GPU: diferenças de rasterização são aceitas apenas
//! dentro de tolerâncias documentadas em fixtures/drw012/README.md.
use feather_draw::{
    exportar_pdf, exportar_svg, Cena, Cor, Estilo, Figura, Ponto, Retangulo, Unidade, Viewport,
};

const SVG_GOLDEN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"), "/fixtures/drw012/snapshot.svg"
));
const PDF_GOLDEN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"), "/fixtures/drw012/pdf-comandos.txt"
));
const ICED_GOLDEN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"), "/fixtures/drw012/iced-coordenadas.csv"
));

pub fn cena() -> Cena {
    let mut cena = Cena::nova(160.0, 100.0);
    cena.adicionar(Figura::Retangulo {
        area: Retangulo::novo(10.0, 10.0, 70.0, 35.0),
        estilo: Estilo {
            preenchimento: Some(Cor::rgb(255, 0, 0)),
            contorno: None,
            espessura: 1.0,
        },
    });
    // Retângulos parcialmente sobrepostos tornam z-order observável.
    cena.adicionar(Figura::Retangulo {
        area: Retangulo::novo(35.0, 25.0, 80.0, 45.0),
        estilo: Estilo {
            preenchimento: Some(Cor::rgb(0, 0, 255)),
            contorno: None,
            espessura: 1.0,
        },
    });
    cena
}

pub fn viewport() -> Viewport {
    Viewport::de_escala_logica(
        Unidade::Ponto,
        1.5, // px lógicos por ponto, sem aplicar DPR duas vezes.
        Ponto::novo(0.0, 0.0),
        Ponto::novo(3.0, 5.0),
        2.0,
    )
    .expect("viewport conhecido é finito")
}

pub fn comandos_pdf(pdf: &[u8]) -> Result<&str, String> {
    if !pdf.starts_with(b"%PDF-1.4\n") {
        return Err("PDF sem assinatura 1.4".into());
    }
    let prefixo = b"stream\n";
    let inicio = pdf.windows(prefixo.len())
        .position(|janela| janela == prefixo)
        .ok_or("stream PDF ausente")?
        + prefixo.len();
    let corpo = &pdf[inicio..];
    let fim = corpo.windows(b"endstream".len())
        .position(|janela| janela == b"endstream")
        .ok_or("fim do stream PDF ausente")?;
    std::str::from_utf8(&corpo[..fim]).map_err(|e| format!("stream PDF UTF-8: {e}"))
}

fn perto(obtido: f32, esperado: f32, tolerancia: f32) -> Result<(), String> {
    if !obtido.is_finite() || (obtido - esperado).abs() > tolerancia {
        return Err(format!(
            "coordenada fora de tolerância: obtido={obtido}; esperado={esperado}; ±{tolerancia}"
        ));
    }
    Ok(())
}

pub fn comparar_coordenadas_iced() -> Result<(), String> {
    let vp = viewport();
    let mut verificadas = 0;
    for linha in ICED_GOLDEN.lines().skip(1) {
        if linha.trim().is_empty() {
            continue;
        }
        let campos = linha.split(',')
            .map(|parte| parte.parse::<f32>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        if campos.len() != 6 {
            return Err(format!("esperados 6 números para {linha:?}"));
        }
        let documento = Ponto::novo(campos[0], campos[1]);
        let logico = vp.logico(documento);
        let fisico = vp.fisico(documento);
        perto(logico.x, campos[2], 0.25)?;
        perto(logico.y, campos[3], 0.25)?;
        perto(fisico.x, campos[4], 0.5)?;
        perto(fisico.y, campos[5], 0.5)?;
        perto(vp.documento(logico).x, documento.x, 0.001)?;
        perto(vp.documento(logico).y, documento.y, 0.001)?;
        verificadas += 1;
    }
    if verificadas != 4 {
        return Err(format!("snapshot Iced incompleto: {verificadas} âncoras"));
    }
    Ok(())
}

/// Compara o desenho neutro nos dois formatos e âncoras do mesmo Viewport Iced.
/// Retorna os arquivos para que o exemplo produza artefatos inspecionáveis.
pub fn verificar() -> Result<(String, Vec<u8>), String> {
    let cena = cena();
    let svg = exportar_svg(&cena).map_err(|e| format!("SVG: {e:?}"))?;
    let pdf = exportar_pdf(&cena).map_err(|e| format!("PDF: {e:?}"))?;
    if svg != SVG_GOLDEN {
        return Err("SVG divergiu do snapshot dourado: verificar geometria, cores e ordem z".into());
    }
    let comandos = comandos_pdf(&pdf)?;
    if comandos != PDF_GOLDEN {
        return Err("PDF divergiu do snapshot dourado: comandos vetoriais/ordem z".into());
    }
    if !String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 160 100]") {
        return Err("dimensões PDF divergentes da Cena".into());
    }
    comparar_coordenadas_iced()?;
    Ok((svg, pdf))
}

#[test]
fn drw012_snapshot_svg_pdf_coordenadas_iced() {
    verificar().expect("regressão cross-backend");
}

#[test]
fn drw012_pdf_e_svg_deterministicamente_estaveis() {
    let (svg_a, pdf_a) = verificar().unwrap();
    let (svg_b, pdf_b) = verificar().unwrap();
    assert_eq!(svg_a, svg_b);
    assert_eq!(pdf_a, pdf_b);
}

#[test]
fn drw012_dpr_nao_muda_coordenadas_logicas() {
    let vp = viewport();
    let outro = Viewport::de_escala_logica(
        Unidade::Ponto, vp.escala(), vp.pan, vp.origem, 3.0,
    ).unwrap();
    let p = Ponto::novo(55.0, 30.0);
    assert_eq!(vp.logico(p), outro.logico(p));
    assert_ne!(vp.fisico(p), outro.fisico(p));
}

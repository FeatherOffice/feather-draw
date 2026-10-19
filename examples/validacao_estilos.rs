//! DRW-006: demonstra stroke, dash, opacidade, gradiente e metadados de texto.
//! cargo run --example validacao_estilos -- /tmp/featheroffice-estilos
use feather_draw::{
    exportar_pdf_com_texto, exportar_svg, Cena, Cor, EstiloTexto, EstiloVetor,
    Extremo, Figura, FontePdf, Juncao, Parada, Ponto, Preenchimento, Stroke, TextoPdf,
};
use std::{env, fs, path::PathBuf};

fn executar() -> Result<(), String> {
    let pasta = PathBuf::from(
        env::args().nth(1).unwrap_or_else(|| "/tmp/featheroffice-estilos".into()),
    );
    fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;

    let mut stroke = Stroke::novo(Cor::rgba(10, 30, 80, 230), 3.5);
    stroke.extremo = Extremo::Redondo;
    stroke.juncao = Juncao::Redonda;
    stroke.dash = vec![8.0, 3.0, 2.0, 3.0];
    stroke.fase = 1.0;

    // O PDF 1.4 aceita alfa constante ao longo do gradiente. Gradientes
    // com alfa variável são recusados explicitamente (sem perda silenciosa).
    let gradiente = Preenchimento::Linear {
        de: Ponto::novo(20.0, 25.0),
        ate: Ponto::novo(260.0, 25.0),
        paradas: vec![
            Parada { posicao: 0.0, cor: Cor::rgba(16, 132, 180, 200) },
            Parada { posicao: 0.5, cor: Cor::rgba(235, 199, 52, 200) },
            Parada { posicao: 0.5, cor: Cor::rgba(245, 110, 30, 200) },
            Parada { posicao: 1.0, cor: Cor::rgba(142, 40, 178, 200) },
        ],
    };
    let texto = EstiloTexto {
        familia: "Helvetica".into(),
        tamanho: 14.0,
        italico: true,
        ..Default::default()
    };
    let estilo = EstiloVetor {
        preenchimento: Some(gradiente.clone()),
        stroke: Some(stroke),
        opacidade: 0.8,
        texto: Some(texto.clone()),
    };
    if !estilo.valido() {
        return Err("estilo inválido".into());
    }
    assert_eq!(gradiente.amostrar_normalizada(0.5), Cor::rgba(245, 110, 30, 200));

    let mut cena = Cena::nova(300.0, 135.0);
    cena.adicionar(Figura::Elipse {
        centro: Ponto::novo(150.0, 64.0),
        raio_x: 125.0,
        raio_y: 42.0,
        estilo,
    });

    let svg = exportar_svg(&cena).map_err(|e| format!("SVG: {e:?}"))?;
    let pdf = exportar_pdf_com_texto(&cena, &[TextoPdf {
        texto: "Estilos vetoriais FeatherWorks".into(),
        x: 40.0,
        y: 125.0,
        tamanho: texto.tamanho,
        cor: Cor::rgb(30, 30, 30),
        fonte: FontePdf::Italico,
    }]).map_err(|e| format!("PDF: {e:?}"))?;

    let svg_saida = pasta.join("estilos.svg");
    let pdf_saida = pasta.join("estilos.pdf");
    fs::write(&svg_saida, svg).map_err(|e| e.to_string())?;
    fs::write(&pdf_saida, pdf).map_err(|e| e.to_string())?;
    println!("SVG: {}", svg_saida.display());
    println!("PDF: {}", pdf_saida.display());
    Ok(())
}
fn main() {
    if let Err(erro) = executar() {
        eprintln!("{erro}");
        std::process::exit(1);
    }
}

//! DRW-012: gera SVG/PDF reais a partir da mesma Cena, valida snapshots dourados.
//! cargo run
//!   --example drw012_snapshots -- /tmp/featheroffice-drw012
#[path = "../tests/drw012_snapshots.rs"]
mod snapshots;
use std::{env, fs, path::PathBuf};

fn executar() -> Result<(), String> {
    let saida = PathBuf::from(
        env::args().nth(1).unwrap_or_else(|| "/tmp/featheroffice-drw012".into()),
    );
    fs::create_dir_all(&saida).map_err(|e| e.to_string())?;
    let (svg, pdf) = snapshots::verificar()?;
    let comandos = snapshots::comandos_pdf(&pdf)?.to_owned();
    let svg_saida = saida.join("snapshot.svg");
    let pdf_saida = saida.join("snapshot.pdf");
    let txt_saida = saida.join("pdf-comandos.txt");
    fs::write(&svg_saida, svg.as_bytes()).map_err(|e| e.to_string())?;
    fs::write(&pdf_saida, &pdf).map_err(|e| e.to_string())?;
    fs::write(&txt_saida, comandos).map_err(|e| e.to_string())?;
    if fs::read(&svg_saida).map_err(|e| e.to_string())? != svg.as_bytes()
        || fs::read(&pdf_saida).map_err(|e| e.to_string())? != pdf
    {
        return Err("bytes persistidos em disco divergem do render produzido".into());
    }
    println!(
        "DRW-012 OK: cena com 2 formas; SVG={}, PDF={}, stream={} (goldens verificados)",
        svg_saida.display(), pdf_saida.display(), txt_saida.display()
    );
    Ok(())
}
fn main() {
    if let Err(e) = executar() {
        eprintln!("DRW-012 FALHOU: {e}");
        std::process::exit(1);
    }
}

use super::*;
use std::f32::consts::TAU;

/// Tremido determinístico (sem crate de aleatório): soma de senos.
fn tremor(i: usize, amplitude: f32) -> (f32, f32) {
    let t = i as f32;
    (
        amplitude * ((t * 1.7).sin() * 0.6 + (t * 0.37).sin() * 0.4),
        amplitude * ((t * 2.3).cos() * 0.5 + (t * 0.53).sin() * 0.5),
    )
}

/// Polilinha pelos vértices, com `n` pontos por lado e tremido.
fn tracado(vertices: &[Ponto], n: usize, amplitude: f32) -> Vec<Ponto> {
    let mut saida = Vec::new();
    let mut i = 0;
    for s in vertices.windows(2) {
        for k in 0..n {
            let t = k as f32 / n as f32;
            let (dx, dy) = tremor(i, amplitude);
            i += 1;
            saida.push(Ponto::novo(
                s[0].x + (s[1].x - s[0].x) * t + dx,
                s[0].y + (s[1].y - s[0].y) * t + dy,
            ));
        }
    }
    let ultimo = vertices[vertices.len() - 1];
    saida.push(ultimo);
    saida
}

fn traco_de(pontos: &[Ponto]) -> Traco {
    Traco::novo(
        pontos.iter().copied().map(PontoTinta::from).collect(),
        Cor::rgb(200, 30, 30),
        3.0,
        Ferramenta::Caneta,
    )
}

fn perto(a: Ponto, b: Ponto, tol: f32) -> bool {
    distancia(a, b) <= tol
}

#[test]
fn simplificacao_reduz_pontos_sem_mudar_o_desenho() {
    // Mil pontos numa curva suave (seno) com tremido miúdo.
    let original: Vec<PontoTinta> = (0..1000)
        .map(|i| {
            let x = i as f32 * 0.5;
            let (dx, dy) = tremor(i, 0.15);
            PontoTinta::novo(x + dx, 40.0 * (x / 80.0).sin() + dy)
                .com_pressao(0.5)
                .com_tempo(i as u32 * 8)
        })
        .collect();
    let mut traco = Traco::novo(original.clone(), Cor::rgb(0, 0, 0), 2.0, Ferramenta::Caneta);
    traco.preparar(0.5);
    assert!(traco.pontos.len() < 80, "ficaram {} pontos", traco.pontos.len());
    assert!(traco.pontos.len() >= 10);
    // Pontas mantidas, e pressão/tempo acompanham.
    assert_eq!(traco.pontos[0], original[0]);
    assert_eq!(traco.pontos.last().unwrap().ponto(), original[999].ponto());
    assert!(traco.pontos.iter().all(|p| p.pressao.is_some() && p.tempo.is_some()));
    // Todo ponto original fica perto do traço simplificado.
    for p in original.iter().step_by(7) {
        assert!(traco.distancia_ate(p.ponto()) < 1.2, "{p:?}");
    }
}

#[test]
fn douglas_peucker_numa_reta_fica_com_as_pontas() {
    let reta: Vec<PontoTinta> = (0..50).map(|i| PontoTinta::novo(i as f32, 2.0 * i as f32)).collect();
    let s = simplificar(&reta, 0.01);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0], reta[0]);
    assert_eq!(s[1], reta[49]);
}

#[test]
fn ids_sao_estaveis_e_remover_nao_reaproveita() {
    let mut tinta = Tinta::nova();
    let a = tinta.adicionar(traco_de(&[Ponto::novo(0., 0.), Ponto::novo(10., 0.)])).unwrap();
    let b = tinta.adicionar(traco_de(&[Ponto::novo(0., 5.), Ponto::novo(10., 5.)])).unwrap();
    assert_ne!(a, b);
    tinta.remover(a);
    let c = tinta.adicionar(traco_de(&[Ponto::novo(0., 9.), Ponto::novo(10., 9.)])).unwrap();
    assert!(c > b);
    assert!(tinta.obter(b).is_some());
    assert!(tinta.adicionar(traco_de(&[])).is_none(), "traço vazio é recusado");
}

#[test]
fn borracha_de_traco_apaga_o_de_cima() {
    let mut tinta = Tinta::nova();
    let baixo = tinta.adicionar(traco_de(&[Ponto::novo(0., 0.), Ponto::novo(100., 0.)])).unwrap();
    let cima = tinta.adicionar(traco_de(&[Ponto::novo(50., -50.), Ponto::novo(50., 50.)])).unwrap();
    assert_eq!(tinta.apagar_traco_em(Ponto::novo(50., 1.), 2.0).map(|t| t.id), Some(cima));
    assert_eq!(tinta.len(), 1);
    assert!(tinta.apagar_traco_em(Ponto::novo(50., 30.), 2.0).is_none());
    assert_eq!(tinta.tracos()[0].id, baixo);
    // Arrastada: toca no traço que sobrou.
    let apagados = tinta.apagar_tracos_no_caminho(&[Ponto::novo(20., -30.), Ponto::novo(20., 30.)], 1.0);
    assert_eq!(apagados.len(), 1);
    assert!(tinta.is_empty());
}

#[test]
fn borracha_de_ponto_divide_o_traco() {
    let mut tinta = Tinta::nova();
    let id = tinta
        .adicionar(traco_de(&[Ponto::novo(0., 0.), Ponto::novo(50., 0.), Ponto::novo(100., 0.)]))
        .unwrap();
    let outro = tinta.adicionar(traco_de(&[Ponto::novo(0., 80.), Ponto::novo(100., 80.)])).unwrap();
    assert!(tinta.apagar_pontos_em(Ponto::novo(50., 0.), 10.0));
    // Dois pedaços: [0..40] mantém o id, [60..100] ganha um novo; o outro intacto.
    assert_eq!(tinta.len(), 3);
    let primeiro = tinta.obter(id).unwrap();
    assert_eq!(primeiro.pontos.first().unwrap().ponto(), Ponto::novo(0., 0.));
    assert!(perto(primeiro.pontos.last().unwrap().ponto(), Ponto::novo(40., 0.), 0.01));
    let segundo = &tinta.tracos()[1];
    assert_ne!(segundo.id, id);
    assert_ne!(segundo.id, outro);
    assert!(perto(segundo.pontos[0].ponto(), Ponto::novo(60., 0.), 0.01));
    assert_eq!(segundo.pontos.last().unwrap().ponto(), Ponto::novo(100., 0.));
    assert_eq!(tinta.tracos()[2].id, outro);
    // Disco que não toca nada não muda nada.
    let antes = tinta.clone();
    assert!(!tinta.apagar_pontos_em(Ponto::novo(500., 500.), 10.0));
    assert_eq!(tinta, antes);
    // Apagar a ponta só encurta; apagar o pedaço inteiro remove.
    assert!(tinta.apagar_pontos_em(Ponto::novo(0., 0.), 5.0));
    assert!(perto(tinta.obter(id).unwrap().pontos[0].ponto(), Ponto::novo(5., 0.), 0.01));
    assert!(tinta.apagar_pontos_no_caminho(&[Ponto::novo(0., 0.), Ponto::novo(45., 0.)], 6.0));
    assert!(tinta.obter(id).is_none());
}

#[test]
fn laco_seleciona_so_os_tracos_cercados() {
    let mut tinta = Tinta::nova();
    let dentro = tinta.adicionar(traco_de(&tracado(&[Ponto::novo(10., 10.), Ponto::novo(30., 20.)], 10, 0.5))).unwrap();
    let metade = tinta.adicionar(traco_de(&[Ponto::novo(40., 10.), Ponto::novo(90., 10.)])).unwrap();
    let fora = tinta.adicionar(traco_de(&[Ponto::novo(200., 200.), Ponto::novo(210., 210.)])).unwrap();
    // Laço torto em volta de (0,0)-(50,40).
    let laco: Vec<Ponto> = (0..40)
        .map(|i| {
            let a = TAU * i as f32 / 40.0;
            let (dx, dy) = tremor(i, 1.0);
            Ponto::novo(25.0 + 30.0 * a.cos() + dx, 18.0 + 22.0 * a.sin() + dy)
        })
        .collect();
    let escolhidos = tinta.selecionar_laco(&laco);
    assert_eq!(escolhidos, vec![dentro]);
    assert!(!escolhidos.contains(&metade) && !escolhidos.contains(&fora));
    assert!(ponto_no_poligono(Ponto::novo(25., 18.), &laco));
    assert!(!ponto_no_poligono(Ponto::novo(80., 18.), &laco));
    assert!(tinta.selecionar_laco(&laco[..2]).is_empty());
}

#[test]
fn reconhece_linha_torta() {
    let pts = tracado(&[Ponto::novo(5., 5.), Ponto::novo(205., 65.)], 60, 1.5);
    match reconhecer_forma(&pts) {
        Some(Forma::Linha { de, ate }) => {
            assert!(perto(de, Ponto::novo(5., 5.), 3.0));
            assert!(perto(ate, Ponto::novo(205., 65.), 0.01));
        }
        outra => panic!("esperava linha, veio {outra:?}"),
    }
}

#[test]
fn reconhece_seta_num_traco_so() {
    // Haste da esquerda para a direita, depois aba de cima, volta à ponta, aba de baixo.
    let pts = tracado(
        &[
            Ponto::novo(0., 100.),
            Ponto::novo(200., 100.),
            Ponto::novo(170., 82.),
            Ponto::novo(200., 100.),
            Ponto::novo(170., 120.),
        ],
        25,
        1.2,
    );
    match reconhecer_forma(&pts) {
        Some(Forma::Seta { de, ate, ponta }) => {
            assert!(perto(de, Ponto::novo(0., 100.), 4.0), "{de:?}");
            assert!(perto(ate, Ponto::novo(200., 100.), 6.0), "{ate:?}");
            assert!((20.0..50.0).contains(&ponta), "{ponta}");
        }
        outra => panic!("esperava seta, veio {outra:?}"),
    }
    // Cabeça pequena (~10 % da haste), como na captura da aba Desenhar.
    let curta = tracado(
        &[
            Ponto::novo(80., 260.),
            Ponto::novo(380., 260.),
            Ponto::novo(352., 242.),
            Ponto::novo(378., 259.),
            Ponto::novo(352., 278.),
        ],
        20,
        0.8,
    );
    assert!(matches!(reconhecer_forma(&curta), Some(Forma::Seta { .. })), "{:?}", reconhecer_forma(&curta));
    // Um "V" aberto não é seta.
    let v = tracado(&[Ponto::novo(0., 0.), Ponto::novo(50., 80.), Ponto::novo(100., 0.)], 25, 1.0);
    assert!(!matches!(reconhecer_forma(&v), Some(Forma::Seta { .. })));
}

#[test]
fn reconhece_retangulo_torto_e_alinha_aos_eixos() {
    let cantos = [
        Ponto::novo(20., 30.),
        Ponto::novo(220., 33.),
        Ponto::novo(218., 130.),
        Ponto::novo(22., 128.),
        Ponto::novo(24., 34.),
    ];
    let pts = tracado(&cantos, 40, 1.8);
    match reconhecer_forma(&pts) {
        Some(Forma::Retangulo { cantos }) => {
            // Inclinação de ~1° vira retângulo alinhado.
            assert!((cantos[0].y - cantos[1].y).abs() < 0.01);
            assert!((cantos[1].x - cantos[2].x).abs() < 0.01);
            let largura = distancia(cantos[0], cantos[1]);
            let altura = distancia(cantos[1], cantos[2]);
            assert!((185.0..215.0).contains(&largura), "{largura}");
            assert!((85.0..110.0).contains(&altura), "{altura}");
        }
        outra => panic!("esperava retângulo, veio {outra:?}"),
    }
}

#[test]
fn reconhece_retangulo_girado() {
    let (s, c) = 30f32.to_radians().sin_cos();
    let gira = |x: f32, y: f32| Ponto::novo(100.0 + x * c - y * s, 100.0 + x * s + y * c);
    let v = [gira(-80., -40.), gira(80., -40.), gira(80., 40.), gira(-80., 40.), gira(-78., -38.)];
    match reconhecer_forma(&tracado(&v, 30, 1.0)) {
        Some(Forma::Retangulo { cantos }) => {
            let ang = (cantos[1].y - cantos[0].y).atan2(cantos[1].x - cantos[0].x).to_degrees();
            assert!((ang - 30.0).abs() < 4.0 || (ang + 60.0).abs() < 4.0 || (ang - 120.0).abs() < 4.0 || (ang + 150.0).abs() < 4.0, "{ang}");
        }
        outra => panic!("esperava retângulo girado, veio {outra:?}"),
    }
}

#[test]
fn reconhece_triangulo() {
    let v = [
        Ponto::novo(100., 10.),
        Ponto::novo(190., 160.),
        Ponto::novo(12., 158.),
        Ponto::novo(97., 14.),
    ];
    match reconhecer_forma(&tracado(&v, 35, 1.5)) {
        Some(Forma::Triangulo { vertices }) => {
            for alvo in [Ponto::novo(100., 10.), Ponto::novo(190., 160.), Ponto::novo(12., 158.)] {
                assert!(vertices.iter().any(|&p| perto(p, alvo, 10.0)), "{alvo:?} em {vertices:?}");
            }
        }
        outra => panic!("esperava triângulo, veio {outra:?}"),
    }
}

#[test]
fn reconhece_circulo_e_elipse() {
    let circulo: Vec<Ponto> = (0..=110)
        .map(|i| {
            let a = TAU * i as f32 / 100.0; // passa um pouco do início, como à mão
            let (dx, dy) = tremor(i, 1.5);
            Ponto::novo(150.0 + 60.0 * a.cos() + dx, 120.0 + 60.0 * a.sin() + dy)
        })
        .collect();
    match reconhecer_forma(&circulo) {
        Some(Forma::Circulo { centro, raio }) => {
            assert!(perto(centro, Ponto::novo(150., 120.), 4.0), "{centro:?}");
            assert!((raio - 60.0).abs() < 4.0, "{raio}");
        }
        outra => panic!("esperava círculo, veio {outra:?}"),
    }
    let elipse: Vec<Ponto> = (0..=95)
        .map(|i| {
            let a = TAU * i as f32 / 96.0;
            let (dx, dy) = tremor(i, 1.2);
            Ponto::novo(200.0 + 120.0 * a.cos() + dx, 80.0 + 50.0 * a.sin() + dy)
        })
        .collect();
    match reconhecer_forma(&elipse) {
        Some(Forma::Elipse { centro, raio_x, raio_y, rotacao }) => {
            assert!(perto(centro, Ponto::novo(200., 80.), 4.0));
            assert!((raio_x - 120.0).abs() < 8.0, "{raio_x}");
            assert!((raio_y - 50.0).abs() < 6.0, "{raio_y}");
            assert_eq!(rotacao, 0.0);
        }
        outra => panic!("esperava elipse, veio {outra:?}"),
    }
}

#[test]
fn rabisco_nao_vira_forma_e_quadrado_nao_vira_circulo() {
    let rabisco: Vec<Ponto> = (0..120)
        .map(|i| {
            let t = i as f32 / 10.0;
            Ponto::novo(t * 15.0 + 20.0 * (t * 3.1).sin(), 30.0 * (t * 1.7).cos() + 10.0 * (t * 4.3).sin())
        })
        .collect();
    assert_eq!(reconhecer_forma(&rabisco), None);
    let quadrado = tracado(
        &[Ponto::novo(0., 0.), Ponto::novo(100., 0.), Ponto::novo(100., 100.), Ponto::novo(0., 100.), Ponto::novo(0., 2.)],
        30,
        0.8,
    );
    assert!(matches!(reconhecer_forma(&quadrado), Some(Forma::Retangulo { .. })));
    assert_eq!(reconhecer_forma(&[Ponto::novo(1., 1.)]), None);
}

#[test]
fn tinta_para_forma_troca_os_pontos_e_mantem_id() {
    let mut tinta = Tinta::nova();
    let id = tinta
        .adicionar(traco_de(&tracado(&[Ponto::novo(0., 0.), Ponto::novo(100., 50.)], 30, 0.8)))
        .unwrap();
    assert!(matches!(tinta.converter_em_forma(id), Some(Forma::Linha { .. })));
    let t = tinta.obter(id).unwrap();
    assert_eq!(t.pontos.len(), 2);
    assert!(t.reto);
    assert_eq!(t.cor, Cor::rgb(200, 30, 30));
}

#[test]
fn caminho_suave_reto_e_figura() {
    let mut t = traco_de(&[Ponto::novo(0., 0.), Ponto::novo(10., 10.), Ponto::novo(20., 0.), Ponto::novo(30., 10.)]);
    let c = t.caminho();
    assert_eq!(c.comandos.len(), 4);
    assert!(matches!(c.comandos[1], Comando::Quadratica { .. }));
    assert_eq!(c.comandos[3], Comando::Linha(Ponto::novo(30., 10.)));
    t.reto = true;
    assert!(t.caminho().comandos[1..].iter().all(|c| matches!(c, Comando::Linha(_))));
    // Toque único vira ponto desenhável.
    let ponto = traco_de(&[Ponto::novo(5., 5.)]);
    assert_eq!(ponto.caminho().comandos.len(), 2);
    match t.figura() {
        Figura::Caminho { estilo, .. } => {
            let s = estilo.stroke.unwrap();
            assert_eq!(s.largura, 3.0);
            assert_eq!(s.extremo, Extremo::Redondo);
            assert!(estilo.preenchimento.is_none());
        }
        _ => panic!("tinta vira Figura::Caminho"),
    }
    // A figura passa pelo exportador SVG existente.
    let mut cena = crate::Cena::nova(40., 20.);
    cena.adicionar(t.figura());
    let mut marca = traco_de(&[Ponto::novo(0., 5.), Ponto::novo(40., 5.)]);
    marca.ferramenta = Ferramenta::MarcaTexto;
    marca.opacidade = 0.4;
    cena.adicionar(marca.figura());
    let svg = crate::exportar_svg(&cena).unwrap();
    assert!(svg.contains("<path"));
}

#[test]
fn transformar_escala_pontos_e_espessura() {
    let t = traco_de(&[Ponto::novo(1., 2.), Ponto::novo(3., 4.)]);
    let g = t.transformar(Transformacao::escalar(2.0, 2.0));
    assert_eq!(g.pontos[1].ponto(), Ponto::novo(6., 8.));
    assert_eq!(g.espessura, 6.0);
}

#[test]
fn serializacao_ida_e_volta() {
    let mut tinta = Tinta::nova();
    let mut caneta = Traco::novo(
        vec![
            PontoTinta::novo(1.25, 2.5).com_pressao(0.5).com_tempo(0),
            PontoTinta::novo(-3.0, 4.25).com_tempo(16),
            PontoTinta::novo(10.0, 20.0).com_pressao(1.0),
            PontoTinta::novo(11.0, 21.0),
        ],
        Cor::rgba(0x12, 0xAB, 0xEF, 0xFF),
        2.5,
        Ferramenta::Caneta,
    );
    caneta.reto = false;
    tinta.adicionar(caneta);
    let mut marca = Traco::novo(
        vec![PontoTinta::novo(0., 0.), PontoTinta::novo(100., 0.)],
        Cor::rgb(255, 230, 0),
        12.0,
        Ferramenta::MarcaTexto,
    );
    marca.reto = true;
    tinta.adicionar(marca);
    let texto = tinta.para_texto();
    assert!(texto.starts_with("FTINTA 1\nT 1 c 12ABEFFF 2.5 1 s 1.25,2.5,0.5,0 -3,4.25,,16 10,20,1 11,21\n"), "{texto}");
    let lida = Tinta::de_texto(&texto).unwrap();
    assert_eq!(lida, tinta);
    // O próximo id continua depois do maior lido.
    let mut lida = lida;
    assert_eq!(lida.adicionar(traco_de(&[Ponto::novo(0., 0.)])), Some(3));
    // Erros: cabeçalho, linha ruim, id repetido.
    assert!(matches!(Tinta::de_texto("FTINTA 9\n"), Err(ErroTinta::Cabecalho(_))));
    assert!(matches!(Tinta::de_texto("FTINTA 1\nT 1 x 000000FF 1 1 s 0,0\n"), Err(ErroTinta::Linha(2, _))));
    assert!(matches!(Tinta::de_texto("FTINTA 1\nT 1 c 000000FF 1 1 s 0,0\nT 1 c 000000FF 1 1 s 1,1\n"), Err(ErroTinta::IdRepetido(1))));
    assert!(matches!(Tinta::de_texto("FTINTA 1\nT 1 c 000000FF 1 1 s nan,0\n"), Err(ErroTinta::Linha(2, _))));
    assert_eq!(Tinta::de_texto("FTINTA 1\n").unwrap(), Tinta::nova());
}

#[test]
fn drawingml_exporta_forma_livre_e_le_de_volta() {
    let mut t = Traco::novo(
        vec![
            PontoTinta::novo(10., 20.),
            PontoTinta::novo(30., 60.),
            PontoTinta::novo(50., 25.),
            PontoTinta::novo(70., 40.),
        ],
        Cor::rgb(0x21, 0x86, 0x5c),
        3.0,
        Ferramenta::Caneta,
    );
    t.opacidade = 0.5;
    let d = traco_para_drawingml(&t, EMU_POR_PIXEL).unwrap();
    assert_eq!((d.x, d.y), (95_250, 190_500));
    assert_eq!((d.cx, d.cy), (571_500, 381_000));
    assert!(d.geometria.contains("<a:custGeom>"));
    assert!(d.geometria.contains("<a:quadBezTo>"));
    assert!(d.geometria.contains("<a:srgbClr val=\"21865C\"><a:alpha val=\"50000\"/>"));
    assert!(d.geometria.contains("<a:ln w=\"28575\" cap=\"rnd\">"));
    let volta = traco_de_drawingml(&d.geometria, EMU_POR_PIXEL).unwrap();
    assert_eq!(volta.pontos.len(), 4);
    for (a, b) in volta.pontos.iter().zip(&t.pontos) {
        assert!(perto(a.ponto(), b.ponto(), 0.001), "{a:?} != {b:?}");
    }
    assert_eq!(volta.cor, t.cor);
    assert!((volta.espessura - 3.0).abs() < 1e-4);
    assert!((volta.opacidade - 0.5).abs() < 1e-4);
    assert!(!volta.reto);
    // Retângulo reconhecido: só lnTo, e marca-texto vira ponta quadrada.
    let mut r = t.com_forma(&Forma::Retangulo {
        cantos: [Ponto::novo(0., 0.), Ponto::novo(10., 0.), Ponto::novo(10., 5.), Ponto::novo(0., 5.)],
    });
    r.ferramenta = Ferramenta::MarcaTexto;
    let d = traco_para_drawingml(&r, EMU_POR_PIXEL).unwrap();
    assert!(!d.geometria.contains("quadBezTo"));
    let volta = traco_de_drawingml(&d.geometria, EMU_POR_PIXEL).unwrap();
    assert!(volta.reto);
    assert_eq!(volta.ferramenta, Ferramenta::MarcaTexto);
    assert_eq!(volta.pontos.len(), 5);
    // Linha horizontal: altura mínima de 1 EMU.
    let h = traco_de(&[Ponto::novo(0., 5.), Ponto::novo(10., 5.)]);
    assert_eq!(traco_para_drawingml(&h, EMU_POR_PIXEL).unwrap().cy, 1);
    assert!(traco_de_drawingml("<a:prstGeom prst=\"rect\"/>", EMU_POR_PIXEL).is_none());
}

// ------------------------------------------------------------ pressão

/// Número de voltas (regra não-zero) somado sobre todos os polígonos.
fn dentro_nao_zero(p: Ponto, poligonos: &[Vec<Ponto>]) -> bool {
    let mut voltas = 0i32;
    for pol in poligonos {
        let n = pol.len();
        for i in 0..n {
            let (a, b) = (pol[i], pol[(i + 1) % n]);
            if a.y <= p.y {
                if b.y > p.y && (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y) > 0.0 {
                    voltas += 1;
                }
            } else if b.y <= p.y && (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y) < 0.0 {
                voltas -= 1;
            }
        }
    }
    voltas != 0
}

/// Meia largura do contorno na vertical do x dado (varre de 0,05 em 0,05).
fn meia_largura(poligonos: &[Vec<Ponto>], x: f32, y: f32) -> f32 {
    let mut d = 0.0;
    while d < 100.0 && dentro_nao_zero(Ponto::novo(x, y + d + 0.05), poligonos) {
        d += 0.05;
    }
    d
}

fn reta_com_pressao(pressao: impl Fn(f32) -> Option<f32>) -> Vec<PontoTinta> {
    (0..=50)
        .map(|i| {
            let t = i as f32 / 50.0;
            let mut p = PontoTinta::novo(t * 200.0, 50.0);
            p.pressao = pressao(t);
            p
        })
        .collect()
}

#[test]
fn curva_de_resposta_vai_do_minimo_a_espessura_cheia() {
    let c = CurvaPressao::PADRAO;
    assert_eq!(c.fator(None), 1.0);
    assert!((c.fator(Some(0.0)) - c.minimo).abs() < 1e-6);
    assert!((c.fator(Some(1.0)) - 1.0).abs() < 1e-6);
    assert!((c.largura(10.0, Some(0.5)) - 10.0 * (0.2 + 0.8 * 0.5)).abs() < 1e-5);
    // Monótona.
    let mut antes = 0.0;
    for i in 0..=20 {
        let f = c.fator(Some(i as f32 / 20.0));
        assert!(f >= antes);
        antes = f;
    }
    // Gama < 1 engrossa com pouca força; > 1 pede mais força.
    let suave = CurvaPressao::nova(0.2, 0.5);
    let firme = CurvaPressao::nova(0.2, 2.0);
    assert!(suave.resposta(0.25) > 0.25 && firme.resposta(0.25) < 0.25);
    assert!((suave.resposta(0.25) - 0.5).abs() < 1e-6);
    // Valores fora da faixa são limitados; NaN não vaza.
    assert_eq!(CurvaPressao::nova(7.0, -1.0), CurvaPressao { minimo: 1.0, gama: CurvaPressao::GAMA_MIN });
    assert_eq!(c.resposta(f32::NAN), 1.0);
    assert_eq!(c.resposta(3.0), 1.0);
}

#[test]
fn sem_pressao_o_contorno_tem_largura_constante() {
    let pts = reta_com_pressao(|_| None);
    let pol = contorno_largura_variavel(&pts, 6.0, false, CurvaPressao::PADRAO);
    assert!(!pol.is_empty());
    for x in [20.0, 100.0, 180.0] {
        let m = meia_largura(&pol, x, 50.0);
        assert!((m - 3.0).abs() < 0.12, "meia largura {m} em x={x}");
        assert!(dentro_nao_zero(Ponto::novo(x, 47.2), &pol));
        assert!(!dentro_nao_zero(Ponto::novo(x, 53.3), &pol));
    }
    // Ponta redonda: além do último ponto, até o raio.
    assert!(dentro_nao_zero(Ponto::novo(202.5, 50.0), &pol));
    assert!(!dentro_nao_zero(Ponto::novo(203.4, 50.0), &pol));
    // E o traço sem pressão continua desenhado como linha (figura antiga).
    let t = Traco::novo(pts, Cor::rgb(0, 0, 0), 6.0, Ferramenta::Caneta);
    assert!(!t.largura_variavel());
    assert_eq!(t.figura_com_pressao(CurvaPressao::PADRAO), t.figura());
}

#[test]
fn pressao_crescente_gera_contorno_mais_largo() {
    let pts = reta_com_pressao(Some);
    let pol = contorno_largura_variavel(&pts, 10.0, false, CurvaPressao::PADRAO);
    let (a, b, c) = (meia_largura(&pol, 20.0, 50.0), meia_largura(&pol, 100.0, 50.0), meia_largura(&pol, 180.0, 50.0));
    assert!(a < b && b < c, "{a} {b} {c}");
    // Valores esperados: 5 × (0,2 + 0,8 × t).
    for (m, t) in [(a, 0.1), (b, 0.5), (c, 0.9)] {
        assert!((m - 5.0 * (0.2 + 0.8 * t)).abs() < 0.2, "{m} para t={t}");
    }
    // Mais força em toda parte = contorno mais largo em toda parte.
    let forte = contorno_largura_variavel(&reta_com_pressao(|_| Some(0.9)), 10.0, false, CurvaPressao::PADRAO);
    let fraco = contorno_largura_variavel(&reta_com_pressao(|_| Some(0.2)), 10.0, false, CurvaPressao::PADRAO);
    assert!(meia_largura(&forte, 100.0, 50.0) > meia_largura(&fraco, 100.0, 50.0) + 2.0);
    // Todas as peças com a mesma orientação (a regra não-zero soma, não fura).
    assert!(pol.iter().all(|p| super::pressao::area_com_sinal(p) > 0.0));
}

#[test]
fn quina_viva_fica_preenchida_e_ponto_unico_vira_disco() {
    let mut pts: Vec<PontoTinta> = (0..=20).map(|i| PontoTinta::novo(i as f32 * 5.0, 0.0).com_pressao(0.8)).collect();
    pts.extend((1..=20).map(|i| PontoTinta::novo(100.0 - i as f32 * 5.0, i as f32 * 0.8).com_pressao(0.8)));
    let pol = contorno_largura_variavel(&pts, 8.0, true, CurvaPressao::PADRAO);
    assert!(pol.len() >= 2, "a quina divide o contorno");
    assert!(dentro_nao_zero(Ponto::novo(100.0, 0.0), &pol));
    assert!(dentro_nao_zero(Ponto::novo(101.5, 0.0), &pol), "a ponta redonda cobre a quina");
    assert!(dentro_nao_zero(Ponto::novo(50.0, 1.5), &pol));
    let um = contorno_largura_variavel(&[PontoTinta::novo(5.0, 5.0).com_pressao(1.0)], 4.0, false, CurvaPressao::PADRAO);
    assert_eq!(um.len(), 1);
    assert!(dentro_nao_zero(Ponto::novo(6.8, 5.0), &um) && !dentro_nao_zero(Ponto::novo(7.3, 5.0), &um));
    assert!(contorno_largura_variavel(&[], 4.0, false, CurvaPressao::PADRAO).is_empty());
    assert!(contorno_largura_variavel(&pts, f32::NAN, false, CurvaPressao::PADRAO).is_empty());
}

#[test]
fn figura_pdf_svg_e_drawingml_usam_o_mesmo_contorno() {
    let mut t = Traco::novo(reta_com_pressao(Some), Cor::rgb(10, 20, 30), 6.0, Ferramenta::Caneta);
    assert!(t.largura_variavel());
    let Figura::Caminho { caminho, estilo } = t.figura_com_pressao(CurvaPressao::PADRAO) else { panic!("caminho") };
    assert!(estilo.stroke.is_none() && estilo.preenchimento.is_some());
    assert_eq!(caminho.regra, crate::RegraPreenchimento::NaoZero);
    let fechados = caminho.comandos.iter().filter(|c| matches!(c, Comando::Fechar)).count();
    assert_eq!(fechados, t.contorno(CurvaPressao::PADRAO).len());
    let d = traco_para_drawingml_com_pressao(&t, EMU_POR_PIXEL, CurvaPressao::PADRAO).unwrap();
    assert!(d.geometria.contains("<a:close/>") && d.geometria.contains("<a:ln><a:noFill/></a:ln>"));
    assert!(d.geometria.contains("<a:solidFill><a:srgbClr val=\"0A141E\">"));
    // A caixa inclui a largura (a ponta passa do último ponto).
    assert!(d.cx > (200.0 * EMU_POR_PIXEL) as i64);
    // Marca-texto continua com largura constante, mesmo com pressão.
    t.ferramenta = Ferramenta::MarcaTexto;
    assert!(!t.largura_variavel());
    assert_eq!(
        traco_para_drawingml_com_pressao(&t, EMU_POR_PIXEL, CurvaPressao::PADRAO),
        traco_para_drawingml(&t, EMU_POR_PIXEL)
    );
}

#[test]
fn velocidade_faz_pressao_sintetica_rapido_mais_fino() {
    // 20 pontos devagar (1 px a cada 10 ms) e 20 rápidos (10 px/ms).
    let mut pts = Vec::new();
    let (mut x, mut t) = (0.0f32, 0u32);
    for _ in 0..20 {
        pts.push(PontoTinta::novo(x, 0.0).com_tempo(t));
        x += 1.0;
        t += 10;
    }
    for _ in 0..20 {
        pts.push(PontoTinta::novo(x, 0.0).com_tempo(t));
        x += 10.0;
        t += 1;
    }
    let pr = pressoes_por_velocidade(&pts, VELOCIDADE_FINA).unwrap();
    assert_eq!(pr.len(), pts.len());
    assert!(pr[10] > 0.95, "devagar = quase cheio: {}", pr[10]);
    assert!((pr[39] - PRESSAO_SINTETICA_MIN).abs() < 0.02, "rápido = mínimo: {}", pr[39]);
    assert!(pr.iter().all(|p| (PRESSAO_SINTETICA_MIN..=1.0).contains(p)));
    assert!(pressoes_por_velocidade(&[PontoTinta::novo(0.0, 0.0)], 2.0).is_none());
    assert!(pressoes_por_velocidade(&[PontoTinta::novo(0.0, 0.0), PontoTinta::novo(1.0, 0.0)], 2.0).is_none(), "sem tempo");
    assert_eq!(pressao_da_velocidade(0.0, 2.0), 1.0);
    assert_eq!(pressao_da_velocidade(f32::NAN, 2.0), 1.0);

    // Aplicar no traço marca como sintética e sobrevive ao texto.
    let mut traco = Traco::novo(pts.clone(), Cor::rgb(0, 0, 0), 2.0, Ferramenta::Caneta);
    assert!(traco.aplicar_pressao_por_velocidade(VELOCIDADE_FINA));
    assert!(traco.pressao_sintetica && traco.largura_variavel());
    let linha = traco.para_linha();
    assert!(linha.contains(" s v "), "{linha}");
    let lido = Traco::de_linha(&linha).unwrap();
    assert!(lido.pressao_sintetica);
    // Pressão real nunca é sobrescrita pela sintética.
    let mut real = Traco::novo(pts.iter().map(|p| p.com_pressao(0.7)).collect(), Cor::rgb(0, 0, 0), 2.0, Ferramenta::Caneta);
    assert!(!real.aplicar_pressao_por_velocidade(VELOCIDADE_FINA));
    assert!(!real.pressao_sintetica && real.pontos.iter().all(|p| p.pressao == Some(0.7)));
    assert!(!Traco::de_linha(&real.para_linha()).unwrap().pressao_sintetica);
}

#[test]
fn simplificar_guarda_onde_a_forca_muda() {
    // Reta perfeita: sem pressão sobram as pontas; com força subindo e
    // descendo, o pico fica.
    let pts = reta_com_pressao(|t| Some(if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 }));
    assert_eq!(simplificar(&pts, 0.5).len(), 2);
    let com = simplificar_com_pressao(&pts, 0.5, 4.0);
    assert!(com.len() >= 3);
    assert!(com.iter().any(|p| (p.pressao.unwrap() - 1.0).abs() < 1e-3));
    let mut t = Traco::novo(pts, Cor::rgb(0, 0, 0), 8.0, Ferramenta::Caneta);
    t.preparar(0.5);
    assert!(t.pontos.len() >= 3, "preparar não apaga a variação de força");
}

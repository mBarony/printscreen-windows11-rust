//! Estilo desenhado à mão: o traço sai irregular, em duas passadas.
//!
//! A perturbação é função do **comprimento percorrido**, não da posição na
//! imagem: assim mover ou arrastar a anotação não muda o desenho dela. E a
//! semente é o `id` da camada — estável entre quadros e entre sessões, e
//! diferente na cópia, porque duplicar já dá um id novo. Sem uma semente
//! fixa o traço mudaria a cada quadro e a anotação tremeria na tela.

use super::shapes::Point;

/// Distância entre os pontos de apoio da tremida, em múltiplos da espessura.
/// Muito curta vira serrilha; muito longa vira uma reta com um arco só.
const STEP: f32 = 4.0;
/// Piso do apoio, em px: num traço de 1 px, quatro vezes a espessura seriam
/// pontos de apoio a cada 4 px e o desenho viraria ruído.
const STEP_MIN: f32 = 10.0;
/// Amplitude do desvio, em múltiplos da espessura, e seu piso em px.
const AMP: f32 = 0.55;
const AMP_MIN: f32 = 1.2;
/// Teto de nós de ruído por passada — o mesmo motivo do teto do `dash`:
/// um caminho fora de escala não pode custar o quadro inteiro.
const MAX_NOS: usize = 4096;

/// As passadas a desenhar por cima de um caminho.
///
/// Sem o estilo, é o caminho original e nada mais. Com ele, duas passadas
/// levemente diferentes — é a segunda que dá o aspecto de quem repassou o
/// traço para reforçá-lo.
pub fn passes(points: &[Point], width: f32, sketch: bool, seed: u64) -> Vec<Vec<Point>> {
    if !sketch || points.len() < 2 {
        return vec![points.to_vec()];
    }
    let total = length(points);
    if !(total > f32::EPSILON && total.is_finite()) {
        return vec![points.to_vec()];
    }
    (0..2)
        .map(|passada| perturb(points, total, width, seed, passada))
        .collect()
}

/// Desloca o caminho na perpendicular, sem perder o que ele já era.
///
/// **Todos os pontos originais sobrevivem**, e os segmentos longos ganham
/// pontos no meio. Reamostrar o caminho inteiro num passo fixo seria mais
/// curto, mas achataria as curvas: uma elipse, que já vem amostrada em 64
/// pontos, voltaria como um polígono de uma dúzia de lados.
///
/// O desvio é interpolado suavemente entre nós espaçados de `step`, e não
/// sorteado ponto a ponto: sorteado, o traço vira serrilha em vez de tremida.
///
/// As duas pontas ficam presas pela envoltória — uma seta cuja ponta some do
/// alvo deixa de apontar para o que foi apontado, e um retângulo com o canto
/// solto deixa de fechar. O tremido vive no meio.
fn perturb(points: &[Point], total: f32, width: f32, seed: u64, passada: u32) -> Vec<Point> {
    let w = width.max(0.5);
    let nos = ((total / (w * STEP).max(STEP_MIN)).ceil() as usize).clamp(1, MAX_NOS);
    let step = total / nos as f32;
    let amp = (w * AMP).max(AMP_MIN);

    let mut out = Vec::with_capacity(points.len() + nos);
    let mut percorrido = 0.0;
    for par in points.windows(2) {
        let (a, b) = (par[0], par[1]);
        let len = dist(a, b);
        if len <= f32::EPSILON {
            continue;
        }
        let (nx, ny) = (-(b.y - a.y) / len, (b.x - a.x) / len);
        let partes = ((len / step).ceil() as usize).max(1);
        for i in 0..partes {
            let t = i as f32 / partes as f32;
            let pos = percorrido + len * t;
            let d = desvio(seed, passada, pos / step, pos / total) * amp;
            out.push(Point::new(
                a.x + (b.x - a.x) * t + nx * d,
                a.y + (b.y - a.y) * t + ny * d,
            ));
        }
        percorrido += len;
    }
    // A última ponta entra intocada: é onde o usuário soltou o ponteiro.
    out.push(points[points.len() - 1]);
    out
}

/// Desvio em `[-1, 1]`: ruído interpolado entre os nós `u`, atenuado pela
/// envoltória que prende as duas pontas (`fração` vai de 0 a 1 no caminho).
fn desvio(seed: u64, passada: u32, u: f32, fracao: f32) -> f32 {
    let k = u.floor();
    let f = u - k;
    let n0 = ruido(seed, passada, k as u32);
    let n1 = ruido(seed, passada, k as u32 + 1);
    // Smoothstep: a interpolação linear deixaria um bico em cada nó.
    let f = f * f * (3.0 - 2.0 * f);
    (n0 + (n1 - n0) * f) * (fracao * std::f32::consts::PI).sin()
}

/// Ruído determinístico em `[-1, 1)`, misturando semente, passada e apoio.
fn ruido(seed: u64, passada: u32, i: u32) -> f32 {
    let mut h = seed
        ^ ((passada as u64).wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ (i as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    (h >> 40) as f32 / 8_388_608.0 - 1.0
}

fn length(points: &[Point]) -> f32 {
    points.windows(2).map(|s| dist(s[0], s[1])).sum()
}

fn dist(a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reta(comprimento: f32) -> Vec<Point> {
        vec![Point::new(0.0, 0.0), Point::new(comprimento, 0.0)]
    }

    #[test]
    fn sem_o_estilo_o_caminho_passa_intacto() {
        let pontos = reta(100.0);
        assert_eq!(passes(&pontos, 3.0, false, 7), vec![pontos]);
    }

    #[test]
    fn com_o_estilo_saem_duas_passadas_diferentes() {
        let passadas = passes(&reta(200.0), 3.0, true, 7);
        assert_eq!(passadas.len(), 2);
        assert_ne!(passadas[0], passadas[1], "a segunda repassa, não decalca");
    }

    #[test]
    fn as_pontas_ficam_presas() {
        // Uma seta cuja ponta sai do alvo deixa de apontar para o alvo.
        let pontos = reta(200.0);
        for passada in passes(&pontos, 3.0, true, 42) {
            let (inicio, fim) = (passada[0], *passada.last().unwrap());
            assert!(inicio.y.abs() < 0.01 && (inicio.x).abs() < 0.01, "{inicio:?}");
            assert!(fim.y.abs() < 0.01 && (fim.x - 200.0).abs() < 0.01, "{fim:?}");
        }
    }

    #[test]
    fn o_traco_sai_da_reta_no_meio() {
        let passada = &passes(&reta(400.0), 3.0, true, 9)[0];
        let maior = passada.iter().map(|p| p.y.abs()).fold(0.0_f32, f32::max);
        assert!(maior > 0.3, "desvio máximo de apenas {maior}");
        assert!(maior < 5.0, "desvio exagerado: {maior}");
    }

    #[test]
    fn a_mesma_semente_da_o_mesmo_traco() {
        // É o que impede a anotação de tremer a cada quadro.
        assert_eq!(passes(&reta(150.0), 3.0, true, 5), passes(&reta(150.0), 3.0, true, 5));
        assert_ne!(passes(&reta(150.0), 3.0, true, 5), passes(&reta(150.0), 3.0, true, 6));
    }

    #[test]
    fn a_curva_nao_vira_poligono() {
        // Todos os pontos originais sobrevivem: reamostrar num passo fixo
        // devolveria a elipse com uma dúzia de lados retos.
        let elipse: Vec<Point> = (0..=64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                Point::new(60.0 * a.cos(), 40.0 * a.sin())
            })
            .collect();
        let passada = &passes(&elipse, 4.0, true, 3)[0];
        assert!(
            passada.len() >= elipse.len(),
            "a curva perdeu pontos: {} contra {}",
            passada.len(),
            elipse.len()
        );
    }

    #[test]
    fn o_segmento_longo_ganha_pontos_no_meio() {
        // Sem isso, uma reta de duas pontas continuaria reta.
        assert!(passes(&reta(400.0), 3.0, true, 1)[0].len() > 10);
    }

    #[test]
    fn caminho_degenerado_nao_quebra() {
        let parado = vec![Point::new(3.0, 3.0), Point::new(3.0, 3.0)];
        assert_eq!(passes(&parado, 3.0, true, 1), vec![parado.clone()]);
        assert_eq!(passes(&[], 3.0, true, 1), vec![Vec::new()]);
        let absurdo = vec![Point::new(-1.0e20, 0.0), Point::new(1.0e20, 0.0)];
        assert_eq!(passes(&absurdo, 3.0, true, 1), vec![absurdo]);
    }
}

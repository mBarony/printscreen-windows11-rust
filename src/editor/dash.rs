//! Padrão do traço: sólido, tracejado ou pontilhado.
//!
//! Quebrar o caminho aqui — e não dentro de cada desenhista — é o que mantém
//! o preview do egui e a exportação com o mesmo desenho: os dois recebem a
//! mesma lista de sub-caminhos e só mudam de escala.
//!
//! O padrão é medido em múltiplos da espessura, não em pixels fixos: um
//! tracejado de 6 px sobre um traço de 12 px de largura sairia como uma fila
//! de quadrados coladinhos, sem leitura de "tracejado" nenhuma.

use super::shapes::Point;

/// Traço e folga do tracejado, em múltiplos da espessura — medidos na
/// **tinta**, não na linha de centro (ver `split`).
const DASH_ON: f32 = 3.0;
const DASH_OFF: f32 = 2.2;
/// Distância entre os centros dos pontos, em múltiplos da espessura.
const DOT_STEP: f32 = 2.4;
/// Teto de partes de um caminho. Acima disso o padrão já não se lê a olho, e
/// o teto é o que impede que um caminho fora de escala — uma sessão gravada
/// à mão, coordenadas que estouram a precisão do `f32` — custe o quadro
/// inteiro ou um `Vec` de tamanho absurdo.
const MAX_PARTES: f32 = 2048.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Sólido",
            Self::Dashed => "Tracejado",
            Self::Dotted => "Pontilhado",
        }
    }

    /// Cicla entre os três padrões.
    pub fn next(self) -> Self {
        match self {
            Self::Solid => Self::Dashed,
            Self::Dashed => Self::Dotted,
            Self::Dotted => Self::Solid,
        }
    }
}

/// Quebra `points` no padrão de `style`.
///
/// Cada sub-caminho sai como uma lista de pontos, e é uma **linha de centro**:
/// quem desenha põe uma ponta redonda de meia espessura em cada extremidade,
/// nos dois lados (rasterizador e preview). O pontilhado devolve sub-caminhos
/// de **um ponto só**, que viram a marca redonda da ponta.
///
/// O período é esticado para caber um número inteiro de vezes no
/// comprimento: assim o caminho começa e termina cheio, sem meio traço órfão
/// na última esquina de um retângulo.
pub fn split(points: &[Point], style: LineStyle, width: f32) -> Vec<Vec<Point>> {
    let total = length(points);
    if style == LineStyle::Solid || points.len() < 2 || !(total > f32::EPSILON && total.is_finite())
    {
        return vec![points.to_vec()];
    }
    let w = width.max(0.5);
    // `clamp` e não `max`: ver `MAX_PARTES`.
    let partes = |quantas: f32| quantas.round().clamp(1.0, MAX_PARTES);
    match style {
        LineStyle::Solid => unreachable!("tratado acima"),
        LineStyle::Dotted => {
            let count = partes(total / (w * DOT_STEP));
            dots(points, total / count, count as usize)
        }
        LineStyle::Dashed => {
            // `count` traços intercalados por `count-1` folgas: o caminho
            // começa e termina com tinta. Se terminasse na folga, o canto de
            // um retângulo ficaria aberto justamente onde ele se fecha.
            //
            // A conta é sobre a **tinta**, e a tinta é uma espessura mais
            // longa que a linha de centro: as duas pontas do traço são
            // redondas e avançam meia espessura além de cada extremidade.
            // Medindo pela linha de centro, um traço de 12 px sairia com o
            // dobro do comprimento pedido e a folga quase fechada.
            let count = partes(((total + w) / w + DASH_OFF) / (DASH_ON + DASH_OFF));
            let unidade = (total + w) / (count * (DASH_ON + DASH_OFF) - DASH_OFF);
            let centro = (unidade * DASH_ON - w).max(0.0);
            dashes(points, centro, unidade * DASH_OFF + w)
        }
    }
}

/// Um ponto a cada `step` px, `count + 1` deles — o primeiro e o último
/// exatamente nas pontas do caminho.
///
/// O cursor do segmento só anda para a frente: a varredura é uma só, e não
/// uma por ponto. Num rabisco de mil pontos a diferença é o quadro inteiro.
fn dots(points: &[Point], step: f32, count: usize) -> Vec<Vec<Point>> {
    let mut out = Vec::with_capacity(count + 1);
    let mut seg = 0;
    let mut acc = 0.0;
    for i in 0..=count {
        let alvo = i as f32 * step;
        while seg + 2 < points.len() && acc + dist(points[seg], points[seg + 1]) < alvo {
            acc += dist(points[seg], points[seg + 1]);
            seg += 1;
        }
        let len = dist(points[seg], points[seg + 1]);
        let t = if len <= f32::EPSILON {
            0.0
        } else {
            ((alvo - acc) / len).clamp(0.0, 1.0)
        };
        out.push(vec![lerp(points[seg], points[seg + 1], t)]);
    }
    out
}

/// Traços de `on` px separados por folgas de `off`, numa varredura só.
///
/// Os vértices do caminho entram nos traços em que caem — é o que preserva a
/// esquina de um retângulo tracejado em vez de cortá-la em diagonal.
fn dashes(points: &[Point], on: f32, off: f32) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let mut atual = vec![points[0]];
    let mut pintando = true;
    let mut restante = on;
    for par in points.windows(2) {
        let (a, b) = (par[0], par[1]);
        let len = dist(a, b);
        if len <= f32::EPSILON {
            continue;
        }
        let mut andado = 0.0;
        while len - andado > restante {
            andado += restante;
            atual.push(lerp(a, b, andado / len));
            if pintando {
                out.push(std::mem::take(&mut atual));
            }
            pintando = !pintando;
            restante = if pintando { on } else { off };
        }
        restante -= len - andado;
        if pintando {
            atual.push(b);
        }
    }
    if pintando && atual.len() > 1 {
        out.push(atual);
    }
    out
}

/// Comprimento percorrido pela polilinha.
fn length(points: &[Point]) -> f32 {
    points.windows(2).map(|s| dist(s[0], s[1])).sum()
}

fn dist(a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    (dx * dx + dy * dy).sqrt()
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reta(comprimento: f32) -> Vec<Point> {
        vec![Point::new(0.0, 0.0), Point::new(comprimento, 0.0)]
    }

    #[test]
    fn solido_devolve_o_caminho_inteiro() {
        let pontos = reta(100.0);
        let partes = split(&pontos, LineStyle::Solid, 3.0);
        assert_eq!(partes, vec![pontos]);
    }

    #[test]
    fn o_tracejado_comeca_e_termina_cheio() {
        // Sem o período esticado, a última esquina de um retângulo ficaria
        // com meio traço solto.
        let partes = split(&reta(100.0), LineStyle::Dashed, 3.0);
        assert!(partes.len() > 1, "{} traços", partes.len());
        assert!(partes[0][0].x.abs() < 0.01, "começa na ponta");
        let fim = partes.last().unwrap().last().unwrap();
        assert!((fim.x - 100.0).abs() < 0.01, "termina na ponta: {}", fim.x);
    }

    #[test]
    fn o_padrao_acompanha_a_espessura() {
        // Traço grosso, traços mais longos: o número de partes cai.
        let fino = split(&reta(200.0), LineStyle::Dashed, 2.0).len();
        let grosso = split(&reta(200.0), LineStyle::Dashed, 8.0).len();
        assert!(grosso < fino, "fino={fino} grosso={grosso}");
    }

    #[test]
    fn o_pontilhado_sai_em_pontos_soltos() {
        let partes = split(&reta(100.0), LineStyle::Dotted, 3.0);
        assert!(partes.iter().all(|p| p.len() == 1), "cada parte é um ponto");
        assert!(partes.len() > 2);
    }

    #[test]
    fn o_traco_guarda_a_esquina_do_caminho() {
        // Um "L": o vértice do meio tem de sobreviver dentro do traço em que
        // cai, senão o tracejado corta o canto em diagonal.
        let l = vec![
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 50.0),
        ];
        let partes = split(&l, LineStyle::Dashed, 3.0);
        assert!(
            partes
                .iter()
                .flatten()
                .any(|p| (p.x - 50.0).abs() < 0.01 && p.y.abs() < 0.01),
            "o vértice ficou de fora: {partes:?}"
        );
    }

    #[test]
    fn os_tracos_nao_se_encostam() {
        // O que separa um tracejado de uma linha cheia é a folga entre um
        // traço e o seguinte — medida na tinta, com as pontas redondas já
        // contadas. Com a espessura no máximo é onde ela quase fecharia.
        for w in [1.0, 3.0, 12.0] {
            let partes = split(&reta(300.0), LineStyle::Dashed, w);
            for par in partes.windows(2) {
                let folga = par[1][0].x - par[0].last().unwrap().x - w;
                assert!(folga > 0.0, "traços colados com espessura {w}: folga={folga}");
            }
        }
    }

    #[test]
    fn o_padrao_e_medido_na_tinta_e_nao_na_linha_de_centro() {
        // As pontas do traço são redondas e avançam meia espessura além de
        // cada extremidade. Sem descontá-las, um traço grosso sairia com o
        // dobro do comprimento pedido e a folga quase fechada.
        let w = 12.0;
        let partes = split(&reta(600.0), LineStyle::Dashed, w);
        assert!(partes.len() > 3, "{} traços", partes.len());
        let meio = &partes[partes.len() / 2];
        let seguinte = &partes[partes.len() / 2 + 1];
        let tinta = meio.last().unwrap().x - meio[0].x + w;
        let folga = seguinte[0].x - meio.last().unwrap().x - w;
        let proporcao = tinta / folga;
        let esperada = DASH_ON / DASH_OFF;
        assert!(
            (proporcao - esperada).abs() < 0.05,
            "tinta={tinta} folga={folga} proporção={proporcao}, esperada {esperada}"
        );
    }

    #[test]
    fn caminho_degenerado_nao_quebra() {
        let parado = vec![Point::new(7.0, 7.0), Point::new(7.0, 7.0)];
        assert_eq!(split(&parado, LineStyle::Dashed, 3.0), vec![parado.clone()]);
        assert_eq!(split(&[], LineStyle::Dotted, 3.0), vec![Vec::new()]);
    }

    #[test]
    fn caminho_fora_de_escala_nao_explode() {
        // Coordenadas que estouram o `f32` ao serem elevadas ao quadrado: só
        // uma sessão gravada à mão chega aqui, e o editor não pode travar
        // por causa dela.
        let absurdo = vec![Point::new(-1.0e20, 0.0), Point::new(1.0e20, 0.0)];
        assert_eq!(split(&absurdo, LineStyle::Dotted, 3.0), vec![absurdo.clone()]);
        assert_eq!(split(&absurdo, LineStyle::Dashed, 3.0), vec![absurdo]);

        let nan = vec![Point::new(f32::NAN, 0.0), Point::new(10.0, 0.0)];
        assert_eq!(split(&nan, LineStyle::Dashed, 3.0).len(), 1, "cai no sólido");

        // Longo mas finito: o padrão degrada, o número de partes não dispara.
        let longo = reta(1.0e6);
        assert!(split(&longo, LineStyle::Dotted, 1.0).len() <= MAX_PARTES as usize + 1);
        assert!(split(&longo, LineStyle::Dashed, 1.0).len() <= MAX_PARTES as usize);
    }
}

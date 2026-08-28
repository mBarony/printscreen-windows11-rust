//! Montagem do texto reconhecido a partir de onde as palavras estão.
//!
//! O motor devolve linhas, e juntar o texto delas já preserva as quebras. O
//! que ele não preserva é o **alinhamento**: uma tabela reconhecida linha a
//! linha vira um amontoado em que a segunda coluna encosta na primeira, e
//! quem colar no Excel recebe tudo numa célula.
//!
//! A reconstrução é a mesma ideia do `ResultTable.cs` do PowerToys: projetar
//! as caixas das palavras no eixo x, achar as faixas por onde **nenhuma**
//! palavra passa e usar o meio delas como divisória de coluna.
//!
//! Só entra quando o texto de fato parece tabular. Um parágrafo comum tem
//! faixas vazias por acaso — entre duas linhas curtas, por exemplo — e
//! encher de tabulações um texto corrido seria estragar o caso comum para
//! atender o raro.

use crate::platform::ocr::TextBox;

/// Largura mínima de uma faixa vazia para ela contar como divisória, em
/// múltiplos da altura média das palavras.
///
/// O espaço entre duas palavras da mesma frase fica bem abaixo disso; o de
/// uma coluna para a outra, bem acima. Medir em alturas, e não em px, é o que
/// faz o critério valer igual numa captura 1:1 e numa a 300%.
const GAP_MIN: f32 = 1.2;

/// Quantas linhas precisam atravessar mais de uma coluna para o texto ser
/// considerado uma tabela.
const LINHAS_TABULARES: usize = 2;

/// Junta as palavras em texto, com tabulação entre colunas quando houver.
pub fn compose(lines: &[Vec<TextBox>]) -> String {
    let Some(divisorias) = dividers(lines) else {
        return simples(lines);
    };
    // Cada linha vira um vetor de colunas; palavras da mesma coluna saem
    // separadas por espaço, como estavam.
    let tabela: Vec<Vec<String>> = lines
        .iter()
        .map(|linha| {
            let mut colunas = vec![String::new(); divisorias.len() + 1];
            for palavra in linha {
                let centro = palavra.x + palavra.w / 2.0;
                let coluna = divisorias.iter().filter(|d| centro > **d).count();
                let alvo = &mut colunas[coluna];
                if !alvo.is_empty() {
                    alvo.push(' ');
                }
                alvo.push_str(&palavra.text);
            }
            colunas
        })
        .collect();

    // Uma divisória que só uma linha atravessa é acaso de parágrafo, não
    // tabela.
    let atravessam = tabela
        .iter()
        .filter(|colunas| colunas.iter().filter(|c| !c.is_empty()).count() > 1)
        .count();
    if atravessam < LINHAS_TABULARES {
        return simples(lines);
    }

    tabela
        .into_iter()
        .map(|mut colunas| {
            // Colunas vazias no fim viram tabulações penduradas.
            while colunas.last().is_some_and(String::is_empty) {
                colunas.pop();
            }
            colunas.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// O texto como o motor o entregou: uma linha por linha.
fn simples(lines: &[Vec<TextBox>]) -> String {
    lines
        .iter()
        .map(|linha| {
            linha
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Posições em x das faixas por onde nenhuma palavra passa, largas o
/// bastante para serem coluna. `None` quando não há nenhuma.
fn dividers(lines: &[Vec<TextBox>]) -> Option<Vec<f32>> {
    let palavras: Vec<&TextBox> = lines.iter().flatten().collect();
    if lines.len() < LINHAS_TABULARES || palavras.len() < 4 {
        return None;
    }
    let esquerda = palavras.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    let direita = palavras.iter().map(|p| p.x + p.w).fold(f32::MIN, f32::max);
    let largura = direita - esquerda;
    if !(largura.is_finite() && largura > 1.0) {
        return None;
    }
    let altura = mediana(palavras.iter().map(|p| p.h).collect());
    if altura <= 0.0 || !altura.is_finite() {
        return None;
    }
    let folga = altura * GAP_MIN;

    // Ocupação em cubas de 1 px do intervalo, com teto para um texto fora de
    // escala não custar um vetor absurdo.
    const MAX_CUBAS: usize = 8192;
    let cubas = (largura.ceil() as usize).clamp(1, MAX_CUBAS);
    let escala = cubas as f32 / largura;
    let mut ocupado = vec![false; cubas];
    for p in &palavras {
        let de = (((p.x - esquerda) * escala).floor() as usize).min(cubas - 1);
        let ate = (((p.x + p.w - esquerda) * escala).ceil() as usize).min(cubas);
        for cuba in &mut ocupado[de..ate.max(de + 1).min(cubas)] {
            *cuba = true;
        }
    }

    let mut out = Vec::new();
    let mut inicio = None;
    for (i, cheia) in ocupado.iter().enumerate() {
        match (cheia, inicio) {
            (false, None) => inicio = Some(i),
            (true, Some(de)) => {
                registrar(&mut out, de, i, esquerda, escala, folga);
                inicio = None;
            }
            _ => {}
        }
    }
    // Uma faixa vazia que chega até o fim é margem à direita, não divisória.
    (!out.is_empty()).then_some(out)
}

fn registrar(out: &mut Vec<f32>, de: usize, ate: usize, esquerda: f32, escala: f32, folga: f32) {
    let largura = (ate - de) as f32 / escala;
    if largura >= folga {
        out.push(esquerda + (de + ate) as f32 / 2.0 / escala);
    }
}

fn mediana(mut valores: Vec<f32>) -> f32 {
    if valores.is_empty() {
        return 0.0;
    }
    valores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    valores[valores.len() / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palavra(texto: &str, x: f32, y: f32, w: f32) -> TextBox {
        TextBox { text: texto.into(), x, y, w, h: 10.0 }
    }

    #[test]
    fn um_paragrafo_comum_nao_ganha_tabulacoes() {
        // Duas linhas de texto corrido, com espaços normais entre palavras.
        let linhas = vec![
            vec![
                palavra("bom", 0.0, 0.0, 30.0),
                palavra("dia", 34.0, 0.0, 30.0),
                palavra("a", 68.0, 0.0, 10.0),
                palavra("todos", 82.0, 0.0, 50.0),
            ],
            vec![
                palavra("como", 0.0, 20.0, 40.0),
                palavra("vão", 44.0, 20.0, 30.0),
                palavra("as", 78.0, 20.0, 20.0),
                palavra("coisas", 102.0, 20.0, 50.0),
            ],
        ];
        let texto = compose(&linhas);
        assert!(!texto.contains('\t'), "texto corrido não é tabela: {texto:?}");
        assert_eq!(texto, "bom dia a todos\ncomo vão as coisas");
    }

    #[test]
    fn uma_tabela_sai_com_as_colunas_separadas() {
        // Duas colunas com uma faixa larga e vazia entre elas.
        let linhas = vec![
            vec![palavra("Nome", 0.0, 0.0, 40.0), palavra("Idade", 200.0, 0.0, 45.0)],
            vec![palavra("Ana", 0.0, 20.0, 30.0), palavra("31", 200.0, 20.0, 20.0)],
            vec![palavra("Bruno", 0.0, 40.0, 50.0), palavra("47", 200.0, 40.0, 20.0)],
        ];
        assert_eq!(compose(&linhas), "Nome\tIdade\nAna\t31\nBruno\t47");
    }

    #[test]
    fn uma_celula_de_duas_palavras_continua_inteira() {
        let linhas = vec![
            vec![
                palavra("Nome", 0.0, 0.0, 40.0),
                palavra("completo", 44.0, 0.0, 70.0),
                palavra("Idade", 300.0, 0.0, 45.0),
            ],
            vec![
                palavra("Ana", 0.0, 20.0, 30.0),
                palavra("Maria", 34.0, 20.0, 45.0),
                palavra("31", 300.0, 20.0, 20.0),
            ],
        ];
        assert_eq!(compose(&linhas), "Nome completo\tIdade\nAna Maria\t31");
    }

    #[test]
    fn uma_celula_vazia_vira_coluna_vazia() {
        // A segunda linha não tem a primeira coluna: sem o lugar guardado, o
        // "31" subiria para a coluna errada ao colar numa planilha.
        let linhas = vec![
            vec![palavra("Ana", 0.0, 0.0, 30.0), palavra("31", 200.0, 0.0, 20.0)],
            vec![palavra("47", 200.0, 20.0, 20.0)],
            vec![palavra("Bruno", 0.0, 40.0, 50.0), palavra("52", 200.0, 40.0, 20.0)],
        ];
        assert_eq!(compose(&linhas), "Ana\t31\n\t47\nBruno\t52");
    }

    #[test]
    fn uma_linha_so_nao_vira_tabela() {
        let linhas = vec![vec![
            palavra("Nome", 0.0, 0.0, 40.0),
            palavra("Idade", 200.0, 0.0, 45.0),
        ]];
        assert_eq!(compose(&linhas), "Nome Idade");
    }

    #[test]
    fn nada_reconhecido_nao_quebra() {
        assert_eq!(compose(&[]), "");
        assert_eq!(compose(&[Vec::new(), Vec::new()]), "\n");
    }

    #[test]
    fn o_criterio_da_folga_acompanha_o_tamanho_da_fonte() {
        // A mesma tabela, com o dobro do tamanho: continua sendo tabela.
        let dobro = |p: &TextBox| TextBox {
            text: p.text.clone(),
            x: p.x * 2.0,
            y: p.y * 2.0,
            w: p.w * 2.0,
            h: p.h * 2.0,
        };
        let linhas = vec![
            vec![palavra("Ana", 0.0, 0.0, 30.0), palavra("31", 200.0, 0.0, 20.0)],
            vec![palavra("Bruno", 0.0, 20.0, 50.0), palavra("47", 200.0, 20.0, 20.0)],
        ];
        let grande: Vec<Vec<TextBox>> = linhas
            .iter()
            .map(|l| l.iter().map(dobro).collect())
            .collect();
        assert_eq!(compose(&linhas), compose(&grande));
        assert!(compose(&grande).contains('\t'));
    }
}

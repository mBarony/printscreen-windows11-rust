//! Anotações na área de transferência, num formato próprio.
//!
//! Elas viajam como **texto**, e não num formato de clipboard registrado: o
//! texto atravessa processos sem nada além do que o Windows já oferece, e é
//! o mesmo caminho pelo qual o RustShot já copia o resultado do OCR. Quem
//! colar num editor de texto vê um JSON — feio, mas honesto; quem colar no
//! RustShot recebe as anotações de volta.
//!
//! A chave `rustshot_layers` é o que distingue este texto de qualquer outro
//! que estiver na área de transferência.

use crate::json::{self, Value};

use super::session_file::{decode_layer, encode_layer};
use super::shapes::Layer;

/// Marca do formato, e ao mesmo tempo sua versão.
const MARK: &str = "rustshot_layers";
const VERSION: f64 = 1.0;

/// Serializa as anotações para o texto que vai à área de transferência.
pub fn encode(layers: &[Layer]) -> String {
    let value = json::obj(vec![
        (MARK, json::n(VERSION)),
        ("layers", json::arr(layers.iter().map(encode_layer).collect())),
    ]);
    json::to_string_pretty(&value)
}

/// Reconhece o formato e devolve as anotações, **com os ids como estavam** —
/// quem cola é que dá ids novos, porque só o documento de destino sabe quais
/// estão livres.
///
/// Qualquer outro texto devolve `None`, e isso não é erro: colar um texto
/// comum no editor simplesmente não faz nada.
pub fn decode(text: &str) -> Option<Vec<Layer>> {
    let value = json::parse(text).ok()?;
    if value.get(MARK).and_then(Value::as_f64)? != VERSION {
        return None;
    }
    let layers: Vec<Layer> = value
        .get("layers")?
        .as_array()?
        .iter()
        .map(decode_layer)
        .collect::<Option<Vec<Layer>>>()?;
    (!layers.is_empty()).then_some(layers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::shapes::{Point, Shape, Style};

    fn layer(id: u64, x: f32) -> Layer {
        Layer {
            id,
            shape: Shape::Line {
                a: Point::new(x, 1.0),
                b: Point::new(x + 10.0, 20.0),
            },
            style: Style {
                color: [1, 2, 3, 255],
                stroke_width: 4.0,
                line: crate::editor::shapes::LineStyle::Dotted,
                sketch: true,
                font_size: 20.0,
                filled: false,
                corner_radius: 2.0,
                text_pill: false,
                redaction: crate::editor::shapes::RedactionStyle::Solid,
                spotlight: crate::editor::shapes::SpotlightForm::Rect,
                magnification: 2.0,
            },
        }
    }

    #[test]
    fn as_anotacoes_sobrevivem_a_ida_e_volta() {
        let originais = vec![layer(1, 0.0), layer(2, 50.0)];
        let voltaram = decode(&encode(&originais)).expect("formato reconhecido");
        assert_eq!(voltaram, originais);
    }

    #[test]
    fn texto_alheio_nao_e_confundido_com_anotacao() {
        assert!(decode("").is_none());
        assert!(decode("bom dia").is_none());
        assert!(decode(r#"{"layers": []}"#).is_none(), "sem a marca não vale");
        assert!(
            decode(r#"{"rustshot_layers": 99, "layers": []}"#).is_none(),
            "outra versão do formato não vale"
        );
    }

    #[test]
    fn uma_lista_vazia_nao_conta_como_colagem() {
        // Colar e não acontecer nada é melhor que colar e registrar um passo
        // de desfazer que não fez nada.
        assert!(decode(&encode(&[])).is_none());
    }
}

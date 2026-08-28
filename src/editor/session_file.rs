//! Gravação da sessão de edição, para ela sobreviver a um fechamento
//! inesperado.
//!
//! São dois arquivos irmãos no diretório de estado: a imagem de origem e o
//! log de operações. Juntos reconstroem o documento inteiro — inclusive o
//! histórico, então desfazer continua funcionando depois de reabrir.
//!
//! A imagem vai em RGBA cru, e não em JPG: ela é regravada a cada sessão
//! recuperada, e recomprimir a mesma captura repetidamente degradaria o que
//! o usuário ainda não terminou de anotar. O custo é espaço em disco por
//! alguns segundos, o que não é problema para um arquivo temporário.
//!
//! Os dois arquivos são removidos quando a sessão termina normalmente. O que
//! sobra deles, portanto, é exatamente um fechamento anormal.

use std::path::{Path, PathBuf};

use crate::error::{err, Context as _, Result};
use crate::imgbuf::RgbaImage;
use crate::json::{self, Value};

use super::backdrop::BackdropStyle;
use super::cut::{Axis, Band};
use super::document::{Document, Op};
use super::redact::RedactionStyle;
use super::shapes::{Layer, Point, Shape, SpotlightForm, Style};

/// Assinatura do arquivo de imagem cru.
const RAW_MAGIC: &[u8; 4] = b"RSRW";
const FORMAT_VERSION: f64 = 1.0;

fn image_path(dir: &Path) -> PathBuf {
    dir.join("session.rsraw")
}

fn log_path(dir: &Path) -> PathBuf {
    dir.join("session.json")
}

/// Grava a imagem de origem. Só precisa acontecer uma vez por sessão: ela
/// não muda, e reescrever dezenas de MB a cada anotação seria absurdo.
pub fn save_source(doc: &Document, dir: &Path) -> Result<()> {
    let image = doc.source_image();
    let mut raw = Vec::with_capacity(12 + image.as_raw().len());
    raw.extend_from_slice(RAW_MAGIC);
    raw.extend_from_slice(&image.width().to_le_bytes());
    raw.extend_from_slice(&image.height().to_le_bytes());
    raw.extend_from_slice(image.as_raw());
    std::fs::write(image_path(dir), raw).context("gravando a imagem da sessão")
}

/// Grava o log de operações — barato, e é o que muda a cada edição.
pub fn save_log(doc: &Document, dir: &Path) -> Result<()> {
    let value = json::obj(vec![
        ("version", json::n(FORMAT_VERSION)),
        ("applied", json::n(doc.applied() as f64)),
        ("next_id", json::n(doc.next_id() as f64)),
        ("ops", json::arr(doc.ops().iter().map(encode_op).collect())),
    ]);
    std::fs::write(log_path(dir), json::to_string_pretty(&value))
        .context("gravando o log da sessão")
}

/// Grava imagem e log de uma vez. Em produção os dois vão separados (a
/// imagem só na primeira vez); aqui é a conveniência dos testes.
#[cfg(test)]
pub fn save(doc: &Document, dir: &Path) -> Result<()> {
    save_source(doc, dir)?;
    save_log(doc, dir)
}

/// Lê a sessão gravada, se houver uma completa.
pub fn load(dir: &Path) -> Option<Document> {
    let raw = std::fs::read(image_path(dir)).ok()?;
    let text = std::fs::read_to_string(log_path(dir)).ok()?;
    let image = decode_image(&raw).ok()?;
    let value = json::parse(&text).ok()?;
    // Formato de outra versão: melhor ignorar a sessão do que restaurá-la
    // errado.
    if value.get("version").and_then(Value::as_f64) != Some(FORMAT_VERSION) {
        return None;
    }
    let ops: Vec<Op> = value
        .get("ops")
        .and_then(Value::as_array)?
        .iter()
        .map(decode_op)
        .collect::<Option<Vec<Op>>>()?;
    let applied = value.get("applied").and_then(Value::as_f64).unwrap_or(0.0) as usize;
    let next_id = value.get("next_id").and_then(Value::as_f64).unwrap_or(1.0) as u64;
    Some(Document::restore(image, ops, applied, next_id))
}

/// Há uma sessão gravada esperando?
pub fn exists(dir: &Path) -> bool {
    image_path(dir).is_file() && log_path(dir).is_file()
}

/// Apaga a sessão gravada — chamado quando o editor fecha por vontade do
/// usuário, que é o que distingue um fim normal de um travamento.
pub fn clear(dir: &Path) {
    let _ = std::fs::remove_file(image_path(dir));
    let _ = std::fs::remove_file(log_path(dir));
}

fn decode_image(raw: &[u8]) -> Result<RgbaImage> {
    if raw.len() < 12 || &raw[0..4] != RAW_MAGIC {
        return Err(err!("imagem de sessão com assinatura inválida"));
    }
    let width = u32::from_le_bytes(raw[4..8].try_into().unwrap());
    let height = u32::from_le_bytes(raw[8..12].try_into().unwrap());
    let expected = width as usize * height as usize * 4;
    if width == 0 || height == 0 || raw.len() < 12 + expected {
        return Err(err!("imagem de sessão truncada"));
    }
    Ok(RgbaImage::from_raw(width, height, raw[12..12 + expected].to_vec()))
}

// ---------------------------------------------------------------------------
// Codificação
// ---------------------------------------------------------------------------

fn point(p: Point) -> Value {
    json::arr(vec![json::n(p.x as f64), json::n(p.y as f64)])
}

fn read_point(v: Option<&Value>) -> Option<Point> {
    let items = v?.as_array()?;
    Some(Point::new(
        items.first()?.as_f64()? as f32,
        items.get(1)?.as_f64()? as f32,
    ))
}

fn encode_style(style: &Style) -> Value {
    json::obj(vec![
        (
            "color",
            json::arr(style.color.iter().map(|c| json::n(*c as f64)).collect()),
        ),
        ("stroke_width", json::n(style.stroke_width as f64)),
        ("font_size", json::n(style.font_size as f64)),
        ("filled", json::b(style.filled)),
        ("corner_radius", json::n(style.corner_radius as f64)),
        ("text_pill", json::b(style.text_pill)),
        (
            "redaction",
            json::s(match style.redaction {
                RedactionStyle::Pixelate => "pixelate",
                RedactionStyle::Solid => "solid",
            }),
        ),
        (
            "spotlight",
            json::s(match style.spotlight {
                SpotlightForm::Ellipse => "ellipse",
                SpotlightForm::Rect => "rect",
                SpotlightForm::RoundedRect => "rounded",
            }),
        ),
        ("magnification", json::n(style.magnification as f64)),
    ])
}

fn decode_style(v: &Value) -> Option<Style> {
    let color = v.get("color")?.as_array()?;
    let mut rgba = [0u8; 4];
    for (slot, item) in rgba.iter_mut().zip(color) {
        *slot = item.as_f64()? as u8;
    }
    let number = |key: &str| v.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let flag = |key: &str| v.get(key).and_then(Value::as_bool).unwrap_or(false);
    let text = |key: &str| v.get(key).and_then(Value::as_str).unwrap_or("");
    Some(Style {
        color: rgba,
        stroke_width: number("stroke_width"),
        font_size: number("font_size"),
        filled: flag("filled"),
        corner_radius: number("corner_radius"),
        text_pill: flag("text_pill"),
        redaction: match text("redaction") {
            "solid" => RedactionStyle::Solid,
            _ => RedactionStyle::Pixelate,
        },
        spotlight: match text("spotlight") {
            "rect" => SpotlightForm::Rect,
            "rounded" => SpotlightForm::RoundedRect,
            _ => SpotlightForm::Ellipse,
        },
        magnification: number("magnification"),
    })
}

fn encode_shape(shape: &Shape) -> Value {
    match shape {
        Shape::Line { a, b } => {
            json::obj(vec![("kind", json::s("line")), ("a", point(*a)), ("b", point(*b))])
        }
        Shape::Arrow { a, b, bend } => json::obj(vec![
            ("kind", json::s("arrow")),
            ("a", point(*a)),
            ("b", point(*b)),
            ("bend", json::n(*bend as f64)),
        ]),
        Shape::Rect { min, max } => json::obj(vec![
            ("kind", json::s("rect")),
            ("min", point(*min)),
            ("max", point(*max)),
        ]),
        Shape::Ellipse { center, rx, ry } => json::obj(vec![
            ("kind", json::s("ellipse")),
            ("center", point(*center)),
            ("rx", json::n(*rx as f64)),
            ("ry", json::n(*ry as f64)),
        ]),
        Shape::Freehand { points, highlight } => json::obj(vec![
            ("kind", json::s("freehand")),
            ("points", json::arr(points.iter().map(|p| point(*p)).collect())),
            ("highlight", json::b(*highlight)),
        ]),
        Shape::Marker { center, number } => json::obj(vec![
            ("kind", json::s("marker")),
            ("center", point(*center)),
            ("number", json::n(*number as f64)),
        ]),
        Shape::Redaction { min, max, seed } => json::obj(vec![
            ("kind", json::s("redaction")),
            ("min", point(*min)),
            ("max", point(*max)),
            ("seed", json::n(*seed as f64)),
        ]),
        Shape::Spotlight { center, rx, ry } => json::obj(vec![
            ("kind", json::s("spotlight")),
            ("center", point(*center)),
            ("rx", json::n(*rx as f64)),
            ("ry", json::n(*ry as f64)),
        ]),
        Shape::Text { anchor, content } => json::obj(vec![
            ("kind", json::s("text")),
            ("anchor", point(*anchor)),
            ("content", json::s(content)),
        ]),
    }
}

fn decode_shape(v: &Value) -> Option<Shape> {
    let number = |key: &str| v.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    match v.get("kind")?.as_str()? {
        "line" => Some(Shape::Line {
            a: read_point(v.get("a"))?,
            b: read_point(v.get("b"))?,
        }),
        "arrow" => Some(Shape::Arrow {
            a: read_point(v.get("a"))?,
            b: read_point(v.get("b"))?,
            bend: v.get("bend").and_then(|b| b.as_f64()).unwrap_or(0.0) as f32,
        }),
        "rect" => Some(Shape::Rect {
            min: read_point(v.get("min"))?,
            max: read_point(v.get("max"))?,
        }),
        "ellipse" => Some(Shape::Ellipse {
            center: read_point(v.get("center"))?,
            rx: number("rx"),
            ry: number("ry"),
        }),
        "freehand" => Some(Shape::Freehand {
            points: v
                .get("points")?
                .as_array()?
                .iter()
                .map(|p| read_point(Some(p)))
                .collect::<Option<Vec<Point>>>()?,
            highlight: v.get("highlight").and_then(Value::as_bool).unwrap_or(false),
        }),
        "marker" => Some(Shape::Marker {
            center: read_point(v.get("center"))?,
            number: number("number") as u32,
        }),
        "redaction" => Some(Shape::Redaction {
            min: read_point(v.get("min"))?,
            max: read_point(v.get("max"))?,
            seed: v.get("seed").and_then(Value::as_f64).unwrap_or(1.0) as u32,
        }),
        "spotlight" => Some(Shape::Spotlight {
            center: read_point(v.get("center"))?,
            rx: number("rx"),
            ry: number("ry"),
        }),
        "text" => Some(Shape::Text {
            anchor: read_point(v.get("anchor"))?,
            content: v.get("content")?.as_str()?.to_owned(),
        }),
        _ => None,
    }
}

fn encode_layer(layer: &Layer) -> Value {
    json::obj(vec![
        ("id", json::n(layer.id as f64)),
        ("shape", encode_shape(&layer.shape)),
        ("style", encode_style(&layer.style)),
    ])
}

fn decode_layer(v: &Value) -> Option<Layer> {
    Some(Layer {
        id: v.get("id")?.as_f64()? as u64,
        shape: decode_shape(v.get("shape")?)?,
        style: decode_style(v.get("style")?)?,
    })
}

fn encode_op(op: &Op) -> Value {
    match op {
        Op::Annotate(layer) => {
            json::obj(vec![("op", json::s("annotate")), ("layer", encode_layer(layer))])
        }
        Op::Patch(layers) => json::obj(vec![
            ("op", json::s("patch")),
            ("layers", json::arr(layers.iter().map(encode_layer).collect())),
        ]),
        Op::Delete(ids) => json::obj(vec![
            ("op", json::s("delete")),
            ("ids", json::arr(ids.iter().map(|id| json::n(*id as f64)).collect())),
        ]),
        Op::Crop { x, y, w, h } => json::obj(vec![
            ("op", json::s("crop")),
            ("x", json::n(*x as f64)),
            ("y", json::n(*y as f64)),
            ("w", json::n(*w as f64)),
            ("h", json::n(*h as f64)),
        ]),
        Op::Cut(band) => json::obj(vec![
            ("op", json::s("cut")),
            (
                "axis",
                json::s(match band.axis {
                    Axis::Horizontal => "horizontal",
                    Axis::Vertical => "vertical",
                }),
            ),
            ("start", json::n(band.start as f64)),
            ("end", json::n(band.end as f64)),
        ]),
        Op::Scale(factor) => json::obj(vec![
            ("op", json::s("scale")),
            ("factor", json::n(*factor as f64)),
        ]),
        Op::Backdrop(style) => json::obj(vec![
            ("op", json::s("backdrop")),
            (
                "style",
                json::s(match style {
                    BackdropStyle::None => "none",
                    BackdropStyle::Aurora => "aurora",
                    BackdropStyle::Sunset => "sunset",
                    BackdropStyle::Lagoon => "lagoon",
                    BackdropStyle::Violet => "violet",
                }),
            ),
        ]),
    }
}

fn decode_op(v: &Value) -> Option<Op> {
    let number = |key: &str| v.get(key).and_then(Value::as_f64).unwrap_or(0.0) as u32;
    match v.get("op")?.as_str()? {
        "annotate" => Some(Op::Annotate(decode_layer(v.get("layer")?)?)),
        "patch" => Some(Op::Patch(
            v.get("layers")?
                .as_array()?
                .iter()
                .map(decode_layer)
                .collect::<Option<Vec<Layer>>>()?,
        )),
        "delete" => Some(Op::Delete(
            v.get("ids")?
                .as_array()?
                .iter()
                .map(|id| id.as_f64().map(|n| n as u64))
                .collect::<Option<Vec<u64>>>()?,
        )),
        "crop" => Some(Op::Crop {
            x: number("x"),
            y: number("y"),
            w: number("w"),
            h: number("h"),
        }),
        "cut" => Some(Op::Cut(Band {
            axis: match v.get("axis")?.as_str()? {
                "vertical" => Axis::Vertical,
                _ => Axis::Horizontal,
            },
            start: number("start"),
            end: number("end"),
        })),
        "scale" => Some(Op::Scale(v.get("factor")?.as_f64()? as f32)),
        "backdrop" => Some(Op::Backdrop(match v.get("style")?.as_str()? {
            "aurora" => BackdropStyle::Aurora,
            "sunset" => BackdropStyle::Sunset,
            "lagoon" => BackdropStyle::Lagoon,
            "violet" => BackdropStyle::Violet,
            _ => BackdropStyle::None,
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style {
            color: [12, 34, 56, 255],
            stroke_width: 4.0,
            font_size: 20.0,
            filled: true,
            corner_radius: 6.0,
            text_pill: true,
            redaction: RedactionStyle::Solid,
            spotlight: SpotlightForm::RoundedRect,
            magnification: 2.5,
        }
    }

    fn p(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    /// Uma de cada forma — se o round-trip cobre todas, o formato está de pé.
    fn every_shape() -> Vec<Shape> {
        vec![
            Shape::Line { a: p(1.0, 2.0), b: p(3.0, 4.0) },
            Shape::Arrow { a: p(5.0, 6.0), b: p(7.0, 8.0), bend: 0.25 },
            Shape::Rect { min: p(9.0, 10.0), max: p(11.0, 12.0) },
            Shape::Ellipse { center: p(13.0, 14.0), rx: 15.0, ry: 16.0 },
            Shape::Freehand { points: vec![p(17.0, 18.0), p(19.0, 20.0)], highlight: true },
            Shape::Marker { center: p(21.0, 22.0), number: 7 },
            Shape::Redaction { min: p(23.0, 24.0), max: p(25.0, 26.0), seed: 4242 },
            Shape::Spotlight { center: p(27.0, 28.0), rx: 29.0, ry: 30.0 },
            Shape::Text { anchor: p(31.0, 32.0), content: "acentuação ☕".into() },
        ]
    }

    #[test]
    fn every_shape_survives_the_round_trip() {
        for shape in every_shape() {
            let restored = decode_shape(&encode_shape(&shape)).expect("forma decodificada");
            assert_eq!(restored, shape, "round-trip de {shape:?}");
        }
    }

    #[test]
    fn the_style_survives_the_round_trip() {
        assert_eq!(decode_style(&encode_style(&style())).unwrap(), style());
    }

    #[test]
    fn every_operation_survives_the_round_trip() {
        let layer = Layer { id: 3, shape: every_shape()[0].clone(), style: style() };
        let ops = vec![
            Op::Annotate(layer.clone()),
            Op::Patch(vec![layer]),
            Op::Delete(vec![1, 2, 3]),
            Op::Crop { x: 1, y: 2, w: 3, h: 4 },
            Op::Cut(Band { axis: Axis::Vertical, start: 5, end: 9 }),
            Op::Backdrop(BackdropStyle::Lagoon),
        ];
        for op in ops {
            assert_eq!(decode_op(&encode_op(&op)).expect("op decodificada"), op);
        }
    }

    #[test]
    fn a_session_round_trips_through_the_disk() {
        let dir = std::env::temp_dir().join(format!(
            "rustshot-session-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let mut doc = Document::new(RgbaImage::filled(8, 6, [10, 20, 30, 255]));
        doc.push(every_shape()[2].clone(), style());
        doc.push(every_shape()[5].clone(), style());
        doc.undo(); // deixa uma operação no "refazer"

        save(&doc, &dir).unwrap();
        let restored = load(&dir).expect("sessão recuperada");

        assert_eq!(restored.layers(), doc.layers());
        assert_eq!(restored.applied(), doc.applied());
        // O refazer sobrevive: era o ponto de guardar o log, e não a imagem
        // achatada.
        assert_eq!(restored.ops().len(), 2);
        assert!(restored.can_redo());

        clear(&dir);
        assert!(load(&dir).is_none(), "limpar apaga a sessão");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_from_another_version_is_ignored() {
        let dir = std::env::temp_dir().join(format!(
            "rustshot-session-version-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = Document::new(RgbaImage::filled(4, 4, [0, 0, 0, 255]));
        save(&doc, &dir).unwrap();

        let text = std::fs::read_to_string(log_path(&dir)).unwrap();
        std::fs::write(log_path(&dir), text.replace("\"version\": 1", "\"version\": 99")).unwrap();
        assert!(load(&dir).is_none(), "restaurar errado é pior que não restaurar");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

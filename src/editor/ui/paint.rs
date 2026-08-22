//! Desenho das anotações no canvas e do cromo de seleção.
//!
//! A geometria aqui é a mesma que `render.rs` usa na exportação; só a escala
//! muda. É o que mantém o preview fiel ao JPG final.

use egui::{
    Align2, Color32, CornerRadius, FontId,
    Pos2, Rect, Stroke, StrokeKind, Vec2,
};


use crate::editor::shapes::{
    arrow_geometry, marker_geometry, stroke_appearance, text_pill_metrics, Layer, Point, Shape, MARKER_INK, TEXT_PILL_COLOR,
};
use crate::editor::{
    HANDLE_EDGE_ROOM_PTS, HANDLE_RADIUS_PTS,
};
use super::ToScreen;

pub(super) fn paint_shape(painter: &egui::Painter, layer: &Layer, ts: ToScreen) {
    let style = &layer.style;
    let stroke = Stroke::new(ts.len(style.stroke_width), color32(style.color));
    match &layer.shape {
        Shape::Line { a, b } => {
            painter.line_segment([ts.pos(*a), ts.pos(*b)], stroke);
        }
        Shape::Arrow { a, b } => {
            let geo = arrow_geometry(*a, *b, style.stroke_width);
            painter.line_segment([ts.pos(geo.shaft_a), ts.pos(geo.shaft_b)], stroke);
            painter.add(egui::Shape::convex_polygon(
                geo.head.iter().map(|p| ts.pos(*p)).collect(),
                color32(style.color),
                Stroke::NONE,
            ));
        }
        Shape::Rect { min, max } => {
            let rect = Rect::from_min_max(ts.pos(*min), ts.pos(*max));
            // O raio é limitado a metade do menor lado, como no rasterizador
            // da exportação — senão o preview arredondaria mais que o JPG.
            let radius = ts
                .len(style.corner_radius)
                .min(rect.width() / 2.0)
                .min(rect.height() / 2.0)
                .max(0.0)
                .round() as u8;
            if style.filled {
                painter.rect_filled(rect, CornerRadius::same(radius), color32(style.color));
            } else {
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(radius),
                    stroke,
                    StrokeKind::Middle,
                );
            }
        }
        Shape::Ellipse { center, rx, ry } => {
            let radii = Vec2::new(ts.len(*rx), ts.len(*ry));
            if style.filled {
                painter.add(egui::Shape::ellipse_filled(
                    ts.pos(*center),
                    radii,
                    color32(style.color),
                ));
            } else {
                painter.add(egui::Shape::ellipse_stroke(ts.pos(*center), radii, stroke));
            }
        }
        Shape::Freehand { points, highlight } => {
            let (width, color) = stroke_appearance(style, *highlight);
            // Mesma lista de pontos que a exportação percorre: a suavização
            // já aconteceu quando o traço foi criado.
            painter.add(egui::Shape::line(
                points.iter().map(|p| ts.pos(*p)).collect(),
                Stroke::new(ts.len(width), color32(color)),
            ));
        }
        Shape::Marker { center, number } => {
            let geo = marker_geometry(style.stroke_width);
            let at = ts.pos(*center);
            painter.circle(
                at,
                ts.len(geo.radius),
                color32(style.color),
                Stroke::new(ts.len(geo.ring_width), color32(MARKER_INK)),
            );
            painter.text(
                at,
                Align2::CENTER_CENTER,
                number,
                FontId::new(
                    ts.len(geo.font_size),
                    egui::FontFamily::Name(crate::theme::INTER.into()),
                ),
                color32(MARKER_INK),
            );
        }
        // Redação e holofote já fazem parte da textura da imagem.
        Shape::Redaction { .. } | Shape::Spotlight { .. } => {}
        Shape::Text { anchor, content } => {
            let font = FontId::new(
                ts.len(style.font_size),
                egui::FontFamily::Name(crate::theme::INTER.into()),
            );
            let at = ts.pos(*anchor);
            if style.text_pill {
                let galley =
                    painter.layout_no_wrap(content.clone(), font.clone(), Color32::WHITE);
                // `split` e não `lines`: uma linha final vazia conta.
                let line_height =
                    galley.size().y / content.split('\n').count().max(1) as f32;
                let (pad, radius) = text_pill_metrics(line_height);
                painter.rect_filled(
                    Rect::from_min_size(at, galley.size()).expand(pad),
                    CornerRadius::same(radius.round() as u8),
                    color32(TEXT_PILL_COLOR),
                );
            }
            painter.text(at, Align2::LEFT_TOP, content, font, color32(style.color));
        }
    }
}

pub(super) fn color32(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

// ---------------------------------------------------------------------------
// Seleção (ferramenta Mover)
// ---------------------------------------------------------------------------

/// Layout do texto exatamente como `paint_shape` o pinta (fonte, tamanho em
/// pontos de tela) — usado para hit-test e caixa de seleção da variante Text.
pub(super) fn text_galley(
    ctx: &egui::Context,
    content: &str,
    font_size: f32,
    ts: ToScreen,
) -> std::sync::Arc<egui::Galley> {
    ctx.fonts(|f| {
        f.layout_no_wrap(
            content.to_owned(),
            FontId::new(ts.len(font_size), egui::FontFamily::Name(crate::theme::INTER.into())),
            Color32::WHITE,
        )
    })
}

/// Hit-test de uma anotação no espaço da imagem; a variante Text é medida com
/// a fonte real para a caixa coincidir com o que está na tela.
pub(super) fn hit_test(ctx: &egui::Context, layer: &Layer, p: Point, tol: f32, ts: ToScreen) -> bool {
    let text_size = match &layer.shape {
        Shape::Text { content, .. } => {
            let size = text_galley(ctx, content, layer.style.font_size, ts).size();
            (size.x / ts.scale, size.y / ts.scale)
        }
        _ => (0.0, 0.0),
    };
    layer.hit_test(p, tol, text_size)
}

/// Retângulo (em pontos de tela) que envolve a anotação, traço incluído.
pub(super) fn shape_screen_bbox(ctx: &egui::Context, layer: &Layer, ts: ToScreen) -> Rect {
    let half_stroke = ts.len(layer.style.stroke_width) / 2.0;
    match &layer.shape {
        Shape::Freehand { highlight, .. } => {
            // O traço do marca-texto é bem mais largo que o do estilo.
            let (width, _) = stroke_appearance(&layer.style, *highlight);
            let (min, max) = layer.bbox().unwrap_or((Point::new(0.0, 0.0), Point::new(0.0, 0.0)));
            Rect::from_min_max(ts.pos(min), ts.pos(max)).expand(ts.len(width) / 2.0)
        }
        Shape::Marker { center, .. } => {
            let geo = marker_geometry(layer.style.stroke_width);
            Rect::from_center_size(ts.pos(*center), Vec2::splat(ts.len(geo.radius) * 2.0))
        }
        Shape::Redaction { min, max, .. } => Rect::from_min_max(ts.pos(*min), ts.pos(*max)),
        Shape::Spotlight { center, rx, ry } => Rect::from_center_size(
            ts.pos(*center),
            Vec2::new(ts.len(*rx), ts.len(*ry)) * 2.0,
        ),
        Shape::Line { a, b } | Shape::Arrow { a, b } => {
            Rect::from_two_pos(ts.pos(*a), ts.pos(*b)).expand(half_stroke)
        }
        Shape::Rect { min, max } => {
            Rect::from_min_max(ts.pos(*min), ts.pos(*max)).expand(half_stroke)
        }
        Shape::Ellipse { center, rx, ry } => {
            Rect::from_center_size(ts.pos(*center), Vec2::new(ts.len(*rx), ts.len(*ry)) * 2.0)
                .expand(half_stroke)
        }
        Shape::Text { anchor, content } => {
            let size = text_galley(ctx, content, layer.style.font_size, ts).size();
            Rect::from_min_size(ts.pos(*anchor), size)
        }
    }
}

pub(super) fn draw_handles(painter: &egui::Painter, layer: &Layer, ts: ToScreen, fill: Color32) {
    for (_, at) in layer.handles(HANDLE_EDGE_ROOM_PTS / ts.scale) {
        painter.circle(
            ts.pos(at),
            HANDLE_RADIUS_PTS,
            fill,
            Stroke::new(1.0_f32, Color32::WHITE),
        );
    }
}

/// Moldura tracejada ao redor da anotação selecionada.
pub(super) fn draw_selection_outline(
    ctx: &egui::Context,
    painter: &egui::Painter,
    layer: &Layer,
    ts: ToScreen,
    color: Color32,
) {
    let rect = shape_screen_bbox(ctx, layer, ts).expand(5.0);
    let stroke = Stroke::new(1.5_f32, color);
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    for i in 0..4 {
        painter.extend(egui::Shape::dashed_line(
            &[corners[i], corners[(i + 1) % 4]],
            stroke,
            4.0,
            3.0,
        ));
    }
}

// ---------------------------------------------------------------------------

/// Escurece o que ficará de fora, contorna a área mantida e anota as suas
/// dimensões. `confirmed` distingue a região já solta (aguardando Enter) do
/// arrasto ainda em curso.
pub(super) fn draw_crop_overlay(
    painter: &egui::Painter,
    image_rect: Rect,
    selection: Rect,
    size_px: (f32, f32),
    confirmed: bool,
) {
    let selection = selection.intersect(image_rect);
    let veil = Color32::from_black_alpha(150);
    // Faixas acima, abaixo, à esquerda e à direita da área mantida.
    let bands = [
        Rect::from_min_max(
            image_rect.min,
            Pos2::new(image_rect.max.x, selection.min.y),
        ),
        Rect::from_min_max(
            Pos2::new(image_rect.min.x, selection.max.y),
            image_rect.max,
        ),
        Rect::from_min_max(
            Pos2::new(image_rect.min.x, selection.min.y),
            Pos2::new(selection.min.x, selection.max.y),
        ),
        Rect::from_min_max(
            Pos2::new(selection.max.x, selection.min.y),
            Pos2::new(image_rect.max.x, selection.max.y),
        ),
    ];
    for band in bands {
        if band.is_positive() {
            painter.rect_filled(band, CornerRadius::ZERO, veil);
        }
    }

    painter.rect_stroke(
        selection,
        CornerRadius::ZERO,
        Stroke::new(1.5_f32, Color32::WHITE),
        StrokeKind::Middle,
    );

    let (w, h) = size_px;
    let label = if confirmed {
        format!("{} × {} — Enter aplica · Esc cancela", w.round(), h.round())
    } else {
        format!("{} × {}", w.round(), h.round())
    };
    // Acima da área quando há espaço; dentro dela quando o recorte encosta
    // no topo da imagem.
    let (anchor, pos) = if selection.min.y - image_rect.min.y > 22.0 {
        (Align2::LEFT_BOTTOM, selection.left_top() + Vec2::new(0.0, -4.0))
    } else {
        (Align2::LEFT_TOP, selection.left_top() + Vec2::new(4.0, 4.0))
    };
    painter.text(
        pos,
        anchor,
        label,
        FontId::proportional(12.0),
        Color32::WHITE,
    );
}

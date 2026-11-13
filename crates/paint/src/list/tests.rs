use egui::{LayerId, Painter, Shape};

use super::*;

const ROW_H: f32 = 50.0;

fn canvas() -> Canvas {
    Canvas::new(&Painter::new(egui::Context::default(), LayerId::background(), Rect::EVERYTHING))
}

fn with_text(check: impl Fn(&mut Canvas)) {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| check(&mut Canvas::new(ui.painter())));
    output.textures_delta.clear();
}

fn texts(canvas: &Canvas) -> Vec<(String, egui::Pos2, Color32)> {
    canvas
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some((text.galley.text().to_owned(), text.pos, text.fallback_color)),
            _ => None,
        })
        .collect()
}

fn hairlines(canvas: &Canvas) -> Vec<Rect> {
    canvas
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rect) if rect.fill == white(HAIRLINE) => Some(rect.rect),
            _ => None,
        })
        .collect()
}

#[test]
fn rows_stack_at_their_own_heights() {
    let card = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 2.0 * ROW_H + 70.0));
    let rows = stacked(card, &[ROW_H, 70.0, ROW_H]);
    assert_eq!(rows.iter().map(Rect::top).collect::<Vec<_>>(), [20.0, 20.0 + ROW_H, 20.0 + ROW_H + 70.0]);
    assert_eq!(rows[1].height(), 70.0);
    assert_eq!(rows[2].bottom(), card.bottom());
    assert!(rows.iter().all(|row| row.left() == 10.0 && row.width() == 300.0));
}

#[test]
fn equal_rows_split_the_card_evenly() {
    let card = Rect::from_min_size(pos2(16.0, 158.0), vec2(340.0, 4.0 * ROW_H));
    let height = card.height() / 4.0;
    let even: Vec<Rect> = (0..4)
        .map(|index| Rect::from_min_size(card.min + vec2(0.0, index as f32 * height), vec2(card.width(), height)))
        .collect();
    assert_eq!(stacked(card, &[height; 4]), even);
    assert!(stacked(card, &[]).is_empty());
}

#[test]
fn card_has_hairlines_between_rows() {
    let mut canvas = canvas();
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 3.0 * ROW_H));
    let rows = card(&mut canvas, rect, &[ROW_H; 3]);
    assert_eq!(rows.len(), 3);
    assert!(matches!(canvas.shapes[0], Shape::Mesh(_)), "the glass comes first");
    let lines = hairlines(&canvas);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], Rect::from_min_size(pos2(INSET, ROW_H - 0.5), vec2(300.0 - INSET, 1.0)));
    assert_eq!(lines[1].top(), 2.0 * ROW_H - 0.5);
}

#[test]
fn single_row_card_has_no_hairline() {
    let mut one = canvas();
    card(&mut one, Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 2.0 * ROW_H)), &[2.0 * ROW_H]);
    assert!(hairlines(&one).is_empty());
    let mut none = canvas();
    assert!(card(&mut none, Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, ROW_H)), &[]).is_empty());
    assert_eq!(none.shapes.len(), 1, "just the glass");
}

#[test]
fn divider_is_inset_on_row_bottom() {
    let mut canvas = canvas();
    divider(&mut canvas, Rect::from_min_max(pos2(10.0, 0.0), pos2(110.0, 40.0)), 6.0);
    assert_eq!(hairlines(&canvas), [Rect::from_min_max(pos2(16.0, 39.5), pos2(110.0, 40.5))]);
    hairline(&mut canvas, Rect::from_min_size(pos2(0.0, 0.0), vec2(5.0, 1.0)));
    assert_eq!(hairlines(&canvas).len(), 2);
}

#[test]
fn lines_default_to_secondary_end_cut() {
    let lines = Lines::new("Title", "Detail");
    assert_eq!(lines, Lines { title: "Title", detail: "Detail", tone: secondary_label(), cut: Cut::End });
    let changed = lines.tone(Color32::RED).cut(Cut::Start);
    assert_eq!((changed.tone, changed.cut, changed.title), (Color32::RED, Cut::Start, "Title"));
    assert_eq!(Cut::default(), Cut::End);
}

#[test]
fn lone_title_is_centred() {
    with_text(|canvas| {
        two_lines(canvas, 20.0, 300.0, 100.0, &Lines::new("Open at login", ""));
        let drawn = texts(canvas);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, "Open at login");
        assert_eq!(drawn[0].2, label());
        assert!((drawn[0].1.y + canvas.text_size("Open at login", font::body()).y * 0.5 - 100.0).abs() < 1e-3);
    });
}

#[test]
fn detail_sits_below_title() {
    with_text(|canvas| {
        two_lines(canvas, 20.0, 300.0, 100.0, &Lines::new("Title", "Detail").tone(Color32::RED));
        let drawn = texts(canvas);
        assert_eq!(drawn.iter().map(|(text, ..)| text.as_str()).collect::<Vec<_>>(), ["Title", "Detail"]);
        assert_eq!((drawn[0].2, drawn[1].2), (label(), Color32::RED));
        assert!(drawn[0].1.y < 100.0 && drawn[1].1.y > drawn[0].1.y);
        assert!(drawn.iter().all(|(_, at, _)| at.x == 20.0));
    });
}

#[test]
fn long_lines_are_cut_to_fit() {
    with_text(|canvas| {
        let path = "C:/Users/someone/Documents/GitHub/notchbase/target/debug/tools";
        let lines = Lines::new("A rather long title that cannot fit", path).cut(Cut::Start);
        two_lines(canvas, 0.0, 90.0, 50.0, &lines);
        let drawn = texts(canvas);
        assert!(drawn[0].0.ends_with('…'), "{drawn:?}");
        assert!(drawn[1].0.starts_with('…') && drawn[1].0.ends_with("tools"), "{drawn:?}");
        assert!(canvas.text_width(drawn[1].0.as_str(), font::caption()) <= 90.0);
        let ended = Lines::new("Title", path);
        two_lines(canvas, 0.0, 90.0, 50.0, &ended);
        assert!(texts(canvas)[3].0.starts_with("C:/") && texts(canvas)[3].0.ends_with('…'));
    });
}

#[test]
fn switch_row_layout() {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let mut canvas = Canvas::new(ui.painter());
        let row = Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, ROW_H));
        let lines = Lines::new("Automatic updates", "Install new versions when the notch is idle");
        assert!(!switch_row(ui, &mut canvas, row, Id::new("row"), &lines, true, false));
        let drawn = texts(&canvas);
        assert!(drawn.iter().all(|(_, at, _)| at.x == INSET));
        let knob = canvas.shapes.iter().rev().find_map(|shape| match shape {
            Shape::Circle(circle) => Some(circle.center),
            _ => None,
        });
        let track = Rect::from_center_size(pos2(320.0 - INSET - SWITCH.x * 0.5, ROW_H * 0.5), SWITCH);
        assert!(knob.is_some_and(|knob| track.contains(knob)), "{knob:?}");
        let room = track.left() - TEXT_GAP - INSET;
        assert!(canvas.text_width(drawn[0].0.as_str(), font::body()) <= room);
        assert!(canvas.text_width(drawn[1].0.as_str(), font::caption()) <= room);
    });
    output.textures_delta.clear();
}

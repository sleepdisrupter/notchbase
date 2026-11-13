use egui::{LayerId, Painter, Rect, Shape, pos2};

use super::*;

const ALL: [Glyph; 76] = [
    Glyph::Speaker,
    Glyph::SpeakerMuted,
    Glyph::Sun,
    Glyph::Headphones,
    Glyph::Display,
    Glyph::Microphone,
    Glyph::MicrophoneMuted,
    Glyph::Device,
    Glyph::Checkmark,
    Glyph::ChevronDown,
    Glyph::ChevronUp,
    Glyph::ChevronLeft,
    Glyph::Xmark,
    Glyph::Tray,
    Glyph::Exclamation,
    Glyph::Convert,
    Glyph::Compress,
    Glyph::Combine,
    Glyph::Split,
    Glyph::Gear,
    Glyph::Note,
    Glyph::Play,
    Glyph::Pause,
    Glyph::Backward,
    Glyph::Forward,
    Glyph::Keyboard,
    Glyph::Mouse,
    Glyph::Gamepad,
    Glyph::Phone,
    Glyph::Watch,
    Glyph::Bluetooth,
    Glyph::ChevronRight,
    Glyph::Plus,
    Glyph::Minus,
    Glyph::Folder,
    Glyph::Stop,
    Glyph::Record,
    Glyph::Pipette,
    Glyph::Viewfinder,
    Glyph::Clock,
    Glyph::Bell,
    Glyph::Star,
    Glyph::Grid,
    Glyph::Gauge,
    Glyph::Timer,
    Glyph::Calendar,
    Glyph::Video,
    Glyph::Cloud,
    Glyph::Link,
    Glyph::Spark,
    Glyph::AirDrop,
    Glyph::Bolt,
    Glyph::Battery,
    Glyph::Lock,
    Glyph::Unlock,
    Glyph::Download,
    Glyph::Cup,
    Glyph::Moon,
    Glyph::Photo,
    Glyph::Camera,
    Glyph::Globe,
    Glyph::Layers,
    Glyph::Frames,
    Glyph::AppIcon,
    Glyph::Document,
    Glyph::Film,
    Glyph::Waveform,
    Glyph::Bars,
    Glyph::Undo,
    Glyph::Redo,
    Glyph::Search,
    Glyph::Brush,
    Glyph::Text,
    Glyph::Pin,
    Glyph::Unpin,
    Glyph::Shield,
];

// Shape count, point count and bounds of every glyph at size 20 around (50, 50).
// When you change a glyph on purpose, update its row here.
const DRAWN: [(Glyph, usize, usize, [f32; 4]); 76] = [
    (Glyph::Speaker, 3, 47, [40.30, 40.86, 59.70, 59.14]),
    (Glyph::SpeakerMuted, 3, 17, [40.30, 40.86, 60.10, 59.14]),
    (Glyph::Sun, 9, 17, [39.90, 39.90, 60.10, 60.10]),
    (Glyph::Headphones, 3, 19, [40.60, 42.50, 59.40, 57.00]),
    (Glyph::Display, 3, 5, [41.40, 42.80, 58.60, 58.10]),
    (Glyph::Microphone, 3, 20, [43.50, 42.20, 56.50, 59.30]),
    (Glyph::MicrophoneMuted, 4, 22, [41.10, 40.70, 58.90, 59.30]),
    (Glyph::Device, 3, 3, [43.80, 42.20, 56.20, 57.80]),
    (Glyph::Checkmark, 1, 3, [41.72, 43.72, 58.68, 56.48]),
    (Glyph::ChevronDown, 1, 3, [43.10, 46.70, 56.90, 54.50]),
    (Glyph::ChevronUp, 1, 3, [43.10, 45.50, 56.90, 53.30]),
    (Glyph::ChevronLeft, 1, 3, [45.50, 43.10, 53.30, 56.90]),
    (Glyph::Xmark, 2, 4, [43.90, 43.90, 56.10, 56.10]),
    (Glyph::Tray, 3, 14, [40.30, 39.90, 59.70, 58.90]),
    (Glyph::Exclamation, 2, 3, [48.56, 41.63, 51.44, 58.24]),
    (Glyph::Convert, 4, 10, [41.50, 43.70, 58.50, 56.30]),
    (Glyph::Compress, 4, 10, [41.10, 41.10, 58.90, 58.90]),
    (Glyph::Combine, 2, 6, [41.70, 41.70, 57.40, 57.40]),
    (Glyph::Split, 2, 2, [43.00, 43.00, 57.00, 57.00]),
    (Glyph::Gear, 9, 17, [40.00, 40.00, 60.00, 60.00]),
    (Glyph::Note, 5, 15, [42.40, 40.21, 57.39, 58.80]),
    (Glyph::Play, 1, 7, [43.50, 40.74, 59.81, 59.26]),
    (Glyph::Pause, 2, 2, [44.00, 42.00, 56.00, 58.00]),
    (Glyph::Backward, 2, 14, [39.46, 43.10, 60.10, 56.90]),
    (Glyph::Forward, 2, 14, [39.90, 43.10, 60.54, 56.90]),
    (Glyph::Keyboard, 10, 11, [40.80, 44.20, 59.20, 55.80]),
    (Glyph::Mouse, 2, 3, [44.80, 41.10, 55.20, 58.20]),
    (Glyph::Gamepad, 5, 7, [40.40, 44.80, 59.60, 56.00]),
    (Glyph::Phone, 2, 3, [45.20, 41.40, 54.80, 58.60]),
    (Glyph::Watch, 3, 3, [45.00, 40.80, 55.00, 59.20]),
    (Glyph::Bluetooth, 1, 6, [45.10, 41.10, 54.90, 58.90]),
    (Glyph::ChevronRight, 1, 3, [46.70, 43.10, 54.50, 56.90]),
    (Glyph::Plus, 2, 4, [42.12, 42.12, 57.88, 57.88]),
    (Glyph::Minus, 1, 2, [42.12, 48.92, 57.88, 51.08]),
    (Glyph::Folder, 2, 9, [40.30, 43.10, 59.70, 57.70]),
    (Glyph::Stop, 1, 1, [40.00, 40.00, 60.00, 60.00]),
    (Glyph::Record, 1, 1, [42.00, 42.00, 58.00, 58.00]),
    (Glyph::Pipette, 1, 49, [41.67, 41.93, 58.07, 58.33]),
    (Glyph::Viewfinder, 5, 13, [40.38, 40.38, 59.62, 59.62]),
    (Glyph::Clock, 2, 4, [40.70, 40.70, 59.30, 59.30]),
    (Glyph::Bell, 2, 21, [43.10, 41.90, 56.90, 58.60]),
    (Glyph::Star, 1, 21, [39.95, 40.04, 60.05, 59.15]),
    (Glyph::Grid, 4, 4, [42.60, 42.60, 57.40, 57.40]),
    (Glyph::Gauge, 3, 20, [40.73, 43.10, 59.27, 58.24]),
    (Glyph::Timer, 5, 9, [41.90, 40.30, 58.10, 59.70]),
    (Glyph::Calendar, 9, 11, [41.60, 39.90, 58.40, 58.00]),
    (Glyph::Video, 2, 10, [40.80, 44.18, 59.70, 55.82]),
    (Glyph::Cloud, 4, 4, [42.00, 43.00, 58.80, 55.20]),
    (Glyph::Link, 2, 44, [41.13, 41.13, 58.87, 58.87]),
    (Glyph::Spark, 2, 194, [39.17, 39.17, 60.83, 60.83]),
    (Glyph::AirDrop, 3, 35, [40.55, 40.90, 59.45, 59.01]),
    (Glyph::Bolt, 1, 13, [42.98, 39.49, 57.02, 60.50]),
    (Glyph::Battery, 3, 3, [41.20, 45.60, 59.40, 54.40]),
    (Glyph::Lock, 2, 20, [43.60, 42.70, 56.40, 58.40]),
    (Glyph::Unlock, 2, 20, [43.60, 39.90, 56.40, 58.40]),
    (Glyph::Download, 3, 7, [42.30, 40.30, 57.70, 58.90]),
    (Glyph::Cup, 4, 22, [42.40, 40.30, 57.90, 58.00]),
    (Glyph::Moon, 2, 100, [41.10, 41.14, 58.72, 58.87]),
    (Glyph::Photo, 3, 7, [41.20, 42.80, 58.80, 57.20]),
    (Glyph::Camera, 3, 3, [41.20, 42.40, 58.80, 57.60]),
    (Glyph::Globe, 3, 27, [40.70, 40.70, 59.30, 59.30]),
    (Glyph::Layers, 3, 15, [40.52, 41.04, 59.48, 59.70]),
    (Glyph::Frames, 3, 11, [41.20, 41.10, 60.10, 58.40]),
    (Glyph::AppIcon, 2, 2, [42.00, 42.00, 58.00, 58.00]),
    (Glyph::Document, 4, 12, [43.10, 40.30, 57.30, 59.70]),
    (Glyph::Film, 5, 9, [40.30, 42.30, 59.70, 57.70]),
    (Glyph::Waveform, 1, 33, [39.90, 42.82, 60.10, 57.18]),
    (Glyph::Bars, 5, 5, [41.10, 41.20, 58.90, 58.80]),
    (Glyph::Undo, 2, 22, [41.50, 42.90, 58.30, 56.70]),
    (Glyph::Redo, 2, 22, [41.70, 42.90, 58.50, 56.70]),
    (Glyph::Search, 2, 3, [42.30, 42.30, 58.90, 58.90]),
    (Glyph::Brush, 1, 37, [40.50, 44.30, 59.50, 55.70]),
    (Glyph::Text, 3, 8, [43.10, 42.50, 56.90, 57.50]),
    (Glyph::Pin, 4, 36, [42.56, 40.30, 57.44, 60.50]),
    (Glyph::Unpin, 6, 47, [40.70, 40.30, 59.30, 60.50]),
    (Glyph::Shield, 2, 29, [42.30, 40.70, 57.70, 59.70]),
];

fn drawn(g: Glyph) -> Vec<Shape> {
    let painter = Painter::new(egui::Context::default(), LayerId::background(), Rect::EVERYTHING);
    let mut canvas = Canvas::new(&painter);
    paint(&mut canvas, g, pos2(50.0, 50.0), 20.0, Color32::WHITE);
    canvas.shapes
}

fn points(shape: &Shape) -> usize {
    match shape {
        Shape::Mesh(mesh) => mesh.vertices.len(),
        Shape::Path(path) => path.points.len(),
        Shape::LineSegment { .. } => 2,
        _ => 1,
    }
}

fn bounds(shapes: &[Shape]) -> Rect {
    shapes.iter().fold(Rect::NOTHING, |all, shape| all.union(shape.visual_bounding_rect()))
}

#[test]
fn glyphs_match_their_snapshot() {
    for (g, shapes, total, [left, top, right, bottom]) in DRAWN {
        let drawn = drawn(g);
        assert_eq!((drawn.len(), drawn.iter().map(points).sum::<usize>()), (shapes, total), "{g:?}");
        let rect = bounds(&drawn);
        let near = |a: f32, b: f32| (a - b).abs() < 0.006;
        let fits = near(rect.left(), left) && near(rect.top(), top);
        assert!(fits && near(rect.right(), right) && near(rect.bottom(), bottom), "{g:?}: {rect:?}");
    }
}

#[test]
fn every_glyph_fits_its_box() {
    for g in ALL {
        let drawn = drawn(g);
        assert!(!drawn.is_empty(), "{g:?}");
        let rect = bounds(&drawn);
        assert!(
            Rect::from_center_size(pos2(50.0, 50.0), egui::vec2(22.0, 22.0)).contains_rect(rect),
            "{g:?}: {rect:?}"
        );
    }
}

#[test]
fn table_names_every_glyph_once() {
    let named: Vec<Glyph> = DRAWN.iter().map(|(g, ..)| *g).collect();
    assert_eq!(named, ALL);
}

#[test]
fn glyph_uses_the_given_color() {
    let painter = Painter::new(egui::Context::default(), LayerId::background(), Rect::EVERYTHING);
    let mut canvas = Canvas::new(&painter);
    paint(&mut canvas, Glyph::Record, pos2(10.0, 10.0), 10.0, Color32::RED);
    assert!(matches!(&canvas.shapes[..], [Shape::Circle(dot)] if dot.fill == Color32::RED && dot.radius == 4.0));
}

#[test]
fn turning_chevron_passes_flat() {
    let painter = Painter::new(egui::Context::default(), LayerId::background(), Rect::EVERYTHING);
    let turned = |up: f32| {
        let mut canvas = Canvas::new(&painter);
        chevron_turning(&mut canvas, pos2(50.0, 50.0), 20.0, Color32::WHITE, up);
        canvas.shapes
    };
    let line = |shapes: &[Shape]| match &shapes[0] {
        Shape::Path(path) => path.points.clone(),
        other => panic!("a line, not {other:?}"),
    };
    assert_eq!(line(&turned(0.0)), line(&drawn(Glyph::ChevronDown)));
    assert_eq!(line(&turned(1.0)), line(&drawn(Glyph::ChevronUp)));
    assert!(line(&turned(0.5)).iter().all(|point| (point.y - 50.0).abs() < 1e-4), "halfway it lies flat");
}

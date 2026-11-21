use super::*;
use crate::layout::{Computer, Rect, push};

fn mac() -> Computer {
    Computer::new("mac", vec![Rect::new(0.0, 0.0, 1512.0, 982.0)], 1.0, [0.0, 0.0])
}

fn pc(at: [f64; 2]) -> Computer {
    // 2560x1440 at 150%: 1706.7 x 960 points.
    Computer::new("pc", vec![Rect::new(0.0, 0.0, 2560.0, 1440.0)], 1.5, at)
}

/// A PC with its main screen and a smaller one to its left, raised up; the Mac left of both.
fn stepped() -> Vec<Computer> {
    let pc = Computer::new(
        "pc",
        vec![Rect::new(0.0, 0.0, 2560.0, 1440.0), Rect::new(-1920.0, -600.0, 1920.0, 1080.0)],
        1.5,
        [1512.0, -400.0],
    );
    vec![mac(), pc]
}

fn named(desk: &[Computer]) -> Vec<Vec<Screen>> {
    let mut colour = 0;
    desk.iter()
        .map(|computer| {
            (0..computer.displays.len())
                .map(|i| {
                    colour += 1;
                    Screen {
                        display: i as u8,
                        colour: colour - 1,
                        title: computer.id.clone(),
                        detail: format!("Screen {}", i + 1),
                        label: format!("{} {}", computer.id, i + 1),
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn side_by_side_the_edge_leads_over_as_far_as_the_other_screen_goes() {
    let desk = [mac(), pc([1512.0, 0.0])];
    let spans = spans(&desk, 0, 0, Edge::Right);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!((spans[0].computer, spans[0].display), (1, 0));
    assert!(spans[0].from.abs() < 1e-9);
    assert!((spans[0].to - 960.0 / 982.0).abs() < 1e-6, "{spans:?}");
    // From the PC, all of its left edge leads back.
    let back = super::spans(&desk, 1, 0, Edge::Left);
    assert_eq!(back.len(), 1);
    assert!((back[0].to - 1.0).abs() < 1e-9);
    // Nothing is above or below either.
    assert!(super::spans(&desk, 0, 0, Edge::Top).is_empty());
    assert!(super::spans(&desk, 1, 0, Edge::Right).is_empty());
}

#[test]
fn a_computers_own_screens_lead_to_each_other() {
    let two = Computer::new(
        "pc",
        vec![Rect::new(0.0, 0.0, 1920.0, 1080.0), Rect::new(1920.0, 540.0, 1920.0, 1080.0)],
        1.0,
        [0.0, 0.0],
    );
    let desk = [two];
    let spans = spans(&desk, 0, 0, Edge::Right);
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].computer, spans[0].display), (0, 1));
    assert!((spans[0].from - 0.5).abs() < 1e-9 && (spans[0].to - 1.0).abs() < 1e-9, "{spans:?}");
}

#[test]
fn every_band_leads_where_the_cursor_really_goes() {
    for desk in [stepped(), vec![mac(), pc([1512.0, 300.0])], vec![mac(), pc([-300.0, -960.0])]] {
        for (on, computer) in desk.iter().enumerate() {
            for (index, display) in computer.displays.iter().enumerate() {
                for edge in Edge::ALL {
                    for span in spans(&desk, on, index, edge) {
                        if span.computer == on {
                            continue;
                        }
                        let middle = (span.from + span.to) / 2.0;
                        let (start, end) = if edge.horizontal() {
                            (display.y, display.bottom())
                        } else {
                            (display.x, display.right())
                        };
                        let along = start + middle * (end - start);
                        let at = match edge {
                            Edge::Left => [display.x, along],
                            Edge::Right => [display.right() - 1.0, along],
                            Edge::Top => [along, display.y],
                            Edge::Bottom => [along, display.bottom() - 1.0],
                        };
                        let [sx, sy] = edge.step();
                        let Some(Step::Cross { to, at: landing }) = push(&desk, on, at, [sx, sy], 0.0) else {
                            panic!("no crossing at {at:?} on {edge:?} of {on}/{index}: {span:?}")
                        };
                        assert_eq!(to, span.computer, "{edge:?} of {on}/{index} at {at:?}");
                        assert!(desk[to].displays[span.display].contains(landing));
                    }
                }
            }
        }
    }
}

#[test]
fn a_stepped_edge_has_a_band_for_each_screen_it_faces() {
    let desk = stepped();
    let spans = spans(&desk, 0, 0, Edge::Right);
    // The external sits from -400 to 320 on the desk, the main screen from 0 to 960: the Mac
    // (0 to 982) faces the external down to 320 (the nearer one), then the main screen to 960.
    assert_eq!(spans.len(), 2, "{spans:?}");
    assert_eq!((spans[0].computer, spans[0].display), (1, 1));
    assert_eq!((spans[1].computer, spans[1].display), (1, 0));
    assert!((spans[0].to - 320.0 / 982.0).abs() < 1e-6, "{spans:?}");
    assert!((spans[1].to - 960.0 / 982.0).abs() < 1e-6, "{spans:?}");
}

#[test]
fn guides_name_and_colour_where_each_band_goes() {
    let desk = [mac(), pc([1512.0, 0.0])];
    let screens = named(&desk);
    let guides = guides(&desk, &screens);
    assert_eq!(guides.len(), 2);
    let mac = &guides[0][0];
    assert_eq!(mac.colour, 0);
    assert_eq!(mac.bands.len(), 1);
    assert_eq!(mac.bands[0].colour, 1);
    assert_eq!(mac.bands[0].label, "pc 1");
    assert_eq!(mac.bands[0].edge, Edge::Right);
    let pc = &guides[1][0];
    assert_eq!(pc.bands[0].label, "mac 1");
    assert_eq!(pc.bands[0].edge, Edge::Left);
}

#[test]
fn near_an_edge_the_cursor_marks_where_it_leaves_and_comes_in() {
    let desk = [mac(), pc([1512.0, 0.0])];
    let screens = named(&desk);
    let marks = marks(&desk, &screens, 0, [1400.0, 491.0], REACH);
    assert_eq!(marks.len(), 2, "{marks:?}");
    let (on, leaves) = marks[0];
    assert_eq!(on, 0);
    assert!(!leaves.arrives && leaves.edge == Edge::Right);
    assert!((leaves.along - 0.5).abs() < 0.001, "{leaves:?}");
    let (to, arrives) = marks[1];
    assert_eq!(to, 1);
    assert!(arrives.arrives && arrives.edge == Edge::Left);
    // 491 points down the Mac is 736.5 pixels down the PC's 1440.
    assert!((arrives.along - 736.5 / 1440.0).abs() < 0.001, "{arrives:?}");
    // Far from the edge, or where nothing is across, nothing is marked.
    assert!(super::marks(&desk, &screens, 0, [700.0, 491.0], REACH).is_empty());
    assert!(super::marks(&desk, &screens, 0, [1500.0, 975.0], REACH).is_empty());
}

#[test]
fn from_the_other_side_the_marks_come_back_to_the_same_spot() {
    let desk = [mac(), pc([1512.0, 0.0])];
    let screens = named(&desk);
    let there = marks(&desk, &screens, 0, [1511.0, 300.0], REACH);
    let back = marks(&desk, &screens, 1, [3.0, 450.0], REACH);
    let arrive_there = there.iter().find(|(c, m)| *c == 1 && m.arrives).unwrap().1;
    let leave_back = back.iter().find(|(c, m)| *c == 1 && !m.arrives).unwrap().1;
    assert!((arrive_there.along - leave_back.along).abs() < 0.001);
}

#[test]
fn a_guide_stays_on_its_screen() {
    let guide = Guide {
        display: 0,
        colour: 3,
        title: "MacBook Pro".into(),
        detail: "Main screen · 1470 × 956".into(),
        bands: vec![
            Band { edge: Edge::Top, from: 0.0, to: 1.0, colour: 1, label: "LG".into() },
            Band { edge: Edge::Left, from: 0.9, to: 1.0, colour: 2, label: "A rather long name for a PC".into() },
        ],
    };
    let size = [1470.0, 956.0];
    let mut shapes = scene(size, &guide);
    shapes.extend(mark_scene(size, &[Mark { display: 0, edge: Edge::Bottom, along: 0.99, arrives: true }]));
    for shape in &shapes {
        let (Shape::Rect { at, .. } | Shape::Text { at, .. }) = shape;
        assert!(at[0] >= -3.0 && at[1] >= -3.0, "{shape:?}");
        assert!(at[0] + at[2] <= size[0] + 3.0 && at[1] + at[3] <= size[1] + 3.0, "{shape:?}");
    }
    let words: Vec<&str> = shapes
        .iter()
        .filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.as_str()) } else { None })
        .collect();
    assert!(words.contains(&"MacBook Pro"));
    assert!(words.contains(&"↑  LG"));
    assert!(words.contains(&"Cursor comes in here"));
}

/// Writes a guide's shapes to ROAM_GUIDE_PREVIEW, for looking at it while working on it.
#[test]
#[ignore = "a preview, not a check"]
fn preview_a_guide() {
    let Ok(path) = std::env::var("ROAM_GUIDE_PREVIEW") else { return };
    let size = [1470.0, 956.0];
    let guide = Guide {
        display: 0,
        colour: 1,
        title: "John's MacBook Air".into(),
        detail: "Main screen · 1470 × 956".into(),
        bands: vec![
            Band { edge: Edge::Top, from: 0.04, to: 1.0, colour: 2, label: "MacBook's screen".into() },
            Band { edge: Edge::Left, from: 0.0, to: 0.7, colour: 0, label: "DESKTOP-FTP7GKN".into() },
        ],
    };
    let mut shapes = scene(size, &guide);
    shapes.extend(mark_scene(size, &[Mark { display: 0, edge: Edge::Left, along: 0.42, arrives: true }]));
    let json: Vec<serde_json::Value> = shapes
        .iter()
        .map(|s| match s {
            Shape::Rect { at, fill, radius } => serde_json::json!({"rect": at, "fill": fill, "radius": radius}),
            Shape::Text { at, text, size, colour, bold } => {
                serde_json::json!({"text": text, "at": at, "size": size, "colour": colour, "bold": bold})
            }
        })
        .collect();
    std::fs::write(path, serde_json::json!({"size": size, "shapes": json}).to_string()).unwrap();
}

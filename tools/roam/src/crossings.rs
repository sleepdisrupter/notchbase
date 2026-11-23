//! Where the cursor crosses between screens on the desk, edge by edge, for the editor to draw.

use crate::layout::{Computer, Edge, Rect, Step, beyond};

/// A stretch of an edge, 0 to 1 along it, from which the cursor goes to display `display` of
/// computer `computer` on the desk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub from: f64,
    pub to: f64,
    pub computer: usize,
    pub display: usize,
}

fn along_of(edge: Edge, rect: &Rect) -> (f64, f64) {
    if edge.horizontal() { (rect.y, rect.bottom()) } else { (rect.x, rect.right()) }
}

/// The last spot on `display`'s `edge` at `along` (in its computer's units): where a cursor
/// pushing against that edge rests.
fn edge_spot(display: &Rect, edge: Edge, along: f64) -> [f64; 2] {
    match edge {
        Edge::Left => [display.x, along],
        Edge::Right => [display.right() - 1.0, along],
        Edge::Top => [along, display.y],
        Edge::Bottom => [along, display.bottom() - 1.0],
    }
}

/// Where a cursor resting at `at` on `edge` of display `index` of computer `on` goes when pushed
/// through it: which computer, which of its displays, and where (in that computer's units). The
/// system takes it to its own screen beside; Roam to another computer's, as `layout` decides.
fn across(desk: &[Computer], on: usize, index: usize, at: [f64; 2], edge: Edge) -> Option<(usize, usize, [f64; 2])> {
    let computer = desk.get(on)?;
    let [sx, sy] = edge.step();
    let next = [at[0] + sx, at[1] + sy];
    if let Some(own) = computer.displays.iter().enumerate().position(|(i, d)| i != index && d.contains(next)) {
        return Some((on, own, next));
    }
    let Some(Step::Cross { to, at }) = beyond(desk, on, at, edge, 0.0) else { return None };
    let display = desk[to].displays.iter().position(|d| d.contains(at))?;
    Some((to, display, at))
}

/// The stretches of `edge` of display `index` of computer `on` that lead to another screen,
/// in order along it.
pub fn spans(desk: &[Computer], on: usize, index: usize, edge: Edge) -> Vec<Span> {
    let Some(computer) = desk.get(on) else { return Vec::new() };
    let (Some(native), Some(placed)) = (computer.displays.get(index), computer.desk_displays().get(index).copied())
    else {
        return Vec::new();
    };
    let (start, end) = along_of(edge, &placed);
    let length = end - start;
    if length <= 0.0 {
        return Vec::new();
    }
    let mut cuts = vec![start, end];
    for other in desk {
        for rect in other.desk_displays() {
            let (a, b) = along_of(edge, &rect);
            cuts.extend([a, b].into_iter().filter(|v| *v > start && *v < end));
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let (native_start, _) = along_of(edge, native);
    let mut spans: Vec<Span> = Vec::new();
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b - a < 0.01 {
            continue;
        }
        let middle = native_start + ((a + b) / 2.0 - start) * computer.ppu;
        let Some((to, display, _)) = across(desk, on, index, edge_spot(native, edge, middle), edge) else { continue };
        let (from, until) = ((a - start) / length, (b - start) / length);
        match spans.last_mut() {
            Some(last) if last.computer == to && last.display == display && (last.to - from).abs() < 1e-9 => {
                last.to = until;
            }
            _ => spans.push(Span { from, to: until, computer: to, display }),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::push;

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
}

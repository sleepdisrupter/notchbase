//! Inks the marks onto a copy of the screenshot, for the clipboard.

use super::mark::Mark;
use super::raster;
use super::rect::PixelRect;
use crate::capture::Screenshot;

pub fn composite(base: &Screenshot, marks: &[Mark]) -> Screenshot {
    let mut inked = base.clone();
    let size = [inked.width, inked.height];
    for mark in marks {
        raster::apply(&mut inked.rgba, size, PixelRect::of_image(size), mark);
    }
    inked
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::annotate::Sheet;
    use crate::annotate::lettering::Lettering;
    use crate::annotate::testing::{Dice, as_image, block, lettering, line, textured, through};
    use crate::ui::color;

    const INKS: [egui::Color32; 5] = [color::RED, color::YELLOW, color::GREEN, color::PEARL, color::INK];
    const WIDTHS: [f32; 6] = [1.0, 2.5, 4.0, 9.0, 17.0, 40.0];
    const SIZE: [usize; 2] = [120, 90];
    const STEPS: usize = 240;
    const WIDEST_LABEL: u32 = 60;
    const TALLEST_LABEL: u32 = 30;
    const CHANGES: [&str; 4] = ["commit", "replace", "undo", "redo"];
    const EVERY_ACTION: [&str; 7] = ["commit", "replace", "undo", "redo", "lift", "settle", "nothing"];

    fn scribble(dice: &mut Dice) -> Mark {
        let samples: Vec<[f32; 2]> = (0..=dice.below(5)).map(|_| dice.spot(SIZE)).collect();
        through(INKS[dice.below(5) as usize], WIDTHS[dice.below(6) as usize], &samples).into()
    }

    fn label(dice: &mut Dice) -> Mark {
        let [x, y] = dice.spot(SIZE).map(|side| side.max(0.0) as usize);
        let [width, height] = [WIDEST_LABEL, TALLEST_LABEL].map(|most| 1 + dice.below(most) as usize);
        let levels = (0..width * height).map(|_| dice.below(256) as u8).collect();
        lettering(INKS[dice.below(5) as usize], PixelRect { x, y, width, height }, levels).into()
    }

    fn any_mark(dice: &mut Dice) -> Mark {
        match dice.below(8) {
            0..=2 => label(dice),
            3 => Lettering::empty().into(),
            _ => scribble(dice),
        }
    }

    fn any_place(dice: &mut Dice, sheet: &Sheet) -> usize {
        dice.below(sheet.marks().len() as u32 + 1) as usize
    }

    fn assert_nothing_changed_outside(dirty: PixelRect, before: &egui::ColorImage, sheet: &Sheet, step: &str) {
        for (index, (was, now)) in before.pixels.iter().zip(&sheet.image().pixels).enumerate() {
            let (x, y) = (index % SIZE[0], index / SIZE[0]);
            let inside = (dirty.x..dirty.right()).contains(&x) && (dirty.y..dirty.bottom()).contains(&y);
            assert!(inside || was == now, "{x},{y} changed outside the rectangle reported after {step}");
        }
    }

    fn assert_same(sheet: &Sheet, step: &str) {
        assert_shows_all_but(sheet, None, step);
    }

    fn assert_shows_all_but(sheet: &Sheet, hidden: Option<usize>, step: &str) {
        let shown = sheet.marks().iter().enumerate().filter(|(at, _)| hidden != Some(*at));
        let copied = composite(sheet.base(), &shown.map(|(_, mark)| mark.clone()).collect::<Vec<Mark>>());
        assert!(sheet.image() == as_image(&copied), "the clipboard and the window differ after {step}");
    }

    #[test]
    fn copy_matches_window_through_undo() {
        let mut sheet = Sheet::new(Arc::new(textured(SIZE[0], SIZE[1]))).expect("a sheet");
        let crossing: [Mark; 5] = [
            through(color::RED, 17.0, &[[10.0, 10.0], [110.0, 80.0]]),
            through(color::GREEN, 17.0, &[[110.0, 10.0], [10.0, 80.0]]),
            through(color::PEARL, 9.0, &[[60.0, 0.0], [55.0, 45.0], [70.0, 90.0]]),
            through(color::INK, 30.0, &[[60.0, 45.0]]),
            through(color::YELLOW, 4.0, &[[-30.0, 44.0], [60.0, 46.0], [200.0, 43.0]]),
        ]
        .map(Mark::from);
        for mark in &crossing {
            sheet.commit(mark.clone());
            assert_same(&sheet, "a commit");
        }
        for step in ["an undo", "a second undo", "a third undo"] {
            sheet.undo();
            assert_same(&sheet, step);
        }
        assert_eq!(sheet.marks(), &crossing[..2]);
        sheet.redo();
        assert_same(&sheet, "a redo");
        sheet.commit(crossing[4].clone());
        assert_same(&sheet, "a commit over undone work");
        sheet.undo();
        sheet.undo();
        sheet.undo();
        assert_same(&sheet, "three more undos");
        sheet.redo();
        sheet.redo();
        assert_same(&sheet, "two redos");
        assert_eq!(sheet.marks(), &crossing[..3]);
    }

    #[test]
    fn copy_matches_window_at_random() {
        let mut dice = Dice(2026);
        let mut sheet = Sheet::new(Arc::new(textured(SIZE[0], SIZE[1]))).expect("a sheet");
        let (mut history, mut hidden, mut put_back_by_a_change) = (Vec::new(), None, 0);
        for step in 0..STEPS {
            let before = sheet.image();
            let place = any_place(&mut dice, &sheet);
            let (action, redrawn) = match dice.below(12) {
                0 | 1 => ("undo", sheet.undo()),
                2 => ("redo", sheet.redo()),
                3 | 4 => ("replace", sheet.replace(place, any_mark(&mut dice))),
                5 | 6 => ("lift", sheet.lift(place)),
                7 => ("settle", Some(sheet.settle())),
                _ => ("commit", Some(sheet.commit(any_mark(&mut dice)))),
            };
            let changes = redrawn.is_some() && CHANGES.contains(&action);
            put_back_by_a_change += usize::from(changes && hidden.is_some());
            hidden = match (action, redrawn) {
                ("lift", Some(_)) => Some(place),
                (_, None) => hidden,
                _ => None,
            };
            history.push(if redrawn.is_some() { action } else { "nothing" });
            let step = format!("step {step}: {history:?}");
            assert_nothing_changed_outside(redrawn.unwrap_or_default(), &before, &sheet, &step);
            assert_shows_all_but(&sheet, hidden, &step);
        }
        sheet.settle();
        assert_same(&sheet, "the whole session");
        let words = sheet.marks().iter().filter(|mark| mark.text().is_some()).count();
        assert!(words > 2 && sheet.marks().len() > words + 2, "{words} of {}", sheet.marks().len());
        assert!(EVERY_ACTION.iter().all(|action| history.contains(action)), "{history:?}");
        assert!(put_back_by_a_change > 5, "{put_back_by_a_change}");
    }

    #[test]
    fn copy_leaves_screenshot_alone() {
        let base = textured(64, 48);
        let untouched = base.rgba.clone();
        let copied = composite(&base, &[line(color::RED, 8.0, [4.0, 4.0], [60.0, 44.0]).into()]);
        assert!(base.rgba == untouched && copied.rgba != untouched);
        assert_eq!((copied.width, copied.height, copied.rgba.len()), (64, 48, untouched.len()));
        assert!(copied.rgba.iter().skip(3).step_by(4).all(|&alpha| alpha == u8::MAX));
        assert!(composite(&base, &[]).rgba == untouched);
    }

    #[test]
    fn colours_stack_in_drawing_order() {
        let base = textured(64, 48);
        let red = Mark::from(line(color::RED, 10.0, [4.0, 24.0], [60.0, 24.0]));
        let green = Mark::from(line(color::GREEN, 10.0, [32.0, 4.0], [32.0, 44.0]));
        let green_on_top = as_image(&composite(&base, &[red.clone(), green.clone()]));
        let red_on_top = as_image(&composite(&base, &[green, red]));
        assert_eq!((green_on_top[(32, 24)], red_on_top[(32, 24)]), (color::GREEN, color::RED));
        assert_eq!((green_on_top[(10, 24)], green_on_top[(32, 8)]), (color::RED, color::GREEN));
    }

    #[test]
    fn copy_stacks_words_and_lines() {
        let base = textured(64, 48);
        let words = Mark::from(block(color::RED, PixelRect { x: 12, y: 10, width: 30, height: 12 }));
        let across = Mark::from(line(color::GREEN, 8.0, [4.0, 16.0], [60.0, 16.0]));
        let words_on_top = as_image(&composite(&base, &[across.clone(), words.clone()]));
        let line_on_top = as_image(&composite(&base, &[words, across]));
        assert_eq!((words_on_top[(20, 16)], line_on_top[(20, 16)]), (color::RED, color::GREEN));
        assert_eq!((words_on_top[(50, 16)], line_on_top[(20, 11)]), (color::GREEN, color::RED));
    }

    #[test]
    fn torn_screenshot_copied_as_is() {
        let torn = Screenshot { width: 8, height: 8, rgba: vec![7; 100] };
        let copied = composite(&torn, &[line(color::RED, 8.0, [0.0, 0.0], [8.0, 8.0]).into()]);
        assert_eq!(copied.rgba, vec![7; 100]);
    }
}

use egui::epaint::Primitive;
use egui::{Key, Modifiers, Rect, TextureId, pos2};

use super::super::super::picture::Picture;
use super::super::super::status::Reading;
use super::super::{across, shown};
use super::{Bench, GREETING, on_the_clipboard, open, placed, typing, untouched, with_the_text_tool};
use crate::annotate::Outcome;

const REDO: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);
const HAIR: f32 = 0.01;

fn textured(bench: &mut Bench) -> Vec<(TextureId, Rect, Rect)> {
    let drawn = bench.tessellated().into_iter().filter_map(|clipped| match clipped.primitive {
        Primitive::Mesh(mesh) => {
            let mut reach = Rect::NOTHING;
            mesh.vertices.iter().for_each(|vertex| reach.extend_with(vertex.pos));
            Some((mesh.texture_id, clipped.clip_rect, reach))
        }
        Primitive::Callback(_) => None,
    });
    drawn.collect()
}

fn masks(bench: &mut Bench, of_the_window: &[TextureId]) -> Vec<(Rect, Rect)> {
    let fresh = textured(bench).into_iter().filter(|(texture, ..)| !of_the_window.contains(texture));
    fresh.map(|(_, clip, reach)| (clip, reach)).collect()
}

fn on_screen(bench: &Bench, pixels: Rect) -> Rect {
    Rect::from_min_max(bench.fit().to_view(pixels.min), bench.fit().to_view(pixels.max))
}

#[test]
fn screen_matches_clipboard_with_words() {
    let mut bench = super::bench();
    let [from, middle, to] = across(&bench);
    bench.drag(&[from, middle, to]);
    bench.key(Key::T, Modifiers::NONE);
    bench.click(middle);
    bench.type_text(GREETING);
    bench.key(Key::Enter, Modifiers::SHIFT);
    bench.type_text("there");
    assert!(shown(&bench) == on_the_clipboard(&bench), "words still being typed are on neither");
    assert_eq!(bench.key(Key::Enter, Modifiers::NONE), [Outcome::Changed(2)]);
    let with_words = shown(&bench);
    assert!(with_words == on_the_clipboard(&bench) && with_words != untouched(&bench));
    bench.key(Key::Num3, Modifiers::NONE);
    bench.key(Key::B, Modifiers::NONE);
    bench.drag(&[pos2(to.x, from.y), middle, pos2(from.x, to.y)]);
    assert_eq!((bench.editor.job().marks.len(), placed(&bench).len()), (3, 1));
    assert!(shown(&bench) == on_the_clipboard(&bench) && shown(&bench) != with_words);
    bench.key(Key::Z, Modifiers::COMMAND);
    assert!(shown(&bench) == with_words && shown(&bench) == on_the_clipboard(&bench));
    bench.key(Key::Z, Modifiers::COMMAND);
    assert!(shown(&bench) == on_the_clipboard(&bench) && shown(&bench) != with_words, "the words are gone");
    bench.key(Key::Z, Modifiers::COMMAND);
    assert!(shown(&bench) == untouched(&bench));
    for step in 0..3 {
        bench.key(Key::Z, REDO);
        assert!(shown(&bench) == on_the_clipboard(&bench), "redo {step}");
    }
    assert_eq!(bench.editor.job().marks.len(), 3);
}

#[test]
fn frameless_words_show_next_frame() {
    let mut bench = typing(GREETING);
    let before = shown(&bench);
    bench.editor.finish_what_is_in_progress(&mut Vec::new());
    bench.frame(Vec::new());
    assert!(shown(&bench) == on_the_clipboard(&bench) && shown(&bench) != before);
}

#[test]
fn status_quiet_while_block_has_words() {
    let mut bench = super::bench();
    bench.drag(&across(&bench));
    bench.editor.copied(1);
    assert_eq!(bench.editor.reading(), Some(Reading::Copied));
    bench.key(Key::T, Modifiers::NONE);
    bench.click(bench.fit().rect().center());
    bench.type_text(" ");
    assert_eq!(bench.editor.reading(), Some(Reading::Copied), "an empty block changes nothing");
    bench.type_text(GREETING);
    assert_eq!(bench.editor.reading(), None, "the clipboard does not hold these words yet");
    bench.key(Key::Enter, Modifiers::NONE);
    assert_eq!(bench.editor.reading(), Some(Reading::Copying));
    bench.editor.copied(2);
    assert_eq!(bench.editor.reading(), Some(Reading::Copied));
}

#[test]
fn open_block_painted_from_its_mask() {
    let mut bench = with_the_text_tool();
    let picture = bench.fit().rect();
    let Picture::Shown(shown) = &bench.editor.picture else { panic!("a picture on screen") };
    let shown = shown.id();
    let of_the_window: Vec<TextureId> = textured(&mut bench).into_iter().map(|(texture, ..)| texture).collect();
    assert!(of_the_window.contains(&shown));
    bench.click(picture.right_center() - egui::vec2(30.0, 0.0));
    assert_eq!(masks(&mut bench, &of_the_window), [], "an empty block has no ink to show");
    bench.type_text("Hanging off the edge");
    let inked = on_screen(&bench, open(&bench).area().to_rect());
    let [(clip, reach)] = masks(&mut bench, &of_the_window)[..] else { panic!("one mask for the one open block") };
    assert!(picture.expand(HAIR).contains_rect(clip), "{clip:?}");
    assert!(reach.min.distance(inked.min) < HAIR && reach.max.distance(inked.max) < HAIR, "{reach:?} for {inked:?}");
    assert!(open(&bench).frame().right() > bench.editor.image_size()[0] as f32, "the words do run off the picture");
    assert!((reach.right() - picture.right()).abs() < HAIR, "and are shown only up to its edge");
    bench.key(Key::Enter, Modifiers::NONE);
    assert_eq!(masks(&mut bench, &of_the_window), [], "once placed the words are pixels of the picture");
}

#[test]
fn nothing_set_again_without_change() {
    let mut bench = typing(GREETING);
    let set = open(&bench).coverage().as_ptr();
    let [from, middle, to] = across(&bench);
    [from, middle, to].into_iter().for_each(|spot| drop(bench.hover(spot)));
    bench.settle();
    assert_eq!(open(&bench).coverage().as_ptr(), set, "the same mask, never made again");
    bench.key(Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(open(&bench).coverage().as_ptr(), set, "moving the caret sets nothing");
    bench.type_text("!");
    assert_ne!(open(&bench).coverage().as_ptr(), set);
}

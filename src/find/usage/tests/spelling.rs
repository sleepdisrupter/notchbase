use super::super::Usage;
use super::super::disk::{decode, encode};
use super::{MONDAY_NOON, at, key, pick};
use crate::find::kind::Kind;
use crate::find::model::fold_query;
use crate::find::store::frame::Frame;
use crate::find::weights::{PREFIX_BYTES, PREFIX_EXTENDED};

const CHROME: &str = "/Applications/Google Chrome.app";
const RECORD: usize = 32;
const LENGTH_AT: usize = PREFIX_BYTES;
const CLOSE: f32 = 1e-3;

fn stored_as(spellings: &[&[u8]]) -> Frame {
    let usage = Usage::load(None, MONDAY_NOON);
    for nth in 0..spellings.len() {
        let stands_in = format!("memory{nth}");
        (0..2).for_each(|_| usage.picked(&pick(CHROME, &stands_in), Kind::App, &at(MONDAY_NOON, None)));
    }
    let mut frame = usage.read(|model| encode(model, MONDAY_NOON));
    let stored = frame.blocks.iter_mut().find(|(tag, _)| tag == b"PREF").map(|(_, raw)| raw).expect("memories");
    assert_eq!(stored.len(), spellings.len() * RECORD);
    for (record, spelling) in stored.as_chunks_mut::<RECORD>().0.iter_mut().zip(spellings) {
        record[..PREFIX_BYTES].fill(0);
        record[..spelling.len()].copy_from_slice(spelling);
        record[LENGTH_AT] = spelling.len() as u8;
    }
    frame
}

#[test]
fn old_spaced_memory_recalled() {
    let model = decode(&stored_as(&[b"google chrom"])).expect("the file of the last release loads");
    let recalled = |typed: &str| model.remembered(&fold_query(typed), key(CHROME), MONDAY_NOON);
    let texts: Vec<&[u8]> = model.prefixes.rows().iter().map(|row| row.typed.text()).collect();
    assert_eq!(texts, [&b"googlechrom"[..]], "kept under its letters");
    for started in ["g", "goo", "google", "google c", "google chrom", "googlechrom", "Google  Chrom"] {
        assert!((recalled(started) - 1.0).abs() < CLOSE, "{started}: {}", recalled(started));
    }
    for whole in ["google chrome", "googlechrome", "Google-Chrome"] {
        assert!((recalled(whole) - PREFIX_EXTENDED).abs() < CLOSE, "{whole}: {}", recalled(whole));
    }
    assert_eq!(recalled("chrome"), 0.0);
}

#[test]
fn two_spellings_load_as_one() {
    let model = decode(&stored_as(&[b"my file", b"my-file", b"myfile"])).expect("it loads");
    let rows: Vec<(&[u8], f32)> = model.prefixes.rows().iter().map(|row| (row.typed.text(), row.weight)).collect();
    assert_eq!(rows, [(&b"myfile"[..], 6.0)]);
    let again = decode(&encode(&model, MONDAY_NOON)).expect("what is written back loads");
    assert_eq!(again.prefixes.rows(), model.prefixes.rows(), "and stays as it is from then on");
}

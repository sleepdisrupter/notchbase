//! Stepping through a prompt with several questions before the answers are sent.

use crate::assistant::{Question, Reply};
use crate::bridge::Ask;

pub(super) enum Move {
    Tap(usize),
    Back,
    Next,
    Reply(Reply),
}

#[derive(Clone, Default)]
pub(in crate::notch) struct Answering {
    id: u64,
    step: usize,
    picks: Vec<Vec<usize>>,
}

impl Answering {
    pub(super) fn follow(&mut self, ask: &Ask) {
        if self.id != ask.id {
            *self = Self { id: ask.id, ..Self::default() };
        }
    }

    pub(super) fn step(&self) -> usize {
        self.step
    }

    pub(super) fn picked(&self, step: usize) -> &[usize] {
        self.picks.get(step).map_or(&[], Vec::as_slice)
    }

    pub(super) fn take(&mut self, questions: &[Question], step: Move) -> Option<Reply> {
        let question = questions.get(self.step)?;
        self.picks.resize(questions.len(), Vec::new());
        match step {
            Move::Tap(index) if question.multi => {
                let picked = &mut self.picks[self.step];
                match picked.iter().position(|chosen| *chosen == index) {
                    Some(at) => {
                        picked.remove(at);
                    }
                    None => picked.push(index),
                }
                None
            }
            Move::Tap(index) => {
                self.picks[self.step] = vec![index];
                self.next(questions)
            }
            Move::Next => self.next(questions),
            Move::Back => {
                self.step = self.step.saturating_sub(1);
                None
            }
            Move::Reply(reply) => Some(reply),
        }
    }

    fn next(&mut self, questions: &[Question]) -> Option<Reply> {
        if self.picks[self.step].is_empty() {
            return None;
        }
        if self.step + 1 < questions.len() {
            self.step += 1;
            return None;
        }
        let mut picks = self.picks.clone();
        picks.iter_mut().for_each(|picked| picked.sort_unstable());
        Some(Reply::Answers(picks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{Offer, Provider};
    use crate::bridge::Choices;

    fn question(multi: bool) -> Question {
        let offer = |label: &str| Offer { label: label.into(), detail: String::new() };
        Question { header: String::new(), text: "Q".into(), offers: vec![offer("A"), offer("B"), offer("C")], multi }
    }

    fn ask(id: u64) -> Ask {
        Ask {
            id,
            provider: Provider::Claude,
            context: "p".into(),
            title: "Q".into(),
            detail: None,
            choices: Choices::Elsewhere,
            waiting: 0,
        }
    }

    #[test]
    fn one_question_one_tap() {
        let questions = [question(false)];
        let mut answering = Answering::default();
        answering.follow(&ask(1));
        assert_eq!(answering.take(&questions, Move::Tap(2)), Some(Reply::Answers(vec![vec![2]])));
    }

    #[test]
    fn questions_step_back_and_forth() {
        let questions = [question(false), question(true)];
        let mut answering = Answering::default();
        answering.follow(&ask(1));
        assert_eq!(answering.take(&questions, Move::Tap(1)), None);
        assert_eq!(answering.step(), 1);
        assert_eq!(answering.take(&questions, Move::Next), None, "nothing chosen yet");
        answering.take(&questions, Move::Tap(2));
        answering.take(&questions, Move::Tap(0));
        answering.take(&questions, Move::Tap(2));
        answering.take(&questions, Move::Tap(1));
        assert_eq!(answering.picked(1), [0, 1]);
        assert_eq!(answering.take(&questions, Move::Back), None);
        assert_eq!((answering.step(), answering.picked(0)), (0, &[1][..]));
        answering.take(&questions, Move::Tap(0));
        assert_eq!(answering.take(&questions, Move::Next), Some(Reply::Answers(vec![vec![0], vec![0, 1]])));
    }

    #[test]
    fn new_prompt_starts_over() {
        let questions = [question(false), question(false)];
        let mut answering = Answering::default();
        answering.follow(&ask(1));
        answering.take(&questions, Move::Tap(1));
        answering.follow(&ask(1));
        assert_eq!(answering.step(), 1);
        answering.follow(&ask(2));
        assert_eq!((answering.step(), answering.picked(0)), (0, &[][..]));
    }
}

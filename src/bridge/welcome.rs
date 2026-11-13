//! The first-run walkthrough's steps.

use crate::system::permissions::{self, Standing};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WelcomeView {
    pub due: bool,
    pub first_run: bool,
    pub stage: WelcomeStage,
    pub steps: Vec<StepView>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WelcomeStage {
    #[default]
    Hello,
    Step(usize),
    AllSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepView {
    pub access: permissions::Access,
    pub standing: Standing,
    pub asked: bool,
}

impl WelcomeView {
    pub fn step(&self) -> Option<&StepView> {
        match self.stage {
            WelcomeStage::Step(index) => self.steps.get(index),
            _ => None,
        }
    }
}

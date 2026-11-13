//! The live activity: a ring counting down to the next meeting, then through it.

use notchbase_sdk::prelude::*;

use crate::Msg;
use crate::meeting::Meeting;
use crate::upcoming::Phase;

const RING: f32 = 13.0;
const SHORTEST: usize = 18;

pub fn live(phase: Phase, lead_ms: u64) -> Live<Msg> {
    let (ring, until) = match phase {
        Phase::Soon(meeting) => (
            ring(1.0).size(RING).tone(Tone::Blue).counting_down(meeting.start.saturating_sub(lead_ms), meeting.start),
            meeting.start,
        ),
        Phase::Now(meeting) => {
            (ring(1.0).size(RING).tone(Tone::Green).counting_down(meeting.start, meeting.end), meeting.end)
        }
    };
    let meeting = phase.meeting();
    let countdown = || timer(until).remaining().tone(Tone::Secondary);
    let trailing = adaptive(countdown(), row![text(shortened(&meeting.title)).callout(), countdown()].spacing(6.0));
    Live::new(ring, trailing).controls(controls(meeting))
}

fn controls(meeting: &Meeting) -> Element<Msg> {
    let join = meeting.join.clone().map(|link| {
        let service = crate::join::service(&link);
        button("").icon(service.icon).tinted(service.color).on_press(Msg::Join(link)).into()
    });
    let hide = button("").icon(Icon::Xmark).on_press(Msg::Hide(meeting.id.clone())).into();
    row(join.into_iter().chain([hide])).spacing(6.0).into()
}

pub fn shortened(title: &str) -> String {
    match title.char_indices().nth(SHORTEST) {
        Some((at, _)) => format!("{}\u{2026}", title[..at].trim_end()),
        None => title.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_titles_are_cut_short() {
        assert_eq!(shortened("Standup"), "Standup");
        assert_eq!(shortened("Quarterly planning with the whole team"), "Quarterly planning\u{2026}");
    }
}

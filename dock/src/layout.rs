//! Where every slot sits along the dock. Only the icon under the pointer grows; its slot widens by just enough to
//! keep a clear gap to its neighbours, so they step aside a few points and the row never ripples.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// Width at rest, already scaled by how present the slot is.
    pub width: f32,
    /// The icon's size at its current scale, or 0 for the separator.
    pub icon: f32,
    /// The icon's scale: 1 at rest.
    pub scale: f32,
    /// The space opening for an app being dragged in.
    pub gap: bool,
    /// How present the slot is, 0 to 1, as it arrives or leaves: the clear gap around a grown icon shrinks with it.
    pub presence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub left: f32,
    pub width: f32,
    /// 1 at rest, larger while hovered.
    pub scale: f32,
    pub gap: bool,
}

impl Placed {
    pub fn center(&self) -> f32 {
        self.left + self.width * 0.5
    }
    pub fn right(&self) -> f32 {
        self.left + self.width
    }
}

/// Lays the slots out side by side, centred on `center`. A slot whose icon has grown past it widens to the icon plus
/// `gap`, so icons never overlap.
pub fn place(slots: &[Slot], center: f32, gap: f32) -> Vec<Placed> {
    let widths: Vec<f32> =
        slots.iter().map(|s| if s.icon > 0.0 { s.width.max(s.icon + gap * s.presence) } else { s.width }).collect();
    let total: f32 = widths.iter().sum();
    let mut x = center - total * 0.5;
    let mut out = Vec::with_capacity(slots.len());
    for (slot, width) in slots.iter().zip(widths) {
        out.push(Placed { left: x, width, scale: slot.scale, gap: slot.gap });
        x += width;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icons(n: usize) -> Vec<Slot> {
        vec![Slot { width: 50.0, icon: 42.0, scale: 1.0, gap: false, presence: 1.0 }; n]
    }

    #[test]
    fn at_rest_slots_sit_side_by_side_around_the_center() {
        let placed = place(&icons(4), 500.0, 6.0);
        assert_eq!(placed[0].left, 400.0);
        assert_eq!(placed[3].right(), 600.0);
    }

    #[test]
    fn a_hovered_icon_only_nudges_its_neighbours() {
        let mut slots = icons(7);
        let rest = place(&slots, 500.0, 6.0);
        slots[3] = Slot { width: 50.0, icon: 42.0 * 1.22, scale: 1.22, gap: false, presence: 1.0 };
        let hovered = place(&slots, 500.0, 6.0);
        let grew = hovered[3].width - rest[3].width;
        assert!(grew > 0.0 && grew < 8.0, "it widens a few points: {grew}");
        assert!((hovered[3].center() - rest[3].center()).abs() < 0.01, "and stays put");
        for i in [0, 1, 2, 4, 5, 6] {
            assert_eq!(hovered[i].width, rest[i].width, "the others keep their size");
            assert!((hovered[i].center() - rest[i].center()).abs() <= grew * 0.5 + 0.01);
        }
        let icon_left = hovered[3].center() - 42.0 * 1.22 * 0.5;
        let neighbour_right = hovered[2].center() + 21.0;
        assert!(icon_left - neighbour_right >= 6.0, "with a clear gap");
    }
}

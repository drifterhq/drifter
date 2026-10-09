use super::*;

pub(super) struct ChargesPanel {
    pub rect: Rect,
    pub art: Option<TextureBuffer<Tex>>,
    pub lane: worker::Lane,
}

impl State {
    fn charges_card(&self) -> drifter_core::glance::Card {
        let plain = drifter_core::glance::Card::themed(&self.palette);
        drifter_core::glance::Card {
            back_alpha: self.veil(0.62),
            ..plain
        }
    }

    pub(super) fn toggle_charges(&mut self) {
        if self.charges.take().is_some() {
            self.kick_now();
            return;
        }
        let Some(spot) = self.battery_rect() else {
            return;
        };
        self.close_panels();
        self.glance_card = None;
        let card = self.charges_card();
        let x = (spot.max().x - card.width + 8.0).clamp(8.0, (self.screen.x - card.width - 8.0).max(8.0));
        let y = self.tray_band().map_or(spot.max().y, |band| band.max().y) + 8.0;
        self.charges = Some(ChargesPanel {
            rect: Rect::new(Vec2::new(x, y).round(), Vec2::ZERO),
            art: None,
            lane: worker::Lane::default(),
        });
        self.paint_charges();
    }

    pub(super) fn paint_charges(&mut self) {
        let (Some(font), Some(bold)) = (self.card_font.clone(), self.font.clone()) else {
            return;
        };
        let Some(origin) = self.charges.as_ref().map(|panel| panel.rect.origin) else {
            return;
        };
        let card = self.charges_card();
        let frosting = self.frosting();
        self.paint_in_lane_sized(
            "batteries",
            |state| state.charges.as_mut().map(|panel| &mut panel.lane),
            State::paint_charges,
            move || {
                let found = drifter_core::charges::read_all();
                let mut image = drifter_core::charges::paint(&card, &font, &bold, &found);
                worker::frost(&frosting, &mut image, origin, card.back_alpha);
                Some(image)
            },
            |state, art, size| {
                if let Some(panel) = state.charges.as_mut() {
                    panel.rect.size = size;
                    panel.art = Some(art);
                }
            },
        );
    }

    pub(super) fn press_charges(&mut self) -> bool {
        let Some(rect) = self.charges.as_ref().map(|panel| panel.rect) else {
            return false;
        };
        if rect.contains(self.pointer) {
            return true;
        }
        let on_icon = self.battery_rect().is_some_and(|spot| spot.contains(self.pointer));
        self.charges = None;
        self.kick_now();
        on_icon
    }
}

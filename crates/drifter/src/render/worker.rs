use super::*;

#[derive(Default)]
pub(super) struct Lane {
    busy: bool,
    again: bool,
}

impl Lane {
    pub(super) fn claim(&mut self) -> bool {
        match self.busy {
            true => {
                self.again = true;
                false
            }
            false => {
                self.busy = true;
                true
            }
        }
    }

    pub(super) fn release(&mut self) -> bool {
        self.busy = false;
        std::mem::take(&mut self.again)
    }
}

#[derive(Clone)]
pub(super) struct Frosting {
    frost: std::sync::Arc<drifter_core::frost::Frost>,
    laid: drifter_core::wallpaper::Placement,
    size: Vec2,
}

impl Frosting {
    pub(super) fn under(&self, image: &mut drifter_core::Image, origin: Vec2, solid: f64) {
        let dense = drifter_core::paint::density();
        drifter_core::frost::underlay(image, solid, |point| {
            self.frost.behind(&self.laid, self.size, origin + point / dense)
        });
    }
}

impl State {
    pub(super) fn frosting(&self) -> Option<Frosting> {
        let paper = self.wallpaper.as_ref()?;
        let (laid, size) = self.laid_paper()?;
        Some(Frosting {
            frost: paper.frost.clone(),
            laid,
            size,
        })
    }

    pub(super) fn paint_on_worker<P, D>(&self, name: &str, paint: P, done: D)
    where
        P: FnOnce() -> Option<drifter_core::Image> + Send + 'static,
        D: FnOnce(&mut State, Option<TextureBuffer<Tex>>) + Send + 'static,
    {
        let density = self.density;
        let what = name.to_string();
        self.background(name, move || {
            drifter_core::paint::set_density(density);
            let image = paint();
            move |state: &mut State| {
                let art = match image.filter(|_| state.density == density) {
                    Some(image) => match upload(&mut state.renderer, Some(&image), &what) {
                        Ok(art) => art,
                        Err(error) => {
                            log!("drifter: {error}");
                            None
                        }
                    },
                    None => None,
                };
                done(state, art);
            }
        });
    }
}

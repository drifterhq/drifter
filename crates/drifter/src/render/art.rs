use super::*;

pub(super) fn dock_items(
    renderer: &mut Painter,
    config: &Config,
    side: u32,
    spare: Option<&drifter_core::Image>,
) -> Result<Pinned, String> {
    let dock = &config.dock;
    let mut wanted: Vec<drifter_core::Launch> = crate::apps::defaults(dock, &config.apps)
        .into_iter()
        .filter(|item| {
            !dock
                .pinned
                .iter()
                .any(|held| held == item || (held.app().is_some() && held.app() == item.app()))
        })
        .collect();
    let button = drifter_core::launchpad::button();
    if dock.apps && !dock.pinned.contains(&button) {
        wanted.insert(0, button);
    }
    wanted.extend(
        dock.pinned
            .iter()
            .filter(|item| dock.apps || !drifter_core::launchpad::is_button(item))
            .cloned(),
    );
    let mut pinned = Vec::new();
    for item in wanted {
        let path = item
            .icon()
            .map(|path| config.beside(path))
            .or_else(|| item.app().and_then(crate::apps::find).and_then(|e| e.icon))
            .or_else(|| crate::apps::icon_file(&item.name()));
        let marked = item
            .command()
            .and_then(drifter_core::place::from_dock)
            .is_some()
            && item.icon().is_none();
        let path = path.filter(|_| !marked);
        let art = path
            .and_then(|path| match drifter_core::image::load_sized(&path, side) {
                Ok(image) => Some(image),
                Err(error) => {
                    eprintln!("drifter: no dock icon for {}: {error}", item.name());
                    None
                }
            })
            .map(|image| image.scaled(side, side))
            .or_else(|| marked.then(|| drifter_core::place::icon(side)))
            .or_else(|| {
                drifter_core::launchpad::is_button(&item)
                    .then(|| drifter_core::launchpad::paint_icon(side))
            })
            .or_else(|| spare.cloned());
        let buffer = match art {
            Some(image) => upload(renderer, Some(&image), "dock icon")?,
            None => None,
        };
        pinned.push((item, buffer));
    }
    Ok(pinned)
}

pub(super) fn asset_paths(config: &Config) -> [Option<PathBuf>; 4] {
    [
        config.cursor_path(),
        config.font_path(),
        config.window_dir(),
        config.dock_dir(),
    ]
}

pub(super) fn load_arrow(
    renderer: &mut Painter,
    config: &Config,
) -> Result<(TextureBuffer<Tex>, Vec2, Vec2), String> {
    let supplied = config
        .cursor_path()
        .and_then(|path| match drifter_core::image::load(&path) {
            Ok(image) => Some(image),
            Err(error) => {
                eprintln!("drifter: no cursor: {error}");
                None
            }
        });
    let look = &config.appearance;
    let (image, hotspot) = match supplied {
        Some(image) => (image, Vec2::ZERO),
        None => crate::cursors::load(
            &["default", "left_ptr"],
            look.cursor_theme.as_deref(),
            look.cursor_size,
        )
        .unwrap_or_else(|| {
            let drawn = Cursor::default();
            (drawn.paint(), drawn.hotspot())
        }),
    };
    let size = image.size();
    let arrow = upload(renderer, Some(&image), "cursor")?
        .ok_or_else(|| "cannot build the cursor".to_string())?;
    Ok((arrow, size, hotspot))
}

pub(super) fn load_skin(
    renderer: &mut Painter,
    config: &Config,
    palette: &drifter_core::theme::Palette,
) -> (SkinSet, Chrome) {
    let mut skin = std::collections::HashMap::new();
    let dir = config.window_dir().map(|dir| {
        let light = dir.join("light");
        match *palette == drifter_core::theme::LIGHT && light.is_dir() {
            true => light,
            false => dir,
        }
    });
    let drawn = Chrome::themed(palette);
    for name in crate::shell::skin_names() {
        let from_file = dir.as_ref().and_then(|dir| {
            drifter_core::image::load(&dir.join(&name))
                .inspect_err(|error| eprintln!("drifter: no window art: {error}"))
                .ok()
        });
        let Some(image) = from_file.or_else(|| drifter_core::window::painted(&name, &drawn)) else {
            continue;
        };
        let image = tile_skin(&name, image);
        let size = image.size();
        match upload(renderer, Some(&image), &name) {
            Ok(Some(buffer)) => {
                skin.insert(name, (buffer, size));
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }
    let chrome = Chrome::themed(palette);
    let chrome = match skin.get("title-mid.png").zip(skin.get("edge-left.png")) {
        Some(((_, title), (_, edge))) => Chrome {
            title: title.y,
            edge: edge.x,
            ..chrome
        },
        None => chrome,
    };
    (skin, chrome)
}

pub(super) fn load_dock_art(
    renderer: &mut Painter,
    config: &Config,
    side: u32,
    palette: &drifter_core::theme::Palette,
) -> DockArt {
    let look = Dock::themed(palette);
    let art = config.dock_dir();
    let from_art = |name: &str| -> Option<drifter_core::Image> {
        let dir = art.as_ref()?;
        match drifter_core::image::load(&dir.join(format!("{name}.png"))) {
            Ok(image) => Some(image.scaled(side, side)),
            Err(error) => {
                eprintln!("drifter: no dock art for {name}: {error}");
                None
            }
        }
    };
    let trash_image =
        from_art("trash").unwrap_or_else(|| drifter_core::dock::paint_trash(side, look.rule_ink()));
    let cog_image = from_art("settings")
        .unwrap_or_else(|| drifter_core::dock::paint_cog(side, look.rule_ink()));
    let mut built =
        |image: &drifter_core::Image, what: &str| match upload(renderer, Some(image), what) {
            Ok(buffer) => buffer,
            Err(error) => {
                eprintln!("drifter: {error}");
                None
            }
        };
    let trash = built(&trash_image, "trash");
    let cog = built(&cog_image, "settings");
    (trash, cog, from_art("app"))
}

pub(super) fn tile_skin(name: &str, image: Image) -> Image {
    let piece = ChromePiece::ALL.into_iter().find(|piece| {
        tiles(*piece) && (piece.file_name(true) == name || piece.file_name(false) == name)
    });
    match piece {
        Some(ChromePiece::TitleMid | ChromePiece::EdgeBottom) => {
            image.tiled(TILE_SPAN, image.height)
        }
        Some(_) => image.tiled(image.width, TILE_SPAN),
        None => image,
    }
}

pub(super) fn tiles(piece: ChromePiece) -> bool {
    matches!(
        piece,
        ChromePiece::TitleMid
            | ChromePiece::EdgeLeft
            | ChromePiece::EdgeRight
            | ChromePiece::EdgeBottom
    )
}

pub(super) fn paint_drops(
    renderer: &mut Painter,
    width: f64,
    shapes: &[Panel; 2],
    palette: &drifter_core::theme::Palette,
) -> Vec<TextureBuffer<Tex>> {
    Edge::ALL
        .into_iter()
        .filter_map(|edge| {
            let image = drawer::drop_shadow(
                edge,
                width.max(1.0) as u32,
                &shapes[edge.index()],
                f64::from(palette.drop) / 255.0,
            );
            upload(renderer, Some(&image), "drawer shadow").ok().flatten()
        })
        .collect()
}

pub(super) fn paint_panels(
    renderer: &mut Painter,
    width: f64,
    shapes: &[Panel; 2],
    palette: &drifter_core::theme::Palette,
) -> Result<[TextureBuffer<Tex>; 2], String> {
    let mut built = Vec::new();
    for edge in Edge::ALL {
        let what = match edge {
            Edge::Top => "top drawer",
            Edge::Bottom => "bottom drawer",
        };
        let mut image = drawer::paint(
            edge,
            width.max(1.0) as u32,
            &shapes[edge.index()],
            palette.bar,
            PANEL_ALPHA,
        );
        drawer::rim(
            &mut image,
            edge,
            &shapes[edge.index()],
            palette.rim,
            f64::from(palette.rim_alpha) / 255.0,
        );
        let buffer = upload(renderer, Some(&image), what)?
            .ok_or_else(|| format!("cannot build the {what}"))?;
        built.push(buffer);
    }
    built
        .try_into()
        .map_err(|_| "the drawer textures went missing".to_string())
}

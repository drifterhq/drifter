use crate::geom::Vec2;
use crate::paint::same_at_two;
use crate::text::Font;

fn font() -> Font {
    Font::find(None, 13.0).expect("this machine has no usable font")
}

#[test]
fn icons_at_density_two_match_density_one() {
    same_at_two("cog", || crate::dock::paint_cog(32, [240, 240, 240]));
    same_at_two("dot", || crate::dock::paint_dot(6, [240, 240, 240]));
    same_at_two("trash", || crate::dock::paint_trash(32, [240, 240, 240]));
    same_at_two("full trash", || crate::dock::paint_trash_full(32, [240, 240, 240]));
    same_at_two("glow", || crate::corner::paint_glow(48, [255, 214, 160]));
    same_at_two("power", || crate::tray::paint_power(20, [240, 240, 240]));
    same_at_two("place", || crate::place::icon(32));
}

#[test]
fn system_info_at_density_two_matches_density_one() {
    let font = font();
    same_at_two("stats", || crate::stats::paint(&font, &[("CPU", 12), ("RAM", 48)], [240, 240, 240]));
}

#[test]
fn the_screenshot_tool_at_density_two_matches_density_one() {
    let font = font();
    let bar = crate::grab::Bar::themed(&crate::theme::DARK, [10, 122, 255]);
    let tool = crate::grab::Tool::new(crate::grab::Take::Still, crate::grab::Pick::Portion);
    same_at_two("bar", || bar.paint(tool, None, &font));
    same_at_two("tag", || crate::grab::paint_tag("640 × 480", &font));
    same_at_two("stop", || crate::grab::paint_stop(75, &font));
    same_at_two("cross", || crate::grab::paint_cross(21));
    let shot = crate::paint::Canvas::new(400, 300).image;
    same_at_two("thumb", || {
        let size = crate::grab::thumb_size(Vec2::new(400.0, 300.0), 160.0);
        assert!(size.x > 0.0);
        crate::grab::paint_thumb(&shot, 160.0)
    });
}

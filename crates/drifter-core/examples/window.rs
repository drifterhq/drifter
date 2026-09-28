use std::path::PathBuf;

use drifter_core::window::{Button, Chrome, Look, Piece, paint, paint_button};

fn out_dir() -> PathBuf {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "img", "window"]
        .iter()
        .collect();
    std::fs::create_dir_all(&dir).expect("cannot make img/window");
    dir
}

fn write(dir: &std::path::Path, name: &str, image: &drifter_core::Image) {
    let mut image = image.clone();
    image.straighten();
    let image = &image;
    let path = dir.join(name);
    let file = std::fs::File::create(&path).expect("cannot write the piece");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("cannot write the png header")
        .write_image_data(&image.rgba)
        .expect("cannot write the png pixels");
    println!("{name}  {} x {}", image.width, image.height);
}

fn main() {
    let chrome = Chrome::default();
    let dir = out_dir();

    for piece in Piece::ALL {
        for active in [true, false] {
            write(
                &dir,
                &piece.file_name(active),
                &paint(piece, &chrome, active),
            );
        }
    }

    for button in Button::ALL {
        for look in Look::ALL {
            write(
                &dir,
                &button.file_name(look),
                &paint_button(button, look, &chrome),
            );
        }
    }

    println!(
        "wrote {} pieces to {}",
        Piece::ALL.len() * 2 + Button::ALL.len() * Look::ALL.len(),
        dir.display()
    );
}

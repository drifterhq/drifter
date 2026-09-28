use std::path::PathBuf;

use drifter_core::Cursor;

fn main() {
    let mut image = Cursor::default().paint();
    image.straighten();
    let out: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "img", "cursor.png"]
        .iter()
        .collect();

    let file = std::fs::File::create(&out).expect("cannot write the cursor");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("cannot write the png header")
        .write_image_data(&image.rgba)
        .expect("cannot write the png pixels");

    println!(
        "wrote {} at {} x {}",
        out.display(),
        image.width,
        image.height
    );
}

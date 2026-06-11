//! Verifies two WebP files decode to bit-identical pixels:
//! `cargo run --example webpcmp -- a.webp b.webp`

fn decode(path: &str) -> (u32, u32, Vec<u8>) {
    let data = std::fs::read(path).unwrap();
    let image = webp::Decoder::new(&data).decode().expect("decode failed");
    (image.width(), image.height(), image.to_vec())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (aw, ah, apx) = decode(&args[1]);
    let (bw, bh, bpx) = decode(&args[2]);
    println!("a: {aw}x{ah}, b: {bw}x{bh}");
    if (aw, ah) != (bw, bh) {
        println!("DIMENSIONS DIFFER");
        std::process::exit(1);
    }
    let diff = apx.iter().zip(&bpx).filter(|(x, y)| x != y).count();
    if diff == 0 && apx.len() == bpx.len() {
        println!("PIXELS IDENTICAL");
    } else {
        println!(
            "PIXELS DIFFER: {diff} bytes (len {} vs {})",
            apx.len(),
            bpx.len()
        );
        std::process::exit(1);
    }
}

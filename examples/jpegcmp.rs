//! Decodes two JPEGs to raw RGB with libjpeg (no color management) and
//! reports whether the pixel data is bit-identical. Verification helper:
//! `cargo run --example jpegcmp -- a.jpg b.jpg`

use mozjpeg_sys::*;
use std::mem;
use std::os::raw::c_ulong;

unsafe fn decode(data: &[u8]) -> (u32, u32, Vec<u8>) {
    unsafe {
        let mut err: jpeg_error_mgr = mem::zeroed();
        jpeg_std_error(&mut err);
        let mut cinfo: jpeg_decompress_struct = mem::zeroed();
        cinfo.common.err = &mut err;
        jpeg_create_decompress(&mut cinfo);
        jpeg_mem_src(&mut cinfo, data.as_ptr(), data.len() as c_ulong);
        jpeg_read_header(&mut cinfo, 1);
        cinfo.out_color_space = J_COLOR_SPACE::JCS_RGB;
        jpeg_start_decompress(&mut cinfo);
        let width = cinfo.output_width;
        let height = cinfo.output_height;
        let row_len = width as usize * 3;
        let mut pixels = vec![0u8; row_len * height as usize];
        while cinfo.output_scanline < height {
            let offset = cinfo.output_scanline as usize * row_len;
            let mut row_ptr = pixels[offset..].as_mut_ptr();
            jpeg_read_scanlines(&mut cinfo, &mut row_ptr, 1);
        }
        jpeg_finish_decompress(&mut cinfo);
        jpeg_destroy_decompress(&mut cinfo);
        (width, height, pixels)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = std::fs::read(&args[1]).unwrap();
    let b = std::fs::read(&args[2]).unwrap();
    let (aw, ah, apx) = unsafe { decode(&a) };
    let (bw, bh, bpx) = unsafe { decode(&b) };
    println!("a: {aw}x{ah}, b: {bw}x{bh}");
    if (aw, ah) != (bw, bh) {
        println!("DIMENSIONS DIFFER");
        std::process::exit(1);
    }
    let diff = apx.iter().zip(&bpx).filter(|(x, y)| x != y).count();
    if diff == 0 {
        println!("PIXELS IDENTICAL");
    } else {
        println!("PIXELS DIFFER: {diff} of {} bytes", apx.len());
        std::process::exit(1);
    }
}

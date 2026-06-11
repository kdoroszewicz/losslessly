//! Lossless JPEG optimization via mozjpeg's transcoding path (the same
//! mechanism as `jpegtran -optimize`): DCT coefficients are copied verbatim,
//! only the entropy coding is rebuilt, so pixel data is bit-identical.

use anyhow::{anyhow, Result};
use mozjpeg_sys::*;
use std::mem;
use std::os::raw::c_ulong;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

/// libjpeg errors surface as controlled panics (see `error_exit_panic`) that
/// are caught in `transcode`. Silence the default panic hook for those so the
/// user only sees the clean error report; anything else still prints normally.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if !msg.starts_with("libjpeg:") && !msg.starts_with("corrupt JPEG") {
            previous(info);
        }
    }));
}

/// Losslessly optimize a JPEG. Tries both baseline and progressive entropy
/// coding (both with optimized Huffman tables) and returns the smaller result.
pub fn optimize(data: &[u8], strip: bool) -> Result<Vec<u8>> {
    let baseline = transcode(data, false, strip)?;
    let progressive = transcode(data, true, strip)?;
    Ok(if progressive.len() < baseline.len() {
        progressive
    } else {
        baseline
    })
}

fn transcode(data: &[u8], progressive: bool, strip: bool) -> Result<Vec<u8>> {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        transcode_unchecked(data, progressive, strip)
    }))
    .map_err(|panic| {
        let msg = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown libjpeg error".to_string());
        anyhow!(msg)
    })
}

/// Replacement for libjpeg's default error handler, which would call `exit()`.
/// Panics instead; the panic unwinds through the `C-unwind` FFI boundary
/// (mozjpeg-sys is built with unwinding support) and is caught in `transcode`.
unsafe extern "C-unwind" fn error_exit_panic(cinfo: &mut jpeg_common_struct) {
    let buf = [0u8; 80];
    if let Some(format) = unsafe { (*cinfo.err).format_message } {
        // writes a NUL-terminated C string into buf (the &[u8; 80] binding is
        // a bindgen artifact; the callee writes through it)
        unsafe { format(cinfo, &buf) };
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    panic!("libjpeg: {}", String::from_utf8_lossy(&buf[..len]));
}

struct DecompressGuard(jpeg_decompress_struct);
impl Drop for DecompressGuard {
    fn drop(&mut self) {
        unsafe { jpeg_destroy_decompress(&mut self.0) }
    }
}

struct CompressGuard(jpeg_compress_struct);
impl Drop for CompressGuard {
    fn drop(&mut self) {
        unsafe { jpeg_destroy_compress(&mut self.0) }
    }
}

unsafe fn transcode_unchecked(data: &[u8], progressive: bool, strip: bool) -> Vec<u8> {
    unsafe {
    let copy_option = if strip {
        JCOPY_OPTION_JCOPYOPT_NONE
    } else {
        JCOPY_OPTION_JCOPYOPT_ALL
    };

    let mut src_err: jpeg_error_mgr = mem::zeroed();
    jpeg_std_error(&mut src_err);
    src_err.error_exit = Some(error_exit_panic);

    let mut src = DecompressGuard(mem::zeroed());
    src.0.common.err = &mut src_err;
    jpeg_create_decompress(&mut src.0);

    jpeg_mem_src(&mut src.0, data.as_ptr(), data.len() as c_ulong);
    jcopy_markers_setup(&mut src.0, copy_option);
    jpeg_read_header(&mut src.0, 1);
    let coefficients = jpeg_read_coefficients(&mut src.0);

    let mut dst_err: jpeg_error_mgr = mem::zeroed();
    jpeg_std_error(&mut dst_err);
    dst_err.error_exit = Some(error_exit_panic);

    let mut dst = CompressGuard(mem::zeroed());
    dst.0.common.err = &mut dst_err;
    jpeg_create_compress(&mut dst.0);

    jpeg_copy_critical_parameters(&src.0, &mut dst.0);
    dst.0.optimize_coding = 1;
    if progressive {
        jpeg_simple_progression(&mut dst.0);
    }

    let mut out_buf: *mut u8 = ptr::null_mut();
    let mut out_size: c_ulong = 0;
    jpeg_mem_dest(&mut dst.0, &mut out_buf, &mut out_size);

    jpeg_write_coefficients(&mut dst.0, coefficients);
    jcopy_markers_execute(&mut src.0, &mut dst.0, copy_option);
    jpeg_finish_compress(&mut dst.0);
    jpeg_finish_decompress(&mut src.0);

    // libjpeg recovers from corrupt data (e.g. truncated files) by padding it
    // and only emitting a warning. Rewriting such a file would bake the
    // padding in, so refuse instead of silently "optimizing" garbage.
    if src_err.num_warnings > 0 {
        panic!("corrupt JPEG data (decoder reported warnings), refusing to rewrite");
    }

    let result = std::slice::from_raw_parts(out_buf, out_size as usize).to_vec();
    libc::free(out_buf.cast());
    result
    }
}

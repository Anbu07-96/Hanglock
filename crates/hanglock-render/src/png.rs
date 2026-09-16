//! A PNG writer with no dependencies, for `--dump-scene`.
//!
//! Deflate in *stored* blocks only: no compressor, still a valid PNG, roughly 1.5× the size of an
//! optimised one, and nobody is downloading it. What matters is that the renderer's output can be
//! looked at on a machine that has no image library — including the maintainer's, which is how the
//! face was reviewed at all.

use crate::canvas::Canvas;

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[tag.as_slice(), data].concat()).to_be_bytes());
}

fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01]; // zlib header: deflate, 32 KiB window
    let mut i = 0;
    loop {
        let n = (data.len() - i).min(0xFFFF);
        let last = if i + n >= data.len() { 1 } else { 0 };
        out.push(last);
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[i..i + n]);
        i += n;
        if i >= data.len() {
            break;
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// RGBA (straight, not premultiplied) for review; the canvas holds premultiplied BGRA, which is
/// what the window needs and not what an image viewer expects.
#[must_use]
pub fn to_rgba(cv: &Canvas) -> Vec<u8> {
    let n = (cv.w as usize) * (cv.h as usize);
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let a = cv.px[i * 4 + 3];
        let unpremul = |c: u8| -> u8 {
            if a == 0 {
                0
            } else {
                ((u32::from(c) * 255) / u32::from(a)).min(255) as u8
            }
        };
        out[i * 4] = unpremul(cv.px[i * 4 + 2]);
        out[i * 4 + 1] = unpremul(cv.px[i * 4 + 1]);
        out[i * 4 + 2] = unpremul(cv.px[i * 4]);
        out[i * 4 + 3] = a;
    }
    out
}

/// Encode as RGBA8 PNG.
#[must_use]
pub fn encode(cv: &Canvas) -> Vec<u8> {
    let rgba = to_rgba(cv);
    let w = cv.w as usize;
    let h = cv.h as usize;
    let mut raw = Vec::with_capacity(h * (w * 4 + 1));
    for y in 0..h {
        raw.push(0); // filter: None
        let s = y * w * 4;
        raw.extend_from_slice(&rgba[s..s + w * 4]);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(cv.w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(cv.h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, truecolour+alpha
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &deflate_stored(&raw));
    chunk(&mut out, b"IEND", b"");
    out
}

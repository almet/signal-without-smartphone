//! Read a device-linking QR code from the clipboard image and decode it.
//! This uses different strategies, because the QR code detection might not work
//! depending how it was captured.
//! (I was naively thinking that people would use their WM screenshot abilities, but
//! turns out sometimes they like to be creative!)

use arboard::Clipboard;
use image::DynamicImage;
use rxing::{
    common::HybridBinarizer, BinaryBitmap, BufferedImageLuminanceSource, DecodeHintType,
    DecodeHintValue, MultiFormatReader, Reader,
};

/// Try to decode a QR code from a grayscale image using rxing.
fn try_decode_gray(gray: &image::GrayImage) -> Option<String> {
    let lum_source = BufferedImageLuminanceSource::new(DynamicImage::ImageLuma8(gray.clone()));
    let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(lum_source));
    let mut hints = std::collections::HashMap::new();
    hints.insert(DecodeHintType::TRY_HARDER, DecodeHintValue::TryHarder(true));
    MultiFormatReader::default()
        .decode_with_hints(&mut bitmap, &hints)
        .ok()
        .map(|r| r.getText().to_string())
}

fn invert(gray: &image::GrayImage) -> image::GrayImage {
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        p.0[0] = 255 - p.0[0];
    }
    out
}

fn apply_threshold(gray: &image::GrayImage, threshold: u8) -> image::GrayImage {
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        p.0[0] = if p.0[0] > threshold { 255 } else { 0 };
    }
    out
}

fn adjust_brightness_contrast(
    gray: &image::GrayImage,
    brightness: i32,
    contrast: f32,
) -> image::GrayImage {
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        let v = ((p.0[0] as i32 - 128) as f32 * contrast) as i32 + 128 + brightness;
        p.0[0] = v.clamp(0, 255) as u8;
    }
    out
}

fn upscale(gray: &image::GrayImage, scale: u32) -> image::GrayImage {
    image::imageops::resize(
        gray,
        gray.width() * scale,
        gray.height() * scale,
        image::imageops::FilterType::Nearest,
    )
}

fn decode_qr_from_image(img: &DynamicImage) -> Result<String, String> {
    let gray = img.to_luma8();

    let try_it = |g: image::GrayImage| -> Option<String> { try_decode_gray(&g) };

    if let Some(s) = try_it(gray.clone()) {
        return Ok(s);
    }
    if let Some(s) = try_it(invert(&gray)) {
        return Ok(s);
    }

    for t in [100u8, 128, 150, 180, 200] {
        if let Some(s) = try_it(apply_threshold(&gray, t)) {
            return Ok(s);
        }
    }
    for (b, c) in [(20, 1.5_f32), (-20, 1.5), (0, 2.0)] {
        if let Some(s) = try_it(adjust_brightness_contrast(&gray, b, c)) {
            return Ok(s);
        }
    }
    for scale in [2u32, 3, 4] {
        if let Some(s) = try_it(upscale(&gray, scale)) {
            return Ok(s);
        }
    }
    for (scale, t) in [(2u32, 128u8), (3, 128), (2, 150)] {
        if let Some(s) = try_it(apply_threshold(&upscale(&gray, scale), t)) {
            return Ok(s);
        }
    }
    for (scale, b, c) in [(2u32, 0, 2.0_f32), (3, 0, 2.0)] {
        if let Some(s) = try_it(adjust_brightness_contrast(&upscale(&gray, scale), b, c)) {
            return Ok(s);
        }
    }
    if gray.width() > 800 || gray.height() > 800 {
        let scale = 800.0 / gray.width().max(gray.height()) as f32;
        let down = image::imageops::resize(
            &gray,
            (gray.width() as f32 * scale) as u32,
            (gray.height() as f32 * scale) as u32,
            image::imageops::FilterType::Lanczos3,
        );
        if let Some(s) = try_it(down) {
            return Ok(s);
        }
    }
    let blurred = image::imageops::blur(&gray, 1.0);
    if let Some(s) = try_it(apply_threshold(&blurred, 128)) {
        return Ok(s);
    }

    Err(
        "Could not decode QR code. The image may be damaged, too blurry, or partially obscured."
            .into(),
    )
}

/// Get image from clipboard and decode QR code.
pub(crate) fn paste_and_decode_qr() -> Result<String, String> {
    let mut clipboard =
        Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;

    let img_data = clipboard
        .get_image()
        .map_err(|e| format!("No image in clipboard: {}. Try taking a screenshot with Cmd+Shift+4 and selecting the QR code area.", e))?;

    let width = img_data.width as u32;
    let height = img_data.height as u32;
    let bytes = img_data.bytes.to_vec();

    let dynamic_img = match bytes.len() / (width as usize * height as usize) {
        4 => image::RgbaImage::from_raw(width, height, bytes)
            .map(DynamicImage::ImageRgba8)
            .ok_or_else(|| format!("Failed to create RGBA image ({}x{})", width, height))?,
        3 => image::RgbImage::from_raw(width, height, bytes)
            .map(DynamicImage::ImageRgb8)
            .ok_or_else(|| format!("Failed to create RGB image ({}x{})", width, height))?,
        n => return Err(format!("Unexpected pixel format: {} bytes per pixel.", n)),
    };

    decode_qr_from_image(&dynamic_img)
}

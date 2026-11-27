//! Bild für ein eigenes Server-Emoji vorbereiten: auf höchstens 128 × 128 verkleinern, unter die
//! Upload-Grenze bringen, Metadaten entfernen. GIFs bleiben animiert.
//!
//! Grenzen wie Mattermost (`server/channels/app/emoji.go`): 512 KiB je Anfrage, 70 GIF-Bilder; größere
//! Bilder verkleinert der Server selbst auf 128 × 128, lehnt aber Originale über 1028 px ab.

use std::io::Cursor;
use std::path::Path;

use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::imageops::FilterType;
use image::{
    AnimationDecoder, DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, Limits,
    RgbaImage,
};

use crate::backend::Avatar;
use crate::error::{Error, Result};

/// Seitenlänge, auf die Mattermost Emojis verkleinert.
pub const EMOJI_SIZE: u32 = 128;
/// Mattermost begrenzt die ganze Anfrage auf 512 KiB; Rest für Multipart-Rahmen und JSON-Feld.
pub const MAX_UPLOAD_BYTES: usize = (512 - 4) << 10;
/// Höchstzahl an Einzelbildern eines GIFs (Mattermost `MaxEmojiGIFFrames`).
pub const MAX_GIF_FRAMES: usize = 70;
/// Größte gewählte Datei (Fotos vom Telefon passen).
pub const MAX_INPUT_BYTES: usize = 20 << 20;
/// Schutz vor Bildbomben: Seitenlänge vor dem Dekodieren prüfen.
const MAX_INPUT_SIDE: u32 = 10_000;

/// Vorbereitetes Emoji-Bild mit Angaben für die Vorschau.
#[derive(Debug, Clone)]
pub struct PreparedEmoji {
    pub image: Avatar,
    pub width: u32,
    pub height: u32,
    /// 1 bei Standbildern.
    pub frames: usize,
    /// Kleiner gerechnet als das Original.
    pub resized: bool,
}

/// Wie [`prepare`]; liest die Datei nur, wenn sie höchstens [`MAX_INPUT_BYTES`] groß ist.
pub fn prepare_file(path: &Path) -> Result<PreparedEmoji> {
    if std::fs::metadata(path)?.len() > MAX_INPUT_BYTES as u64 {
        return Err(input_too_big());
    }
    prepare(&std::fs::read(path)?)
}

/// PNG, JPEG, WebP (Standbild) oder GIF vorbereiten. Fehler sind für Nutzer formuliert.
pub fn prepare(raw: &[u8]) -> Result<PreparedEmoji> {
    if raw.len() > MAX_INPUT_BYTES {
        return Err(input_too_big());
    }
    let prepared = match image::guess_format(raw) {
        Ok(ImageFormat::Gif) => prepare_gif(raw)?,
        Ok(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP) => prepare_still(raw)?,
        _ => return Err(Error::other("Nur PNG, JPEG, GIF oder WebP")),
    };
    if prepared.image.data.len() > MAX_UPLOAD_BYTES {
        return Err(Error::other(format!(
            "Auch verkleinert zu groß ({} KiB, höchstens {} KiB); weniger Bilder oder Farben verwenden",
            prepared.image.data.len() >> 10,
            MAX_UPLOAD_BYTES >> 10
        )));
    }
    Ok(prepared)
}

fn input_too_big() -> Error {
    Error::other(format!(
        "Datei zu groß (höchstens {} MiB)",
        MAX_INPUT_BYTES >> 20
    ))
}

fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_INPUT_SIDE);
    limits.max_image_height = Some(MAX_INPUT_SIDE);
    limits
}

fn unreadable(e: image::ImageError) -> Error {
    Error::other(format!("Bild nicht lesbar: {e}"))
}

/// Größe innerhalb von `EMOJI_SIZE` × `EMOJI_SIZE` mit gleichem Seitenverhältnis; nie vergrößern.
fn fit(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= EMOJI_SIZE {
        return (width, height);
    }
    let scale =
        |side: u32| ((u64::from(side) * u64::from(EMOJI_SIZE)) / u64::from(longest)).max(1) as u32;
    (scale(width), scale(height))
}

/// Standbild immer neu als PNG kodieren: entfernt EXIF (z. B. Aufnahmeort) und dreht nach EXIF-Ausrichtung.
fn prepare_still(raw: &[u8]) -> Result<PreparedEmoji> {
    let mut reader = ImageReader::new(Cursor::new(raw))
        .with_guessed_format()
        .map_err(|e| Error::other(format!("Bild nicht lesbar: {e}")))?;
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(unreadable)?;
    let orientation = decoder.orientation().map_err(unreadable)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(unreadable)?;
    img.apply_orientation(orientation);

    let (width, height) = fit(img.width(), img.height());
    let resized = (width, height) != (img.width(), img.height());
    if resized {
        img = img.resize_exact(width, height, FilterType::Lanczos3);
    }
    let mut data = Vec::new();
    img.write_to(&mut Cursor::new(&mut data), ImageFormat::Png)
        .map_err(|e| Error::other(format!("PNG schreiben: {e}")))?;
    Ok(PreparedEmoji {
        image: Avatar {
            data,
            mime: "image/png".into(),
        },
        width,
        height,
        frames: 1,
        resized,
    })
}

/// GIF, das schon passt, unverändert lassen (neues Kodieren reduziert Farben); sonst jedes Bild verkleinern.
fn prepare_gif(raw: &[u8]) -> Result<PreparedEmoji> {
    let mut decoder = GifDecoder::new(Cursor::new(raw)).map_err(unreadable)?;
    decoder.set_limits(limits()).map_err(unreadable)?;
    let (source_width, source_height) = decoder.dimensions();
    let (width, height) = fit(source_width, source_height);
    let resized = (width, height) != (source_width, source_height);

    // Bild für Bild: nur die verkleinerten Bilder bleiben im Speicher.
    let mut frame_count = 0;
    let mut small_frames = Vec::new();
    for frame in decoder.into_frames() {
        frame_count += 1;
        if frame_count > MAX_GIF_FRAMES {
            return Err(Error::other(format!(
                "GIF hat mehr als {MAX_GIF_FRAMES} Bilder"
            )));
        }
        let frame = frame.map_err(unreadable)?;
        if resized {
            let small: RgbaImage =
                image::imageops::resize(frame.buffer(), width, height, FilterType::Lanczos3);
            small_frames.push(Frame::from_parts(small, 0, 0, frame.delay()));
        }
    }
    if frame_count == 0 {
        return Err(Error::other("GIF enthält kein Bild"));
    }

    let data = if resized {
        encode_gif(small_frames)?
    } else {
        raw.to_vec()
    };
    Ok(PreparedEmoji {
        image: Avatar {
            data,
            mime: "image/gif".into(),
        },
        width,
        height,
        frames: frame_count,
        resized,
    })
}

fn encode_gif(frames: Vec<Frame>) -> Result<Vec<u8>> {
    let failed = |e: image::ImageError| Error::other(format!("GIF schreiben: {e}"));
    let mut data = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut data);
        encoder.set_repeat(Repeat::Infinite).map_err(failed)?;
        encoder.encode_frames(frames).map_err(failed)?;
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use image::{Delay, GenericImageView, Rgba};

    fn gif_duration(data: &[u8]) -> Duration {
        GifDecoder::new(Cursor::new(data))
            .unwrap()
            .into_frames()
            .map(|f| Duration::from(f.unwrap().delay()))
            .sum()
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(width, height, Rgba([200, 30, 30, 255]));
        let mut data = Vec::new();
        DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut data), ImageFormat::Png)
            .unwrap();
        data
    }

    fn gif(side: u32, frame_count: usize) -> Vec<u8> {
        let frames = (0..frame_count).map(|index| {
            let shade = (index * 255 / frame_count.max(1)) as u8;
            Frame::from_parts(
                RgbaImage::from_pixel(side, side, Rgba([shade, 0, 255 - shade, 255])),
                0,
                0,
                Delay::from_saturating_duration(Duration::from_millis(100)),
            )
        });
        encode_gif(frames.collect()).unwrap()
    }

    fn decoded_size(data: &[u8]) -> (u32, u32) {
        image::load_from_memory(data).unwrap().dimensions()
    }

    #[test]
    fn large_still_image_shrinks_to_fit_keeping_aspect() {
        let prepared = prepare(&png(600, 300)).unwrap();
        assert_eq!((prepared.width, prepared.height), (128, 64));
        assert!(prepared.resized);
        assert_eq!(prepared.image.mime, "image/png");
        assert_eq!(decoded_size(&prepared.image.data), (128, 64));
    }

    #[test]
    fn small_still_image_keeps_its_size() {
        let prepared = prepare(&png(40, 20)).unwrap();
        assert_eq!((prepared.width, prepared.height), (40, 20));
        assert!(!prepared.resized);
    }

    #[test]
    fn large_gif_shrinks_and_stays_animated() {
        let prepared = prepare(&gif(256, 3)).unwrap();
        assert_eq!((prepared.width, prepared.height), (128, 128));
        assert_eq!(prepared.frames, 3);
        assert_eq!(prepared.image.mime, "image/gif");
        assert_eq!(decoded_size(&prepared.image.data), (128, 128));
        assert_eq!(
            gif_duration(&prepared.image.data),
            Duration::from_millis(300)
        );
    }

    #[test]
    fn fitting_gif_is_uploaded_unchanged() {
        let raw = gif(32, 2);
        let prepared = prepare(&raw).unwrap();
        assert_eq!(prepared.image.data, raw);
        assert_eq!(prepared.frames, 2);
        assert!(!prepared.resized);
    }

    #[test]
    fn gif_with_too_many_frames_is_rejected() {
        let error = prepare(&gif(16, MAX_GIF_FRAMES + 1)).unwrap_err();
        assert!(error.to_string().contains("mehr als 70"), "{error}");
        assert!(prepare(&gif(16, MAX_GIF_FRAMES)).is_ok());
    }

    #[test]
    fn non_images_and_oversized_files_are_rejected() {
        assert!(prepare(b"<svg></svg>").is_err());
        assert!(prepare(&vec![0; MAX_INPUT_BYTES + 1]).is_err());
    }
}

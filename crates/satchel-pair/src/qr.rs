//! QR image of an invite. The desktop shows it. This crate does not open a camera.

use std::io::Cursor;

use image::ImageEncoder;
use qrcode::QrCode;

use crate::Error;
use crate::announce::Invite;

/// PNG of `invite`. The payload is the invite JSON and nothing else.
pub fn invite_qr_png(invite: &Invite) -> Result<Vec<u8>, Error> {
    let text = invite.encode()?;
    let code = QrCode::new(text.as_bytes()).map_err(|_| Error::Message)?;
    let image = code
        .render::<image::Luma<u8>>()
        .quiet_zone(true)
        .min_dimensions(280, 280)
        .build();
    let mut png = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::L8,
        )
        .map_err(|_| Error::Message)?;
    Ok(png.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_round_trips_the_invite_json() {
        let invite = Invite::new(
            "482193",
            "127.0.0.1:9",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("invite");
        let png = invite_qr_png(&invite).expect("png");
        assert!(png.starts_with(b"\x89PNG"));
        let image = image::load_from_memory(&png)
            .expect("decode png")
            .into_luma8();
        let mut prepared = rqrr::PreparedImage::prepare(image);
        let grids = prepared.detect_grids();
        assert_eq!(grids.len(), 1);
        let (_meta, content) = grids[0].decode().expect("qr");
        let decoded = Invite::decode(&content).expect("invite json");
        assert_eq!(decoded, invite);
        assert!(!content.contains("recovery"));
    }
}

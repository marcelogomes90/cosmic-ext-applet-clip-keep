use image::ImageReader;

use super::model::Thumbnail;

pub const MAX_EDGE: u32 = 256;

pub fn generate(body: &[u8]) -> Option<Thumbnail> {
    let reader = ImageReader::new(std::io::Cursor::new(body))
        .with_guessed_format()
        .ok()?;

    let decoded = reader.decode().ok()?;
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return None;
    }

    let scaled = if width.max(height) > MAX_EDGE {
        decoded.thumbnail(MAX_EDGE, MAX_EDGE)
    } else {
        decoded
    };

    let mut png = Vec::new();
    scaled
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;

    Some(Thumbnail { width, height, png })
}

pub fn fit(width: u32, height: u32, max_height: u16) -> (u32, u32) {
    let max_height = u32::from(max_height);
    if height <= max_height || height == 0 {
        return (width, height);
    }

    let scaled = u64::from(width) * u64::from(max_height) / u64::from(height);
    (u32::try_from(scaled).unwrap_or(u32::MAX).max(1), max_height)
}

pub fn fit_within(width: u32, height: u32, max_width: u16, max_height: u16) -> (u32, u32) {
    let (width, height) = fit(width, height, max_height);
    let max_width = u32::from(max_width);
    if width <= max_width || width == 0 {
        return (width, height);
    }

    let scaled = u64::from(height) * u64::from(max_width) / u64::from(width);
    (max_width, u32::try_from(scaled).unwrap_or(u32::MAX).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_4x2() -> Vec<u8> {
        let mut buffer = Vec::new();
        let image = image::RgbaImage::from_pixel(4, 2, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut buffer),
                image::ImageFormat::Png,
            )
            .unwrap();
        buffer
    }

    #[test]
    fn a_small_image_keeps_its_dimensions() {
        let thumbnail = generate(&png_4x2()).unwrap();

        assert_eq!((thumbnail.width, thumbnail.height), (4, 2));
        assert!(!thumbnail.png.is_empty());
    }

    #[test]
    fn a_large_image_reports_its_original_size() {
        let mut buffer = Vec::new();
        let image = image::RgbaImage::from_pixel(1000, 500, image::Rgba([0, 0, 255, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut buffer),
                image::ImageFormat::Png,
            )
            .unwrap();

        let thumbnail = generate(&buffer).unwrap();

        assert_eq!((thumbnail.width, thumbnail.height), (1000, 500));
        assert!(
            thumbnail.png.len() < buffer.len(),
            "the thumbnail should be smaller than the original"
        );
    }

    #[test]
    fn nonsense_bytes_are_not_an_image() {
        assert!(generate(b"this is not a png").is_none());
        assert!(generate(&[]).is_none());
    }

    #[test]
    fn fitting_preserves_the_aspect_ratio() {
        assert_eq!(fit(1920, 1080, 48), (85, 48));
    }

    #[test]
    fn fitting_never_enlarges() {
        assert_eq!(fit(20, 10, 48), (20, 10));
    }

    #[test]
    fn a_very_wide_image_still_gets_at_least_one_pixel() {
        assert_eq!(fit(1, 10_000, 48), (1, 48));
    }

    #[test]
    fn a_box_bounds_the_taller_side_first() {
        assert_eq!(fit_within(1000, 2000, 256, 192), (96, 192));
    }

    #[test]
    fn a_box_bounds_the_wider_side_too() {
        assert_eq!(fit_within(1920, 1080, 256, 192), (256, 144));
    }

    #[test]
    fn a_box_never_enlarges() {
        assert_eq!(fit_within(20, 10, 256, 192), (20, 10));
    }

    #[test]
    fn a_panoramic_image_still_gets_at_least_one_pixel() {
        assert_eq!(fit_within(10_000, 1, 256, 192), (256, 1));
    }
}

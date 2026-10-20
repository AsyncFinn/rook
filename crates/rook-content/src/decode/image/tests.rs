use super::*;

/// A 4 × 2 PNG: the left half red, the right half transparent.
fn png() -> Vec<u8> {
    let mut img = ::image::RgbaImage::new(4, 2);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = if x < 2 {
            ::image::Rgba([255, 0, 0, 255])
        } else {
            ::image::Rgba([0, 255, 0, 0])
        };
    }
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ::image::ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn decodes_png_with_mips() {
    let img = Image::decode(&png()).unwrap();
    assert_eq!(
        (img.width, img.height, img.format),
        (4, 2, ImageFormat::Png)
    );
    assert!(img.translucent);
    // 4x2, 2x1, 1x1.
    assert_eq!(
        img.mips.iter().map(Vec::len).collect::<Vec<_>>(),
        [32, 8, 4]
    );
    // The transparent half doesn't tint the red: 2x1 is red, then
    // transparent; 1x1 is red at half alpha.
    assert_eq!(&img.mips[1][..4], &[255, 0, 0, 255]);
    assert_eq!(img.mips[1][7], 0);
    assert_eq!(img.mips[2], [255, 0, 0, 128]);
}

#[test]
fn other_formats() {
    let src = ::image::DynamicImage::ImageRgba8(::image::RgbaImage::from_pixel(
        8,
        8,
        ::image::Rgba([10, 200, 30, 255]),
    ));
    for (format, codec) in [
        (ImageFormat::Jpeg, ::image::ImageFormat::Jpeg),
        (ImageFormat::Bmp, ::image::ImageFormat::Bmp),
    ] {
        let mut bytes = Vec::new();
        let img = if format == ImageFormat::Jpeg {
            ::image::DynamicImage::ImageRgb8(src.to_rgb8())
        } else {
            src.clone()
        };
        img.write_to(&mut Cursor::new(&mut bytes), codec).unwrap();
        let decoded = Image::decode(&bytes).unwrap();
        assert_eq!((decoded.format, decoded.width), (format, 8));
        assert!(!decoded.translucent);
        assert_eq!(decoded.mips.len(), 4);
    }
}

/// A 4 × 4 DXT1 DDS: one block whose two colours are pure red and
/// pure blue, every texel the first.
#[test]
fn decodes_dds() {
    let mut b = b"DDS ".to_vec();
    let mut header = [0u32; 31];
    header[0] = 124; // size
    header[1] = 0x1 | 0x2 | 0x4 | 0x1000 | 0x8_0000; // flags
    header[2] = 4; // height
    header[3] = 4; // width
    header[4] = 8; // linear size
    header[18] = 32; // pixel format size
    header[19] = 0x4; // FOURCC
    header[20] = u32::from_le_bytes(*b"DXT1");
    header[26] = 0x1000; // caps: texture
    for v in header {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&0xF800u16.to_le_bytes());
    b.extend_from_slice(&0x001Fu16.to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    let img = Image::decode(&b).unwrap();
    assert_eq!(
        (img.format, img.width, img.height),
        (ImageFormat::Dds, 4, 4)
    );
    assert_eq!(&img.mips[0][..4], &[255, 0, 0, 255]);
}

#[test]
fn rejects_what_isnt_an_image() {
    assert_eq!(
        Image::decode(b"version 2.00\n..."),
        Err(ImageError::NotAnImage)
    );
    assert_eq!(Image::decode(b"OggS...."), Err(ImageError::NotAnImage));
    let full = png();
    assert!(matches!(
        Image::decode(&full[..full.len() / 2]),
        Err(ImageError::Damaged("PNG", _))
    ));
    // A PNG signature followed by junk.
    let mut junk = full[..16].to_vec();
    junk.extend_from_slice(&[7; 64]);
    assert!(matches!(
        Image::decode(&junk),
        Err(ImageError::Damaged("PNG", _))
    ));
}

#[test]
fn halves_images_past_the_limit() {
    let big = ::image::RgbaImage::from_pixel(2048, 4, ::image::Rgba([1, 2, 3, 255]));
    let mut bytes = Vec::new();
    big.write_to(&mut Cursor::new(&mut bytes), ::image::ImageFormat::Png)
        .unwrap();
    let img = Image::decode(&bytes).unwrap();
    assert_eq!((img.width, img.height), (1024, 2));
}

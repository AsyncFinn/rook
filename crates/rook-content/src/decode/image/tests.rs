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

/// A 4 × 4 DXT1 DDS: one block whose two colors are pure red and
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

fn png_of(img: &::image::RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ::image::ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn halves_images_past_the_limit() {
    let big = ::image::RgbaImage::from_pixel(8192, 4, ::image::Rgba([1, 2, 3, 255]));
    let img = Image::decode(&png_of(&big)).unwrap();
    assert_eq!((img.width, img.height), (4096, 2));
    // GUI measures it fitted to 1024.
    assert_eq!(img.gui_size, (1024, 1));
}

/// Past 1024 a texture is kept in blocks (BC3 where it's translucent);
/// up to it, and decoded for its pixels, RGBA8 as before.
#[test]
fn keeps_large_uploads_in_blocks() {
    let opaque = ::image::RgbaImage::from_pixel(2048, 8, ::image::Rgba([1, 2, 3, 255]));
    let img = Image::decode_texture(&png_of(&opaque)).unwrap();
    assert_eq!((img.width, img.encoding), (2048, Encoding::Bc1));
    assert_eq!(img.mips.len(), 12);
    let clear = ::image::RgbaImage::from_pixel(2048, 8, ::image::Rgba([1, 2, 3, 9]));
    let img = Image::decode_texture(&png_of(&clear)).unwrap();
    assert_eq!(img.encoding, Encoding::Bc3);
    assert!(img.translucent);
    let small = ::image::RgbaImage::from_pixel(1024, 8, ::image::Rgba([1, 2, 3, 255]));
    assert_eq!(
        Image::decode_texture(&png_of(&small)).unwrap().encoding,
        Encoding::Rgba8
    );
    let pixels = Image::decode(&png_of(&opaque)).unwrap();
    assert_eq!((pixels.width, pixels.encoding), (2048, Encoding::Rgba8));
}

/// A transcode holding every level is kept in its blocks, sized as
/// uploaded; a single level is decoded to RGBA8 with Rook's own mips.
#[test]
fn keeps_transcoded_textures_in_blocks() {
    let block = ktx2::tests::bc1_block(0xF800);
    let chain = ktx2::tests::file(
        131,
        (8, 4),
        &[block.repeat(2), block.clone(), block.clone(), block.clone()],
        &[("RobloxOriginalWidth", "10"), ("RobloxOriginalHeight", "5")],
        true,
    );
    let img = Image::decode(&chain).unwrap();
    assert_eq!(
        (img.width, img.height, img.format, img.encoding),
        (8, 4, ImageFormat::Ktx2, Encoding::Bc1)
    );
    assert_eq!(img.mips.len(), 4);
    assert_eq!(img.gui_size, (10, 5));
    assert!(!img.translucent);
    assert_eq!(img.average(), [255, 0, 0, 255]);

    let level = ktx2::tests::file(131, (8, 8), &[block.repeat(4)], &[], false);
    let img = Image::decode(&level).unwrap();
    assert_eq!((img.encoding, img.mips.len()), (Encoding::Rgba8, 4));
}

/// Studio 0.748's measurements.
#[test]
fn gui_measures_images_fitted_to_1024() {
    assert_eq!(gui_size((4096, 4096)), (1024, 1024));
    assert_eq!(gui_size((1264, 632)), (1024, 512));
    assert_eq!(gui_size((1000, 3)), (1000, 3));
    assert_eq!(gui_size((3000, 1)), (1024, 1));
}

#[test]
fn rejects_zero_max_size() {
    let rgba = ::image::RgbaImage::from_pixel(1, 1, ::image::Rgba([1, 2, 3, 255]));
    assert!(matches!(
        Image::from_rgba(rgba, ImageFormat::Png, 0),
        Err(ImageError::Damaged("PNG", _))
    ));
}

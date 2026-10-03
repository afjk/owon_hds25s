use owon_core::waveform;
use serde_json::Value;
use std::{
    fs::OpenOptions,
    io::{Cursor, Write},
    path::Path,
};

pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > 32 * 1024 * 1024 {
        return Err("出力サイズが不正です（最大32 MiB）".into());
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("上書きせず保存します: {e}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("保存失敗。作成途中のファイルを確認してください: {e}"))
}
pub fn table(record: Value, channel: &str, format: &str) -> Result<Vec<u8>, String> {
    let frame = waveform::from_record(record)?;
    let names: &[&str] = match channel {
        "both" => &["CH1", "CH2"],
        "CH1" => &["CH1"],
        "CH2" => &["CH2"],
        _ => return Err("CH選択が不正です".into()),
    };
    let separator = match format {
        "csv" => ",",
        "txt" => "\t",
        _ => return Err("表の形式が不正です".into()),
    };
    let mut rows = vec![
        "# uncalibrated signed screen bytes; index is not ADC sample index".into(),
        format!(
            "byte_index{separator}{}",
            names
                .iter()
                .map(|name| format!("{name}_raw"))
                .collect::<Vec<_>>()
                .join(separator)
        ),
    ];
    let count = names
        .iter()
        .filter_map(|name| frame.values.get(*name))
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    for i in 0..count {
        let values = names
            .iter()
            .map(|name| {
                frame
                    .values
                    .get(*name)
                    .and_then(|v| v.get(i))
                    .map(ToString::to_string)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        rows.push(format!("{i}{separator}{}", values.join(separator)));
    }
    Ok((rows.join("\n") + "\n").into_bytes())
}
pub fn image(png: &[u8], format: &str) -> Result<Vec<u8>, String> {
    use image::{ImageFormat, ImageReader, Limits};
    if png.len() > 5 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("PNG入力が不正です（最大5 MiB）".into());
    }
    let format = match format {
        "png" => ImageFormat::Png,
        "bmp" => ImageFormat::Bmp,
        "gif" => ImageFormat::Gif,
        _ => return Err("画像形式が不正です".into()),
    };
    let mut reader = ImageReader::with_format(Cursor::new(png), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    // The plot has a painted background; flatten to RGB to avoid BMP alpha compatibility issues.
    let decoded = image::DynamicImage::ImageRgb8(decoded.to_rgb8());
    let mut output = Cursor::new(Vec::new());
    decoded
        .write_to(&mut output, format)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_export_preserves_signed_values_and_selection() {
        let r = serde_json::from_str(include_str!(
            "../../crates/owon-core/tests/fixtures/synthetic-python-capture.json"
        ))
        .unwrap();
        let bytes = table(r, "CH1", "txt").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("byte_index\tCH1_raw\n0\t89\n"));
        assert!(!text.contains("CH2_raw"));
        assert_eq!(text.lines().count(), 602);
    }
    #[test]
    fn three_real_image_formats_roundtrip() {
        let picture = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            3,
            2,
            image::Rgb([10, 20, 30]),
        ));
        let mut png = Cursor::new(Vec::new());
        picture.write_to(&mut png, image::ImageFormat::Png).unwrap();
        for (name, format) in [
            ("png", image::ImageFormat::Png),
            ("bmp", image::ImageFormat::Bmp),
            ("gif", image::ImageFormat::Gif),
        ] {
            let bytes = image(png.get_ref(), name).unwrap();
            let loaded = image::load_from_memory_with_format(&bytes, format).unwrap();
            assert_eq!((loaded.width(), loaded.height()), (3, 2));
        }
        assert!(image(b"not a PNG", "png").is_err());
    }
    #[test]
    fn existing_files_are_never_replaced() {
        let path = std::env::temp_dir().join(format!(
            "owon-export-{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_new(&path, b"original").unwrap();
        assert!(write_new(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::remove_file(path).unwrap();
    }
}

//! OCR 集成（审计 Q6）：tesseract 外部进程调用，补齐"图片/扫描件不可检索"的缺口。
//!
//! - `ocr_image_file`：对单张图片执行 OCR（中文 chi_sim），返回识别文本；
//! - `extract_pdf_images`：从 PDF 页面资源提取 XObject 图片到临时文件（扫描件
//!   = 无文本层 + 每页一张图片），供 OCR 消费；
//! - **静默降级**：找不到 tesseract（未配置 `MEMORI_OCR_TESSERACT_PATH` 且 PATH
//!   无 `tesseract`）或识别失败时返回 None/空列表，既有提取链路行为完全不变。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tracing::{info, warn};

/// 单张图片 OCR 的超时上限（大图 30s 足够）。
const OCR_TIMEOUT_SECS: u64 = 30;
/// 跳过超大图片（防病态文档拖死索引）。
pub(crate) const MAX_OCR_IMAGE_BYTES: usize = 20 * 1024 * 1024;
/// tesseract 路径环境变量名（server/desktop 启动时从 settings 注入）。
pub const OCR_TESSERACT_PATH_ENV: &str = "MEMORI_OCR_TESSERACT_PATH";
/// 页面分割模式：PSM 4（单列可变尺寸）。实测 PSM 3（全自动）在图文混排/扫描件上
/// 输出严重乱序（单字碎片），PSM 4 按列顺序输出，对检索场景显著更优。
const OCR_PSM: &str = "4";

/// 临时文件唯一序号（进程内自增，防并发索引时临时文件互相覆盖）。
static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 取下一个临时文件序号（进程内唯一递增）。
pub(crate) fn next_temp_seq() -> u64 {
    TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// tesseract 路径探测结果缓存（每次调用不再重复 spawn --version）。
static TESSERACT_CACHE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 解析 tesseract 可执行文件：`MEMORI_OCR_TESSERACT_PATH` 优先，回退 PATH 查找。
/// 结果进程内缓存一次（失败也缓存，避免每张图重复探测）。
fn resolve_tesseract() -> Option<PathBuf> {
    TESSERACT_CACHE
        .get_or_init(|| {
            if let Ok(configured) = std::env::var(OCR_TESSERACT_PATH_ENV) {
                let path = PathBuf::from(configured.trim());
                if path.is_file() {
                    return Some(path);
                }
                warn!(
                    path = %path.display(),
                    "MEMORI_OCR_TESSERACT_PATH 指向的文件不存在，跳过 OCR"
                );
                return None;
            }
            let name = if cfg!(windows) {
                "tesseract.exe"
            } else {
                "tesseract"
            };
            let path = PathBuf::from(name);
            if std::process::Command::new(&path)
                .arg("--version")
                .output()
                .is_ok()
            {
                return Some(path);
            }
            None
        })
        .clone()
}

/// 检测 OCR 是否可用（找不到 tesseract 时调用方直接跳过）。
pub fn ocr_available() -> bool {
    resolve_tesseract().is_some()
}

/// 对单张图片执行 OCR（chi_sim 中文）。任何失败返回 None，调用方静默降级。
pub fn ocr_image_file(path: &Path) -> Option<String> {
    let tesseract = resolve_tesseract()?;
    let started = std::time::Instant::now();
    // spawn + 轮询等待实现超时：tesseract 卡死时强制终止，不拖住索引线程。
    let mut child = match Command::new(&tesseract)
        .arg(path)
        .arg("stdout")
        .arg("-l")
        .arg("chi_sim")
        .arg("--psm")
        .arg(OCR_PSM)
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            warn!(path = %path.display(), "tesseract 启动失败，跳过 OCR");
            return None;
        }
    };
    let deadline = Duration::from_secs(OCR_TIMEOUT_SECS);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            if !status.success() {
                warn!(
                    path = %path.display(),
                    status = %status,
                    "tesseract 识别失败，跳过 OCR"
                );
                return None;
            }
            break;
        }
        if started.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            warn!(
                path = %path.display(),
                timeout_secs = OCR_TIMEOUT_SECS,
                "OCR 超时已终止"
            );
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let Ok(output) = child.wait_with_output() else {
        return None;
    };
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return None;
    }
    info!(
        path = %path.display(),
        chars = text.chars().count(),
        elapsed_ms = started.elapsed().as_millis(),
        "OCR 识别完成"
    );
    Some(text)
}

/// 从 PDF 提取页面 XObject 图片到临时目录，返回图片文件路径列表。
/// 支持 DCTDecode（JPEG 直写）与 FlateDecode（raw 像素 → PNG 编码）；其余过滤跳过。
pub fn extract_pdf_images(pdf_path: &Path) -> Vec<PathBuf> {
    let Ok(doc) = lopdf::Document::load(pdf_path) else {
        warn!(path = %pdf_path.display(), "PDF 加载失败，无法提取内嵌图片");
        return Vec::new();
    };
    let pages = doc.get_pages();
    let mut images = Vec::new();
    for (page_num, page_id) in pages {
        // lopdf 的 get_page_resources 第一个返回值是页面资源字典（含继承），
        // 需要自行遍历 /XObject 条目并 dereference 每个值（Reference 或直接 Stream）。
        let Ok((Some(resources), _)) = doc.get_page_resources(page_id) else {
            continue;
        };
        let Some(xobjects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|value| value.as_dict().ok())
        else {
            continue;
        };
        for (_, value) in xobjects.iter() {
            let Some(stream) = (match value {
                lopdf::Object::Reference(object_id) => doc
                    .get_object(*object_id)
                    .ok()
                    .and_then(|obj| obj.as_stream().ok()),
                lopdf::Object::Stream(stream) => Some(stream),
                _ => None,
            }) else {
                continue;
            };
            if !is_image_stream(stream) {
                continue;
            }
            if stream.content.len() > MAX_OCR_IMAGE_BYTES {
                warn!(page = page_num, "PDF 图片流过大，跳过 OCR");
                continue;
            }
            let Some(path) = write_pdf_image_file(pdf_path, page_num, stream) else {
                continue;
            };
            images.push(path);
        }
    }
    images
}

/// 判断流是否为图片 XObject。
fn is_image_stream(stream: &lopdf::Stream) -> bool {
    stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        .is_some_and(|name| name == b"Image")
}

/// 把 PDF 图片流解码写为临时文件（.jpg 或 .png）。
fn write_pdf_image_file(pdf_path: &Path, page_num: u32, stream: &lopdf::Stream) -> Option<PathBuf> {
    // Filter 可能是单个 Name，也可能是链式数组（如 [ASCII85Decode FlateDecode]）。
    let filters: Vec<&[u8]> = match stream.dict.get(b"Filter").ok() {
        Some(lopdf::Object::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_name().ok())
            .collect(),
        Some(value) => value.as_name().ok().into_iter().collect(),
        None => Vec::new(),
    };

    let (ext, bytes) = match filters.as_slice() {
        // DCTDecode = 完整 JPEG 数据，直写。
        [b"DCTDecode"] => ("jpg", stream.content.clone()),
        // 链式解码（FlateDecode / ASCII85Decode / ASCIIHexDecode）后为 raw 像素，
        // 按宽度/高度/通道数编码为 PNG。
        filters if filters.contains(&&b"FlateDecode"[..]) || filters.is_empty() => {
            let raw = decode_stream_filters(filters, &stream.content)?;
            let (width, height, channels) = image_dimensions(stream)?;
            let png = encode_raw_to_png(&raw, width, height, channels)?;
            ("png", png)
        }
        _ => return None, // CCITTFax / JPXDecode 等暂不支持，静默跳过
    };

    let base = pdf_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "pdf_image".to_string());
    let dir = std::env::temp_dir().join("memori-ocr");
    std::fs::create_dir_all(&dir).ok()?;
    // 唯一名：pid + 原子序号（防同页多图/并发索引时临时文件互相覆盖）。
    let seq = next_temp_seq();
    let path = dir.join(format!(
        "{base}_p{page_num}_{}_{seq}.{ext}",
        std::process::id()
    ));
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// 按顺序执行过滤器链解码（PDF 规范：先应用的列在前）。
fn decode_stream_filters(filters: &[&[u8]], content: &[u8]) -> Option<Vec<u8>> {
    let mut data = content.to_vec();
    for filter in filters {
        match *filter {
            b"FlateDecode" => {
                // PDF 的 FlateDecode = zlib 封装（RFC1950）。
                let mut out = Vec::new();
                flate2::read::ZlibDecoder::new(&data[..])
                    .read_to_end(&mut out)
                    .ok()?;
                data = out;
            }
            b"ASCII85Decode" => data = ascii85_decode(&data)?,
            b"ASCIIHexDecode" => data = ascii_hex_decode(&data)?,
            _ => return None,
        }
    }
    Some(data)
}

/// ASCII85 解码（PDF 规范：'!'..'u' 参与，'z' = 4 零字节，'~>' 终止）。
fn ascii85_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 5 * 4);
    let mut group = [0u8; 5];
    let mut group_len = 0;
    for &byte in input {
        if byte == b'~' {
            break; // 终止符，剩余组按短组处理
        }
        if byte == b'z' && group_len == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(33..=117).contains(&byte) {
            continue; // 忽略空白等
        }
        group[group_len] = byte;
        group_len += 1;
        if group_len == 5 {
            out.extend_from_slice(&decode_ascii85_group(&group)?.to_be_bytes());
            group_len = 0;
        }
    }
    if group_len > 0 {
        // 短组：补 'u'（84）凑满 5 位解码，输出 group_len-1 字节。
        for item in group.iter_mut().skip(group_len) {
            *item = b'u';
        }
        let decoded = decode_ascii85_group(&group)?;
        out.extend_from_slice(&decoded.to_be_bytes()[..group_len - 1]);
    }
    Some(out)
}

/// 5 个 ASCII85 字符解码为一个 u32。
fn decode_ascii85_group(group: &[u8; 5]) -> Option<u32> {
    group.iter().try_fold(0u32, |acc, &item| {
        acc.checked_mul(85)?
            .checked_add((item as u32).checked_sub(33)?)
    })
}

/// ASCIIHex 解码（PDF 规范：十六进制对，'>' 终止，忽略空白）。
fn ascii_hex_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 2);
    let mut hi: Option<u8> = None;
    let hex_value = |byte: u8| -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    };
    for &byte in input {
        if byte == b'>' {
            break;
        }
        let Some(value) = hex_value(byte) else {
            continue;
        };
        match hi.take() {
            None => hi = Some(value),
            Some(high) => out.push(high << 4 | value),
        }
    }
    if let Some(high) = hi {
        out.push(high << 4); // 奇数个十六进制位，末位补 0
    }
    Some(out)
}

/// 读取图片流字典中的宽度/高度/通道数。
fn image_dimensions(stream: &lopdf::Stream) -> Option<(u32, u32, u8)> {
    let width = stream
        .dict
        .get(b"Width")
        .ok()
        .and_then(|v| v.as_i64().ok())?;
    let height = stream
        .dict
        .get(b"Height")
        .ok()
        .and_then(|v| v.as_i64().ok())?;
    let color_space = stream
        .dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|v| v.as_name().ok());
    let channels = match color_space {
        Some(name) if name == b"DeviceGray" => 1,
        Some(name) if name == b"DeviceRGB" => 3,
        // 索引色/其它色彩空间暂不支持编码，跳过（不参与 OCR）。
        _ => return None,
    };
    Some((width as u32, height as u32, channels))
}

/// raw 像素字节编码为 PNG（灰度 1 通道 / RGB 3 通道）。
fn encode_raw_to_png(raw: &[u8], width: u32, height: u32, channels: u8) -> Option<Vec<u8>> {
    use image::{ImageBuffer, Luma, Rgb};
    let expected = width as usize * height as usize * channels as usize;
    if raw.len() < expected {
        return None;
    }
    let mut png = Vec::new();
    match channels {
        1 => {
            let img: ImageBuffer<Luma<u8>, _> =
                ImageBuffer::from_raw(width, height, raw[..expected].to_vec())?;
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .ok()?;
        }
        3 => {
            let img: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(width, height, raw[..expected].to_vec())?;
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .ok()?;
        }
        _ => return None,
    }
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未配置 tesseract 时 OCR 静默返回 None（环境无关的降级行为）。
    #[test]
    fn ocr_returns_none_when_tesseract_missing() {
        unsafe {
            std::env::set_var("MEMORI_OCR_TESSERACT_PATH", "__definitely_missing__.exe");
        }
        let result = ocr_image_file(Path::new("does-not-matter.png"));
        assert!(result.is_none());
        unsafe {
            std::env::remove_var("MEMORI_OCR_TESSERACT_PATH");
        }
    }

    /// 不是图片的流不会被当作图片提取。
    #[test]
    fn non_image_stream_is_rejected() {
        let mut dict = lopdf::Dictionary::new();
        dict.set("Subtype", "Form");
        let stream = lopdf::Stream {
            dict,
            content: vec![0u8; 4],
            allows_compression: true,
            start_position: None,
        };
        assert!(!is_image_stream(&stream));
    }
}

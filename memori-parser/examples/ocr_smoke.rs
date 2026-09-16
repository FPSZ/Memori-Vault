//! OCR 冒烟工具：打印给定文件的提取文本（含扫描 PDF 的 OCR 回退）。
//!
//! tesseract 路径解析顺序：环境变量 > settings.json 的 ocr_tesseract_path > PATH。
//!
//! 用法：
//!   cargo run -p memori-parser --example ocr_smoke -- <文件路径>

use memori_parser::{extract_document_text, extract_pdf_images, ocr_available};

fn main() {
    // 与 server/desktop 启动注入一致：settings.json 的 ocr_tesseract_path 作为 fallback，
    // 让本工具开箱即用（不覆盖已显式设置的环境变量）。
    if std::env::var_os(memori_parser::OCR_TESSERACT_PATH_ENV).is_none()
        && let Some(path) = read_ocr_path_from_settings()
        && !path.trim().is_empty()
    {
        unsafe {
            std::env::set_var(memori_parser::OCR_TESSERACT_PATH_ENV, path);
        }
    }
    let path = std::env::args().nth(1).expect("用法: ocr_smoke <文件路径>");
    println!("ocr_available = {}", ocr_available());
    // 深挖：lopdf 视角的页面资源与 XObject 结构
    if path.ends_with(".pdf")
        && let Ok(doc) = lopdf::Document::load(&path)
    {
        for (page_num, page_id) in doc.get_pages() {
            println!("page {page_num} (id {page_id:?})");
            match doc.get_page_resources(page_id) {
                Ok((_, xobject_ids)) => {
                    println!("  xobject ids: {xobject_ids:?}");
                    for id in xobject_ids {
                        match doc.get_object(id) {
                            Ok(obj) => {
                                let kind = match obj {
                                    lopdf::Object::Stream(s) => format!(
                                        "Stream subtype={:?} filter={:?} w={:?} h={:?} bytes={}",
                                        s.dict.get(b"Subtype").ok().and_then(|v| v.as_name().ok()),
                                        s.dict.get(b"Filter").ok().and_then(|v| v.as_name().ok()),
                                        s.dict.get(b"Width").ok().and_then(|v| v.as_i64().ok()),
                                        s.dict.get(b"Height").ok().and_then(|v| v.as_i64().ok()),
                                        s.content.len()
                                    ),
                                    other => format!("{other:?}"),
                                };
                                println!("  xobject {id:?}: {kind}");
                            }
                            Err(err) => println!("  xobject {id:?}: get err {err}"),
                        }
                    }
                }
                Err(err) => println!("  resources err: {err}"),
            }
        }
    }
    let images = extract_pdf_images(std::path::Path::new(&path));
    println!("extracted images = {}", images.len());
    let started = std::time::Instant::now();
    match extract_document_text(&path) {
        Some(text) => {
            println!(
                "提取成功，{} 字符，耗时 {}ms",
                text.chars().count(),
                started.elapsed().as_millis()
            );
            println!("----- 文本预览 -----");
            let preview: String = text.chars().take(600).collect();
            println!("{preview}");
        }
        None => {
            println!("提取失败（可能无文本层且 OCR 不可用）");
        }
    }
}

/// 从用户配置目录的 settings.json 读取 ocr_tesseract_path（与 server/desktop 同源）。
fn read_ocr_path_from_settings() -> Option<String> {
    let settings_path = if cfg!(windows) {
        std::env::var("APPDATA").ok().map(|dir| {
            std::path::PathBuf::from(dir)
                .join("Memori-Vault")
                .join("settings.json")
        })
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|dir| std::path::PathBuf::from(dir).join(".config"))
            })
            .map(|dir| dir.join("Memori-Vault").join("settings.json"))
    }?;
    let raw = std::fs::read_to_string(settings_path).ok()?;
    // 轻量解析：只取需要的字段，避免给 parser 引入 serde 依赖。
    raw.split('\n')
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim().trim_matches('"') == "ocr_tesseract_path").then(|| {
                value
                    .trim()
                    .trim_end_matches(',')
                    .trim_matches('"')
                    .to_string()
            })
        })
        .filter(|value| !value.is_empty() && *value != "null")
}

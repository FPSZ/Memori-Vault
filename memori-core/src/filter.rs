use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::debug;

/// 文件索引筛选配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexFilterConfig {
    pub enabled: bool,
    /// 白名单扩展名（如 ["md", "txt"]），空表示全部支持类型
    #[serde(default)]
    pub include_extensions: Vec<String>,
    /// 黑名单扩展名
    #[serde(default)]
    pub exclude_extensions: Vec<String>,
    /// 排除路径模式（glob，相对于 watch_root）
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    /// 手动包含路径（glob，优先级最高，可覆盖排除规则）
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// 最小修改日期 (YYYY-MM-DD)
    pub min_mtime: Option<String>,
    /// 最大修改日期 (YYYY-MM-DD)
    pub max_mtime: Option<String>,
    /// 最小文件大小（字节）
    pub min_size: Option<u64>,
    /// 最大文件大小（字节）
    pub max_size: Option<u64>,
}

/// 判断给定文件是否应该被索引
///
/// 筛选优先级：
/// 1. 若 filter 未启用或不存在，全部通过
/// 2. 若路径匹配 include_paths（glob），直接通过
/// 3. 若路径匹配 exclude_paths（glob），直接拒绝
/// 4. 检查扩展名白名单/黑名单
/// 5. 若提供了 metadata，检查文件大小和修改日期范围
pub fn should_index_file(
    path: &Path,
    filter: Option<&IndexFilterConfig>,
    watch_root: Option<&Path>,
    file_size: Option<u64>,
    mtime_secs: Option<i64>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    if !filter.enabled {
        return true;
    }

    // 计算相对路径（用于 glob 匹配）
    let relative = watch_root
        .and_then(|root| path.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));

    // 2. 手动包含（优先级最高）
    if matches_glob_any(&relative, &filter.include_paths) {
        return true;
    }

    // 3. 排除路径
    if matches_glob_any(&relative, &filter.exclude_paths) {
        debug!(path = %path.display(), "文件被 exclude_paths 规则排除");
        return false;
    }

    // 4. 扩展名检查
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if !filter.include_extensions.is_empty()
        && !filter
            .include_extensions
            .iter()
            .any(|e| e.to_ascii_lowercase() == ext)
    {
        debug!(path = %path.display(), ext = %ext, "文件扩展名不在白名单中");
        return false;
    }

    if filter
        .exclude_extensions
        .iter()
        .any(|e| e.to_ascii_lowercase() == ext)
    {
        debug!(path = %path.display(), ext = %ext, "文件扩展名在黑名单中");
        return false;
    }

    // 5. 文件大小范围
    if let Some(size) = file_size {
        if let Some(min) = filter.min_size {
            if size < min {
                debug!(path = %path.display(), size = size, min = min, "文件大小小于最小值");
                return false;
            }
        }
        if let Some(max) = filter.max_size {
            if size > max {
                debug!(path = %path.display(), size = size, max = max, "文件大小大于最大值");
                return false;
            }
        }
    }

    // 6. 修改日期范围
    if let Some(mtime) = mtime_secs {
        if let Some(ref min_date) = filter.min_mtime {
            if let Ok(min_ts) = date_str_to_timestamp(min_date) {
                if mtime < min_ts {
                    debug!(path = %path.display(), mtime = mtime, min_ts = min_ts, "文件修改日期早于最小值");
                    return false;
                }
            }
        }
        if let Some(ref max_date) = filter.max_mtime {
            if let Ok(max_ts) = date_str_to_timestamp(max_date) {
                // max_date 设为当天的 23:59:59
                let max_ts_end = max_ts + 86399;
                if mtime > max_ts_end {
                    debug!(path = %path.display(), mtime = mtime, max_ts = max_ts_end, "文件修改日期晚于最大值");
                    return false;
                }
            }
        }
    }

    true
}

/// 判断相对路径是否匹配任一 glob 模式
fn matches_glob_any(relative: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    for pattern in patterns {
        if pattern.is_empty() {
            continue;
        }
        // 支持两种写法：
        // 1. 纯前缀匹配（如 "drafts/" 或 "drafts"）
        // 2. 标准 glob（如 "**/*.tmp" 或 "temp/**"）
        let is_glob = pattern.contains('*') || pattern.contains('?');
        if is_glob {
            if let Ok(globber) = glob::Pattern::new(pattern) {
                if globber.matches(relative) {
                    return true;
                }
            }
        } else {
            // 前缀匹配：目录名或文件前缀
            let pat = pattern.trim_end_matches('/');
            let rel = relative.trim_end_matches('/');
            if rel == pat || rel.starts_with(&format!("{}/", pat)) {
                return true;
            }
        }
    }
    false
}

/// 将 YYYY-MM-DD 字符串转换为 UTC 时间戳（秒）
fn date_str_to_timestamp(date: &str) -> Result<i64, String> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("日期格式错误: {}", date));
    }
    let year: i32 = parts[0]
        .parse()
        .map_err(|_| format!("年份解析失败: {}", parts[0]))?;
    let month: u32 = parts[1]
        .parse()
        .map_err(|_| format!("月份解析失败: {}", parts[1]))?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|_| format!("日期解析失败: {}", parts[2]))?;

    // 使用 chrono 或手动计算太复杂，这里用一个简单的近似：
    // 从 1970-01-01 到目标日期的天数 * 86400
    // 简单实现：利用文件系统的本地时间转换
    // 但为了编译简单，使用一个近似算法
    let days_since_epoch = days_since_1970(year, month, day);
    Ok(days_since_epoch * 86400)
}

/// 计算从 1970-01-01 到给定日期的天数（近似，不考虑闰秒）
fn days_since_1970(year: i32, month: u32, day: u32) -> i64 {
    let is_leap = |y: i32| -> bool { y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) };
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month {
        let md = month_days[(m - 1) as usize];
        days += if m == 2 && is_leap(year) { md + 1 } else { md } as i64;
    }
    days += (day - 1) as i64;
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_glob_any() {
        assert!(matches_glob_any("temp/foo.md", &["temp".to_string()]));
        assert!(matches_glob_any("temp/foo.md", &["temp/".to_string()]));
        assert!(!matches_glob_any("template/foo.md", &["temp".to_string()]));
        assert!(matches_glob_any("drafts/foo.md", &["**/*.md".to_string()]));
        assert!(matches_glob_any("a/b/c.tmp", &["**/*.tmp".to_string()]));
        assert!(!matches_glob_any("a/b/c.md", &["**/*.tmp".to_string()]));
    }

    #[test]
    fn test_should_index_file_basic() {
        let mut filter = IndexFilterConfig::default();
        filter.enabled = true;
        filter.exclude_extensions = vec!["tmp".to_string()];
        assert!(!should_index_file(
            Path::new("/root/test.tmp"),
            Some(&filter),
            Some(Path::new("/root")),
            None,
            None
        ));
        assert!(should_index_file(
            Path::new("/root/test.md"),
            Some(&filter),
            Some(Path::new("/root")),
            None,
            None
        ));
    }

    #[test]
    fn test_should_index_file_include_paths() {
        let mut filter = IndexFilterConfig::default();
        filter.enabled = true;
        filter.exclude_paths = vec!["drafts".to_string()];
        filter.include_paths = vec!["drafts/important.md".to_string()];
        // include_paths 优先于 exclude_paths
        assert!(should_index_file(
            Path::new("/root/drafts/important.md"),
            Some(&filter),
            Some(Path::new("/root")),
            None,
            None
        ));
        assert!(!should_index_file(
            Path::new("/root/drafts/other.md"),
            Some(&filter),
            Some(Path::new("/root")),
            None,
            None
        ));
    }
}

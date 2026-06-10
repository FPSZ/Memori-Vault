/// OS keychain 集成：远程 API key 存 Windows Credential Manager / macOS Keychain /
/// Linux Secret Service，settings.json 里只保留哨兵值，明文不落盘。
///
/// 哨兵值：`__keychain__`——JSON 里出现此值时，运行时替换为 keychain 实际内容。
/// 若 keychain 不可用（无守护进程、沙箱限制），写入失败时降级为明文并记录警告。
use tracing::warn;

const SERVICE: &str = "Memori-Vault";
const API_KEY_ACCOUNT: &str = "remote_api_key";

/// settings.json 中存放的哨兵值——读取时透明替换为 keychain 实际密钥。
pub(crate) const KEYCHAIN_SENTINEL: &str = "__keychain__";

/// 将 API key 写入 OS keychain。返回错误时调用方应降级为明文存储。
pub(crate) fn save_api_key(key: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, API_KEY_ACCOUNT)
        .map_err(|err| format!("keychain 初始化失败: {err}"))?
        .set_password(key)
        .map_err(|err| format!("keychain 写入失败: {err}"))
}

/// 从 OS keychain 读取 API key；不存在或不可用时返回 None。
pub(crate) fn load_api_key() -> Option<String> {
    let entry = keyring::Entry::new(SERVICE, API_KEY_ACCOUNT).ok()?;
    match entry.get_password() {
        Ok(key) if !key.is_empty() => Some(key),
        Ok(_) => None,
        Err(keyring::Error::NoEntry) => None,
        Err(err) => {
            warn!(error = %err, "keychain 读取失败，API key 将为空");
            None
        }
    }
}

/// 从 OS keychain 删除 API key（用户清空 key 或切换到本地模式时调用）。
pub(crate) fn delete_api_key() {
    if let Ok(entry) = keyring::Entry::new(SERVICE, API_KEY_ACCOUNT) {
        match entry.delete_credential() {
            Ok(()) => {}
            Err(keyring::Error::NoEntry) => {}
            Err(err) => {
                warn!(error = %err, "keychain 删除失败（条目可能已不存在）");
            }
        }
    }
}

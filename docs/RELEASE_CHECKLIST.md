# Memori-Vault Release Checklist

这份清单用于桌面版发布前的最终确认，目标是把“能构建”提升为“可对外发布”。

## 1. 版本与文档

- [ ] `Cargo.toml` 中 `workspace.package.version` 已更新
- [ ] `ui/package.json` 中 `version` 已更新
- [ ] `memori-desktop/tauri.conf.json` 中 `version` 已更新
- [ ] 以上三个版本号保持一致
- [ ] 已编写对应版本的 release notes：`docs/RELEASE_NOTES_vX.Y.Z.md`
- [ ] `README.md` 与 `README.zh-CN.md` 的运行模式和企业版口径已同步
- [ ] `docs/enterprise.md` 与 `docs/enterprise.zh-CN.md` 的 preview/GA 口径已同步

## 2. 代码质量门

- [ ] 运行 `cargo fmt --all -- --check`
- [ ] 运行 `cargo clippy --workspace -- -D warnings`
- [ ] 运行 `cargo test --workspace`
- [ ] 运行 `npm --prefix ui ci` 或 `npm ci --prefix ui`
- [ ] 运行 `npm --prefix ui run build`
- [ ] 主 CI（`rust-ci.yml`）已通过

## 3. 发布关键行为验证

- [ ] 新增/修改 `.md` 或 `.txt` 文件后可正常进入索引
- [ ] 删除单个文档后，旧内容不会继续出现在检索结果中
- [ ] 将 `.md/.txt` 重命名为不支持后缀后，旧索引会被清理
- [ ] 删除目录后，目录内旧索引会被清理
- [ ] 索引状态不会卡在 `scanning` / `embedding`
- [ ] 设置中心可正常保存关键配置

## 4. 运行时与产品口径

- [ ] 桌面版作为当前主体验进行发布
- [ ] `memori-server` 口径保持为 server runtime / private deployment preview
- [ ] 企业能力口径保持为 preview，不对外宣称完整 GA 级企业身份安全能力
- [ ] Ollama 依赖与推荐模型说明清晰

## 5. Release Workflow

- [ ] `desktop-release.yml` 已校验 tag/version 一致
- [ ] `desktop-release.yml` 已校验 release notes 文件存在
- [ ] draft release 使用正式 `docs/RELEASE_NOTES_vX.Y.Z.md`
- [ ] 三端构建产物上传规则与当前 Tauri 输出一致

## 6. 发版前手动检查

- [ ] Windows 包可安装并启动
- [ ] Linux 包可启动并加载 UI
- [ ] macOS 包可启动并加载 UI
- [ ] 首次启动时基础流程清晰：选择目录、配置模型、发起首次检索
- [ ] About / 设置页显示的版本号与 release 版本一致

## 7. 发版后动作

- [ ] 检查 GitHub draft release 附件是否齐全
- [ ] 检查 release title、tag、notes 是否匹配
- [ ] 发布后验证下载链接可用
- [ ] 在 README 或官网入口同步最新版本说明（如适用）

## 建议发布口径

- 个人版：可作为当前主要发布目标
- 服务端 / 私有化：建议以 preview 口径发布
- 企业能力：建议明确为 private deployment preview，而不是完整 GA 企业版

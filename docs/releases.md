# 发布与 Homebrew 更新

源仓库为 `mac-fantasy/shrimpapi-email`，统一 Homebrew tap 为 `mac-fantasy/homebrew-tap`，cask token 为 `shrimpapi-email`。

## 发布新版

1. 更新 `Cargo.toml` 的版本，并运行 `cargo check` 同步 `Cargo.lock`。
2. 运行 `cargo fmt --check`、`cargo test --locked` 和 `cargo clippy --locked --all-targets -- -D warnings`。
3. 提交并推送到 `main`，确认 CI 通过。
4. 创建与包版本一致的标签并推送，例如 `git tag v0.2.1`、`git push origin v0.2.1`。

`Release` 工作流在 macOS 上验证代码、构建 `aarch64-apple-darwin` / `x86_64-apple-darwin`，通过 `lipo` 生成通用应用，设置最低 macOS 11.0，并进行 ad-hoc 签名和验证。随后生成以下附件，上传完整草稿后再公开 Release：

- `ShrimpAPI.Email-<version>-macos-universal.zip`
- `SHA256SUMS`

标签必须与 `Cargo.toml` 版本一致；打包脚本会拒绝不匹配的标签。当前发布不使用 Developer ID 证书，没有 Apple 公证。

## 同步 Homebrew

tap 的 `apps.json` 已登记本项目附件和校验清单。共用的 `Update Casks` 工作流每 6 小时检查最新稳定 Release，实际下载附件，核对 GitHub digest 和 `SHA256SUMS`，再更新 cask 的版本及 SHA-256 并提交。

立即同步可以在 tap 的 Actions 页面手动运行 `Update Casks`，维护者也可以执行：

```sh
gh workflow run update-casks.yml --repo mac-fantasy/homebrew-tap
```

不需要在本项目保存跨仓库访问令牌。应用自身没有自动更新器，安装用户通过 `brew update` 和 `brew upgrade --cask mac-fantasy/tap/shrimpapi-email` 更新。

## 设置兼容

Bundle ID、系统凭据服务名和 macOS 数据目录均使用固定标识 `com.shrimpapi.email`。安装包与应用版本不参与数据路径。普通升级和卸载保留用户模板、草稿和凭据。修改设置结构时维护 `src/settings.rs` 中的版本迁移和测试，不能通过改变路径规避迁移。

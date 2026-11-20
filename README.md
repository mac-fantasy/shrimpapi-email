# ShrimpAPI Email

使用 Rust 和 **egui / eframe** 实现的纯桌面邮件发送应用。启动后直接进入图形界面，邮件配置、收件人管理、模板编辑、预览、发送和报告导出均在窗口中完成。

界面使用 ShrimpAPI 原始品牌 Logo；应用图标裁切自左侧虾形和双向箭头，并提供完整的 macOS / Retina 图标尺寸。素材和重新生成方法见 [assets/brand](assets/brand/README.md)。

## 安装与更新

支持 macOS 11 或更新版本，安装包同时包含 Apple Silicon 和 Intel 架构。

```sh
brew install --cask mac-fantasy/tap/shrimpapi-email
```

之后通过 Homebrew 更新：

```sh
brew update
brew upgrade --cask mac-fantasy/tap/shrimpapi-email
```

安装包也可以从 [GitHub Releases](https://github.com/mac-fantasy/shrimpapi-email/releases) 下载。当前发布使用 ad-hoc 签名，尚未经过 Apple Developer ID 签名与公证；macOS 首次打开时可能需要在「系统设置 → 隐私与安全性」中允许打开。

已有手动安装的同名应用时，请先退出，并将 `/Applications/ShrimpAPI Email.app` 移到其他位置后再通过 Homebrew 安装。应用数据和系统凭据存储位置固定，升级或更换安装方式后继续使用原设置。

普通卸载保留设置、模板和系统凭据：

```sh
brew uninstall --cask mac-fantasy/tap/shrimpapi-email
```

统一 tap 每 6 小时校验最新稳定 Release 并更新 cask。发布步骤见 [docs/releases.md](docs/releases.md)。

## 打开应用

macOS 上双击 `ShrimpAPI Email.app`。首次启动会载入内置示例和三个默认模板，可以立即体验变量替换和邮件预览，不需要 API 令牌。以后启动会自动恢复上次的草稿、模板库和设置。

实际使用流程：

1. 在「连接设置」填写发件人、回复邮箱和 Cloudflare API 令牌。
2. 在「收件人」添加一个或多个邮箱，按需设置称呼和个人变量。
3. 在「邮件内容」编辑主题、HTML 或纯文本正文。
4. 在「变量与匹配规则」设置默认变量和按目标邮箱匹配的替换值。
5. 点击「生成预览」，选择收件人检查最终内容；HTML 实际排版可通过「浏览器预览 HTML」查看。
6. 点击「发送邮件」，在「发送记录」查看逐封提交结果，按需导出 JSON 报告。

发件域名需要在 Cloudflare 接入 Email Sending，API 令牌需要 **Email Sending: Edit** 权限。令牌在界面中输入、默认遮挡，并自动保存到系统凭据库，下次启动自动读取。令牌不会写入 JSON 活动配置或设置备份；清空令牌会移除已保存的凭据。

## 界面功能

| 页面 | 功能 |
| --- | --- |
| 邮件内容 | 编辑主题、HTML 和纯文本；从文件导入正文；插入内置占位符；按收件人预览；导出整批 HTML / TXT / EML |
| 模板管理 | 内置欢迎、通知、活动推广模板；新增、复制、编辑、删除、设为常用、选择启动默认、导入和导出 |
| 收件人 | 批量粘贴邮箱、导入 JSON、增删收件人；设置称呼和专属变量 |
| 变量与匹配规则 | 表格编辑变量，支持文本和 JSON 类型；按邮箱匹配，并通过上移、下移调整覆盖顺序 |
| 连接设置 | 设置发件人、回复邮箱、API 令牌和超时时间 |
| 发送记录 | 实时查看各收件人的提交结果、Message-ID 和错误信息；导出 JSON 报告 |

SMTP 发送在后台线程执行，窗口继续响应。发送中锁定配置编辑，当前批次完成后可关闭窗口。修改内容后，旧预览会失效，需要重新生成。

## 模板管理

「模板管理」内置 **欢迎邮件、通知邮件、活动推广** 三个可编辑模板。可以从当前邮件新建模板，也可以新建空白模板；选中模板后可以修改名称、主题、HTML、纯文本、默认变量和匹配规则，还可以复制、删除或导出。

多个模板可以同时标为「常用」，会出现在「邮件内容」顶部，点击即可应用。可以另外指定一个「启动默认」模板：点击「新建邮件」时使用它；重新启动应用时优先恢复尚未完成的草稿。

模板包含主题、正文、全局变量和匹配规则。应用模板时保留当前发件人、回复邮箱和收件人，个人专属变量继续优先于模板默认值。修改已应用的邮件不会自动覆盖模板；需要保存这些修改时，选择「从当前邮件新建模板」或「用当前邮件更新」。

「导入配置为模板」可以导入已有活动配置，提取其中的模板内容。「导出模板」会生成 JSON 和配套正文文件，可在另一台电脑的模板管理中重新导入。

## 自动保存与软件更新

应用自动保存草稿（包括未完成的编辑）、收件人及个人变量、全局变量和规则、整个模板库、常用和启动默认选择、发件人和回复邮箱、超时、最近发送记录、当前页面及正文编辑/预览选择、尚未添加的邮箱输入、最近打开的配置路径。窗口大小和位置单独保存。

数据保存在系统用户目录，与安装位置和软件版本无关：

| 系统 | 设置目录 |
| --- | --- |
| macOS | `~/Library/Application Support/com.shrimpapi.email/` |
| Windows | `%APPDATA%\shrimpapi\email\config\` |
| Linux | `$XDG_CONFIG_HOME/email/`，未设置时为 `~/.config/email/` |

「连接设置」中的「设置与数据」会显示实际路径，并提供「打开数据目录」和「立即保存全部设置」。只替换应用或升级软件时，这些文件会保留，启动后自动恢复；不会用内置模板覆盖用户修改过的模板。

- `settings.json`：带格式版本号的设置和模板库。
- `settings.backup.json`：上一份有效设置，用于损坏恢复。
- `window.ron`：窗口布局信息。
- API 令牌：保存到 macOS 钥匙串、Windows 凭据管理器或 Linux Secret Service，凭据标识固定为 `com.shrimpapi.email`。

设置通过临时文件写入并原子替换，保存前保留上一份有效数据。旧格式会补全新增设置项；遇到更高版本的数据格式会停止读取和覆盖。主设置损坏时会尝试读取有效备份，并保留损坏原文件；失败会显示错误，不会静默重置。

「备份设置 / 恢复设置备份」可以手动迁移模板库、草稿和普通设置。API 令牌保留在本机系统凭据库中，迁移到新电脑后需重新填写。此处实现的是同一台电脑升级后的自动继承和手动备份迁移，没有接入云端账号同步。

## 配置导入与导出

点击「导出配置」，通过原生文件对话框选择位置。应用生成一个 `.json` 配置及同目录、同文件名前缀的 `.html` / `.txt` 正文模板。通过「打开配置」选择 JSON 文件即可导入，模板路径相对于配置文件解析。日常编辑会自动保存，不需要手动导出配置。

示例配置见 [examples/campaign.json](examples/campaign.json)，示例正文见 [examples/welcome.html](examples/welcome.html) 和 [examples/welcome.txt](examples/welcome.txt)。可以直接在应用内打开示例配置。

通过「导出全部预览」选择目录后，生成的 `manifest.json` 会列出收件人、主题和对应文件。每个收件人有独立的正文文件及可由邮件客户端打开的 `.eml` 邮件。

## 收件人和变量

「收件人」页面支持使用换行、英文逗号或分号批量添加邮箱，也可以通过「导入收件人 JSON」导入 [examples/recipients.json](examples/recipients.json) 这样的文件：

```json
[
  {
    "email": "alice@example.com",
    "name": "Alice",
    "variables": { "plan": "专业版", "coupon": "ALICE30" }
  },
  "bob@example.net"
]
```

导入会追加到当前收件人列表。重复邮箱忽略 ASCII 大小写，发送时保留首次出现的记录及其变量。

变量覆盖优先级，从低到高为：默认称呼 → 全局变量 → 按顺序命中的匹配规则 → 收件人专属变量 → 收件人显式称呼。同名变量由后者覆盖，嵌套对象按顶层键整体覆盖。

匹配规则针对完整邮箱做不区分大小写的 glob 匹配：

| 匹配模式 | 用途 |
| --- | --- |
| `alice@example.com` | 精确匹配邮箱 |
| `*@example.com` | 匹配某个域名的收件人 |
| `sales+*@example.com` | 匹配带特定标签的邮箱 |

所有命中规则按界面中的顺序应用，后面的规则覆盖前面的同名变量。

内置占位符：

| 占位符 | 含义 |
| --- | --- |
| `{{email}}` | 完整收件邮箱，例如 `alice@example.com` |
| `{{local}}` | 邮箱 `@` 前的部分，例如 `alice` |
| `{{domain}}` | 邮箱域名，例如 `example.com` |
| `{{name}}` | 收件人称呼，默认使用显示名或邮箱 `local` 部分 |

`email`、`local`、`domain` 保留给实际收件地址，不能由变量覆盖。变量类型选「文本」时按字符串处理；选「JSON」时可输入布尔值、数字、对象或数组。例如关闭条件应使用 JSON 值 `false`，文本 `"false"` 仍是非空字符串。

## 模板和预览

主题、HTML 和纯文本使用 Handlebars，支持 `{{variable}}`、嵌套路径 `{{account.id}}`、条件 `{{#if flag}}…{{/if}}`、循环 `{{#each items}}…{{/each}}` 等语法。

HTML 中的 `{{variable}}` 自动转义，可使用 `{{{trusted_html}}}` 插入可信 HTML 片段。主题和纯文本保留原始文本。HTML 和纯文本同时启用时生成 `multipart/alternative` 邮件。

应用内提供纯文本与 HTML 源码预览，HTML 实际排版通过默认浏览器查看。使用 eframe 的系统字体回退显示中文，系统需要安装中文字体。

缺失变量、无效模板或邮箱、空主题等问题会在整批预检时显示，不会开始提交。主题和姓名不能包含换行或控制字符，MIME 编码后超过 25 MiB 的邮件会被拒绝。

## 发送结果

依据 [Cloudflare SMTP 文档](https://developers.cloudflare.com/email-service/api/send-emails/smtp/)，应用固定使用 `smtp.mx.cloudflare.net:465`、隐式 TLS 和 `AUTH PLAIN`，用户名为 `api_token`。每封邮件只包含一个收件人，使用独立 SMTP 会话。

发送记录和 JSON 报告包含三类结果：

- `accepted`：SMTP 接受，包含服务端返回的 Message-ID，可用于查询 Cloudflare 日志。
- `accepted_without_message_id`：SMTP 接受但没有返回 Message-ID，需到 Cloudflare 核对抑制和投递日志。
- `failed`：提交出错，包含错误信息；其余收件人继续处理。

SMTP 接受后的最终投递结果以 Cloudflare Email Sending 日志为准。客户端不自动重试；`DATA` 后发生的网络超时可能意味着服务端已经接受，重新发送前应先核对日志。

## 开发和打包

需要 Rust / Cargo。开发时直接启动窗口：

```sh
cargo run --locked
```

macOS 打包为可双击打开的应用：

```sh
sh scripts/bundle-macos.sh
# 输出：target/ShrimpAPI Email.app
# 开发构建：sh scripts/bundle-macos.sh --debug
```

发布通用安装包前，安装两个目标并运行打包脚本：

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
sh scripts/release-macos.sh
# 输出：dist/ShrimpAPI.Email-<version>-macos-universal.zip 和 dist/SHA256SUMS
```

Windows 构建使用 GUI 子系统，不附带控制台窗口。应用启动错误通过原生对话框显示。Linux 的持久凭据存储需要可用的 Secret Service（例如 GNOME Keyring 或 KWallet）及 D-Bus 开发库。

检查项目：

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

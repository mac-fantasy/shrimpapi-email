# 默认邮件模板

应用内置三类模板，均提供可独立编辑的 HTML 和纯文本版本，主题和变量默认值由 `src/templates.rs` 定义。

| 模板 | 正文文件 | 主要变量 |
| --- | --- | --- |
| 新用户注册 | `new-user-registration.html` / `.txt` | `welcome_message`、`console_url`、`docs_url` |
| 模型上新 | `model-launch.html` / `.txt` | `model_name`、`provider`、`model_id`、`model_summary`、`context_window`、`pricing`、`console_url`、`model_docs_url` |
| 服务通知 | `service-notice.html` / `.txt` | `notice_title`、`notice_type`、`start_time`、`end_time`、`affected_services`、`impact`、`action_required`、`status_url` |

通用变量为 `product` 和 `support_email`，收件人称呼和邮箱使用内置的 `name` / `email`。模型名称、调用标识、时间窗口和链接都是可编辑的示例值，发送前应填写实际内容并生成预览。

默认启动模板为「新用户注册」。HTML 使用适合邮件客户端的表格布局和内联样式；三个模板不依赖外部图片。

设置格式从版本 1 升级到版本 2 时，旧模板库会整体替换为上述三类模板，同时保留当前草稿、收件人、发件身份和其他设置。升级前的设置另存为 `settings.v1.backup.json`，之后的普通自动保存不会覆盖此备份。此替换只执行一次，升级后创建和修改的模板会正常保存。

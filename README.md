# Understand Everything

把知识库渲染成可缩放思维导图的桌面学习工具（Rust + Makepad）。

## 运行

```sh
cargo run
```

## 数据目录

用户数据（卡片、地图、资料、设置、RAG 缓存、模型）存放在平台数据目录，可用环境变量 `UE_DATA_DIR` 覆盖：

| 平台 | 路径 |
|---|---|
| Linux | `~/.local/share/understand-everything/` |
| macOS | `~/Library/Application Support/understand-everything/` |
| Windows | `%LOCALAPPDATA%\understand-everything\` |

旧版放在程序目录旁的数据会在首次启动时自动迁移过去。

## 更新

About 面板点击「检查更新」即可从 GitHub Releases 自动下载并替换自身。发布约定：tag = `v{版本号}`（与 `Cargo.toml` 一致），asset = `understand-everything-{linux|macos|windows}-{x86_64|aarch64}`。

## 许可证

[GPL-3.0](LICENSE)

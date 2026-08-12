# Understand Everything

基于「渐构」学习观（参考 [modevol.com](https://www.modevol.com/)《渐构：世界模型》）设计的桌面学习工具：把知识建构为判别模型（概念卡）与联结模型（知识卡），在可缩放思维导图上展开——路线规划先判别后联结、按模型类型出题测验、明确输入输出空间、已见/未见掌握状态标注。Rust + Makepad。

[English](README.md)

![Understand Everything](assets/UE_ScreenShot.png)

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

# Desktop Alchemy

简体中文 · [English](README.en.md)

Desktop Alchemy 是基于 Rust、Tauri 2 和 Vue 3 的 Windows x64 多设备客户端，内置 M18 与 Microsoft Audio Dock 适配。M18 通过官方 C Device SDK（`transport.dll`）在客户端进程内控制，无需额外 SDK 程序。

- M18 的 18 键输入、静态图片与按键 GIF、临时背景、屏幕亮度及软件 LED 灯效。
- 页面编辑、自动保存、场景自动化、2D／3D 配置预览和按键导航。
- 图片素材与应用预设资源库，支持程序、快捷方式、Windows 已注册应用、URL 和自定义协议。
- Audio Dock 音频设置、五频段均衡器及只读 Teams 键输入映射。
- Windows 托盘、关闭行为设置、单实例窗口唤醒和设置备份。
- 中文／英语界面，默认跟随系统，支持手动选择。
- 语言无关的进程插件协议、Rust 示例和内置计算机性能插件。
- 可选的本机 MCP HTTP 接口，供 AI Agent 管理素材、应用预设、页面、场景、设备设置、插件与备份，并启动应用。

[文档索引（英文）](docs/README.md) · [客户端使用](docs/CLIENT.md) · [开发与打包](docs/DEVELOPMENT.md) · [插件协议](docs/PLUGIN-PROTOCOL.md) · [MCP 接口](docs/MCP.md) · [开发协作指南](AGENTS.md)

## 开发启动

```powershell
pnpm install --frozen-lockfile
pnpm sdk:fetch
pnpm plugins:build
pnpm desktop
```

`pnpm desktop` 启动原生客户端；`pnpm dev` 仅提供浏览器预览。工具链、按需测试及安装包生成见[开发指南](docs/DEVELOPMENT.md)。安装版用户不需要开发工具链。

## 设备与扩展

每台设备独立运行，按自身能力提供页面、输入、灯效、临时覆盖和场景，切换界面只改变编辑对象。设备适配器通过独立 Rust crate 和注册声明接入；开发模拟器提供显示键、旋钮和模拟量输入，默认发行版不启用。参见[设备模块](docs/DEVICES.md)与[新设备接入](docs/NEW-DEVICE.md)。

Audio Dock 支持提示音、音量、按下即可发言、麦克风降噪和均衡器。Teams 映射默认关闭且没有动作；Windows 原有行为保留。详见 [Audio Dock](docs/hardware/microsoft-audio-dock/README.md) 和[应用与启动](docs/APPLICATIONS.md)。相关 MIT 声明已列入 [`licenses/README.md`](licenses/README.md)，并随安装包保留。

## 数据与接口

正式数据保存于应用数据目录的 `platform-v1` 子目录；旧数据目录不读取或自动迁移。工作区与备份采用独立标识和版本 1，旧格式不导入。设备操作统一经各自 Core 排队；MCP 管理接口支持持久配置修改；原有设备控制工具和插件保持临时控制语义。MCP 不提供设备开机图写入。

# 鼠标连点器 (Derek Mouse)

> **声明：本项目完全由 GitHub Copilot 开发，如有问题概不负责。**

一款轻量级 Windows 桌面工具，提供 **鼠标自动连点** 和 **键鼠操作录制与回放** 两大核心功能。基于 Tauri 2 构建，体积小巧、资源占用低。

## 功能特性

### 🖱️ 鼠标连点器

- **两种连点方式**：热键切换（按一次开始/再按停止）或按住鼠标连点（按住触发、松开停止）
- **点击类型**：支持左键 / 中键 / 右键
- **自定义间隔**：最低 0.01 秒，步进 0.05 秒
- **窗口内保护**：按住模式下鼠标在应用窗口内时不触发连点

### 🎬 键鼠录制与回放

- **完整录制**：捕获键盘按键、鼠标点击、移动和滚轮事件，记录精确时间间隔
- **速度调节**：0.5x ~ 10x 回放速度
- **循环回放**：支持单次或循环模式
- **多方案管理**：支持创建多个录制方案，可重命名、删除
- **独立录制热键**：默认 F9，与全局热键互不冲突
- **数据持久化**：录制数据自动保存到本地

### ⌨️ 全局热键

- **可配置热键**：F1-F12、A-Z、0-9、Space、Enter、Esc
- **修饰键组合**：字母和数字键支持 Ctrl / Alt 修饰
- **冲突检测**：自动校验全局热键与录制热键不重复
- **智能路由**：热键根据当前 Tab 自动控制连点器或录制器

### 🖥️ 系统与界面

- **系统托盘**：托盘图标实时显示运行状态（已停止 / 运行中 / 录制中），左键点击显示窗口
- **主题切换**：浅色 / 深色 / 跟随系统
- **窗口置顶**：一键切换窗口始终置顶
- **关闭行为**：可选隐藏到托盘或直接退出
- **单实例运行**：防止重复启动，自动聚焦已有窗口
- **系统通知**：热键触发时弹出状态通知

## 技术栈

| 层级 | 技术 |
|------|------|
| 应用框架 | Tauri 2 |
| 前端 | Vue 3 + TypeScript + Vite |
| UI 组件库 | Element Plus |
| 后端 | Rust |
| 全局输入 | rdev（键鼠事件监听与模拟） |
| 数据存储 | JSON 文件（`~/.derek-mouse/`） |

## 开发环境

### 前置要求

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)

### 推荐 IDE

[VS Code](https://code.visualstudio.com/) + 以下扩展：
- [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar)
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

### 安装与运行

```bash
# 安装依赖
npm install

# 启动开发模式
npm run tauri dev

# 构建安装包 (NSIS)
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/nsis/`。

## 项目结构

```
src/                          # Vue 前端
├── App.vue                   # 主布局（Tab 切换）
├── components/
│   ├── TitleBar.vue          # 自定义标题栏 + 设置面板
│   └── GlobalHotkeyBar.vue   # 全局热键 + 速度控制
├── features/
│   ├── clicker/ClickerPanel.vue   # 连点器面板
│   └── recorder/RecorderPanel.vue # 录制器面板
└── types.ts                  # 类型定义

src-tauri/src/                # Rust 后端
├── lib.rs                    # Tauri 命令注册 + 全局事件监听
├── clicker.rs                # 连点器逻辑
├── recorder.rs               # 录制与回放逻辑
├── input.rs                  # 输入事件处理
└── tray.rs                   # 系统托盘
```

## 许可证

MIT

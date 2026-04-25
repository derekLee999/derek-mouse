# 鼠标连点器 (Derek Mouse)

一款面向 Windows 的 Tauri 桌面工具，围绕三类自动化能力构建：

- 鼠标连点
- 键鼠录制与回放
- 鼠标宏编辑与执行


## 功能概览

### 1. 鼠标连点

- 支持两种模式：
  - 热键切换连点
  - 按住鼠标连点
- 支持左键 / 中键 / 右键点击
- 点击间隔最小 `0.01` 秒
- 热键切换模式支持点击次数限制，`0` 表示不限制
- 按住模式可选择左键或右键作为触发键
- 按住模式下，鼠标位于应用窗口内部时不会触发连点
- 支持热键触发时自动隐藏窗口，停止时自动重新显示窗口

### 2. 键鼠录制与回放

- 独立录制热键，默认是 `F9`
- 当前处于“键鼠录制”Tab 时：
  - 录制热键用于开始 / 停止录制
  - 顶部全局热键用于回放当前选中的录制方案
- 录制内容包括：
  - 键盘按下 / 释放
  - 鼠标按下 / 释放
  - 鼠标移动
  - 滚轮事件
- 录制结束后会自动做事件清洗：
  - 去掉细碎抖动
  - 合并连续鼠标移动
  - 规范化延迟
- 支持多录制方案管理：
  - 选择
  - 重命名
  - 删除
  - 调整回放速度
  - 切换单次 / 循环
- 支持录制编辑窗口：
  - 查看事件列表
  - 多选删除
  - Shift 范围选择
  - 跳转关键操作
  - 保存为新增方案或覆盖原方案

### 3. 鼠标宏

- 当前处于“鼠标宏”Tab 时，顶部全局热键用于执行当前选中的宏方案
- 支持新建 / 编辑 / 删除宏方案
- 支持调整回放速度和单次 / 循环模式
- 宏编辑器支持以下事件类型：
  - 鼠标单击 / 双击 / 按下 / 释放
  - 鼠标移动到指定坐标
  - 键盘点击 / 按下 / 释放
  - 延迟等待
  - 找图
  - 找色
  - 文字识别
- 鼠标移动事件支持追加后续动作：
  - 无
  - 左键点击
  - 左键双击
- 鼠标宏编辑器支持：
  - 拾取屏幕坐标
  - 框选搜索区域
  - 吸取屏幕颜色
  - 截取模板图
  - 本地图片作为模板
  - 拖拽调整事件顺序
  - 单步修改 / 删除
  - 对找图、找色、文字识别做即时测试

### 4. 找图 / 找色 / OCR

- 找图：
  - 以屏幕区域截图或本地图片作为模板
  - 支持匹配阈值
  - 支持模板缩放
  - 支持找到后点击 / 双击 / 移动
  - 支持“直到找到为止”
  - 支持偏移量
- 找色：
  - 在指定区域内搜索颜色
  - 支持阈值
  - 支持找到后点击 / 双击 / 移动
  - 支持“直到找到为止”
  - 支持偏移量
- 文字识别：
  - 依赖外部 OCR 引擎 `RapidOCR-json`
  - 应用内可直接下载安装
  - 支持在指定区域内找文字
  - 支持找到后点击 / 双击 / 移动
  - 支持“直到找到为止”
  - 支持偏移量

### 5. 桌面行为

- 自定义无边框窗口
- 浅色 / 深色 / 跟随系统
- 窗口置顶
- 关闭按钮可配置为：
  - 隐藏窗口
  - 退出程序
- 系统托盘常驻
- 托盘图标会随状态变化：
  - 已停止
  - 运行中
  - 录制中
  - 录制待命
- 托盘左键显示主窗口
- 托盘菜单提供“退出程序”
- 全局热键触发时支持系统通知
- 单实例运行，重复启动会自动聚焦已有窗口

## 热键规则

- 可选按键范围：
  - `F1-F12`
  - `A-Z`
  - `0-9`
  - `Space`
  - `Enter`
  - `Esc`
- 字母和数字键必须与 `Ctrl` 或 `Alt` 组合
- `F1-F12` / `Space` / `Enter` / `Esc` 不能再叠加 `Ctrl` 或 `Alt`
- 录制热键不能与全局回放热键相同
- 顶部全局热键会根据当前激活的 Tab 路由到不同功能：
  - `鼠标连点` Tab：启动 / 停止连点
  - `键鼠录制` Tab：启动 / 停止当前选中录制的回放
  - `鼠标宏` Tab：启动 / 停止当前选中宏的执行

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | Tauri 2 |
| 前端 | Vue 3 + TypeScript + Vite |
| UI | Element Plus |
| 后端 | Rust |
| Windows 输入监听 / 模拟 | `windows` crate + Win32 Hook + `SendInput` |
| 屏幕捕获 | `xcap` |
| 找图 | OpenCV C++ Shim + Rust FFI |
| OCR | RapidOCR-json（按需下载安装） |
| 数据存储 | JSON 文件（`%USERPROFILE%\\.derek-mouse\\`） |

## 平台与权限

### 仅支持 Windows

Rust 端直接使用了 Win32 Hook、`SendInput`、屏幕像素读取等 Windows API，当前不是跨平台实现。

### 需要管理员权限

`src-tauri/app.manifest.xml` 中声明了：

- `requestedExecutionLevel = requireAdministrator`

也就是说，应用运行时会以管理员权限启动。这和全局输入监听、输入模拟、屏幕取色等能力有关。

## 数据存储

应用数据默认保存在：

```text
%USERPROFILE%\.derek-mouse\
```

当前代码里的主要文件 / 目录如下：

- `config.json`
  - 连点配置
  - 录制热键
  - “停止后显示窗口”
  - “热键触发后自动隐藏”
- `recordings.json`
  - 录制方案与事件数据
- `macros.json`
  - 鼠标宏方案与事件数据
- `ocr-engine\`
  - RapidOCR-json 引擎文件

## 开发环境

### 前置要求

- Windows 10 / 11
- Node.js LTS
- Rust stable
- Tauri 2 所需系统依赖
- Visual Studio C++ Build Tools

### OpenCV 额外要求

当前项目的 `src-tauri/build.rs` 里写死了 OpenCV 的头文件和库路径：

```text
C:\tools\opencv\opencv\build\include
C:\tools\opencv\opencv\build\x64\vc16\lib
```

也就是说，本地开发环境如果要成功编译 Rust 端，需要满足以下任一条件：

1. 按这个目录安装 / 解压 OpenCV 4.9
2. 自己修改 `src-tauri/build.rs` 中的路径

运行时项目会尝试把 `opencv_world490.dll` 复制到目标目录；仓库根目录下也已经放了一份 DLL 用于打包 / 开发时加载。

## 安装与运行

### 安装依赖

```bash
npm install
```

### 仅启动前端

```bash
npm run dev
```

这只适合调样式和普通前端逻辑。全局热键、输入监听、托盘、坐标拾取、找图、找色、OCR 安装等功能都依赖 Tauri / Rust 端，在纯浏览器里不能完整工作。

### 启动桌面开发模式

```bash
npm run tauri -- dev
```

### 构建前端

```bash
npm run build
```

### 构建安装包

```bash
npm run tauri:build
```

当前 Tauri 配置使用 `NSIS` 作为打包目标，产物一般位于：

```text
src-tauri\target\release\bundle\nsis\
```

## 使用说明

### 鼠标连点

1. 打开 `鼠标连点` Tab
2. 设置全局热键、点击方式、点击按钮和间隔
3. 如果是热键切换模式，按一次热键开始，再按一次停止
4. 如果是按住模式，按住所选触发键开始，松开停止

### 键鼠录制

1. 打开 `键鼠录制` Tab
2. 设置录制热键
3. 按录制热键开始录制，再按一次停止
4. 选择某个录制方案后，按顶部全局热键回放
5. 右键方案可打开编辑窗口或删除

### 鼠标宏

1. 打开 `鼠标宏` Tab
2. 新建或编辑宏方案
3. 在编辑器里逐条添加事件
4. 选择一个宏方案
5. 按顶部全局热键执行 / 停止当前宏

## 项目结构

```text
src/
├── App.vue                              # 主窗口，包含 3 个功能 Tab 和子窗口入口
├── main.ts                              # Vue / Element Plus / 主题初始化
├── types.ts                             # 前端共享类型
├── components/
│   ├── TitleBar.vue                     # 自定义标题栏、主题、关闭行为、置顶、热键选项
│   └── GlobalHotkeyBar.vue              # 顶部全局热键配置
└── features/
    ├── clicker/
    │   └── ClickerPanel.vue             # 连点器面板
    ├── recorder/
    │   ├── RecorderPanel.vue            # 录制方案列表与录制热键配置
    │   └── RecordingEditorWindow.vue    # 录制事件编辑窗口
    └── mouse-macro/
        ├── MouseMacroPanel.vue          # 宏方案列表
        ├── MouseMacroEditorWindow.vue   # 宏编辑器
        └── CoordinatePickerWindow.vue   # 屏幕坐标 / 区域 / 颜色拾取窗口

src-tauri/
├── Cargo.toml                           # Rust 依赖
├── tauri.conf.json                      # Tauri 窗口 / 打包配置
├── app.manifest.xml                     # Windows 管理员权限声明
├── build.rs                             # Tauri 构建 + OpenCV C++ Shim 编译与链接
├── capabilities/default.json            # Tauri 窗口能力声明
└── src/
    ├── lib.rs                           # Tauri 命令注册、全局监听、状态协调
    ├── input.rs                         # 全局输入监听与输入模拟
    ├── clicker.rs                       # 连点器运行时
    ├── recorder.rs                      # 录制 / 回放 / 清洗 / 落盘
    ├── mouse_macro.rs                   # 宏执行、找图、找色、OCR、截图
    ├── ocr_engine.rs                    # RapidOCR-json 下载、安装与调用
    ├── tray.rs                          # 托盘和通知
    ├── main.rs                          # 程序入口
    └── opencv_shim.cpp                  # OpenCV 模板匹配桥接
```

## 当前实现上的注意点

- 这是一个真实依赖 Windows API 的桌面工具，不是普通网页应用。
- 纯前端预览不等于功能可用，关键能力必须通过 Tauri 运行。
- OpenCV 构建路径目前是硬编码的，换机器开发前需要先处理这部分。
- OCR 不是内置模型，首次使用文字识别时需要先在应用里下载安装引擎。
- 录制和鼠标宏的数据都直接落在用户目录，没有接数据库。

## 许可证

Apache License 2.0

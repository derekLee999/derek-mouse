# 发版流程

本文档约定了这个项目的固定发版流程，目标是：

- 发布包版本号一致
- GitHub Release 的 `tag` 与应用版本一致
- 自动更新始终使用同一把 updater 签名私钥
- 你自己和同事都能按同一套步骤发版

## 核心规则

1. GitHub Release 的 `tag` 必须使用 `vX.Y.Z` 格式。
2. 应用内部版本号使用 `X.Y.Z`，不带 `v`。
3. 自动更新必须始终使用同一把 updater 私钥签名。
4. 私钥不能提交到仓库。
5. 发版时必须上传 updater 产物，不然应用只能检测到新版本，不能直接安装。

## 当前项目中的关键位置

- updater 公钥：
  - `src-tauri/updater.pubkey`
- 应用版本号：
  - `package.json`
  - `src-tauri/Cargo.toml`
  - `src-tauri/tauri.conf.json`
- 本地发版脚本：
  - `scripts/release.ps1`

## 私钥管理

### 推荐存放位置

当前项目默认约定私钥路径为：

```text
C:\Users\<你的用户名>\.tauri\derek-mouse-updater.key
```

例如当前机器默认是：

```text
C:\Users\17210\.tauri\derek-mouse-updater.key
```

### 换电脑 / 同事电脑

可以继续发版，但必须满足：

- 使用同一把私钥
- 私钥通过安全方式单独传递
- 不要把私钥放进仓库

如果换了新私钥，旧版本用户将无法通过自动更新升级。

## 一次性准备

### 1. 安装依赖

```bash
npm install
```

### 2. 准备 updater 私钥

如果还没有正式对外发布过带自动更新的版本，可以生成一把新的私钥：

```powershell
npm run tauri signer generate -- -w $env:USERPROFILE\.tauri\derek-mouse-updater.key
```

更推荐生成时带密码。

如果已经发布过，则不要重新生成，继续复用原来的私钥。

### 3. 确认公钥已在仓库内

公钥文件：

```text
src-tauri\updater.pubkey
```

如果你重新生成了新的正式私钥，必须同步更新这个公钥文件，再发布新版本。

## 标准发版流程

### 方案 A：你自己本地发版

1. 先确保当前改动都已完成，并且能正常构建。
2. 决定新版本号，例如 `0.1.4`。
3. 运行发版脚本：

```powershell
.\scripts\release.ps1 -Version 0.1.4
```

如果私钥不在默认路径，显式指定：

```powershell
.\scripts\release.ps1 -Version 0.1.4 -SigningKeyPath "D:\secret\derek-mouse-updater.key"
```

如果私钥有密码，先在当前 PowerShell 会话设置：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD="你的密码"
```

脚本会做这些事：

- 同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 的版本号
- 设置 `TAURI_SIGNING_PRIVATE_KEY_PATH`
- 执行 `npm run tauri:build`
- 输出建议上传到 GitHub Release 的文件路径

4. 检查改动并提交代码。
5. 创建并推送 tag：

```powershell
git tag v0.1.4
git push origin main
git push origin v0.1.4
```

6. 在 GitHub 创建 `v0.1.4` Release。
7. 上传脚本输出的产物。

## 必须上传的 Release 产物

当前项目只打 `NSIS`，所以至少要上传：

- 安装包 `.exe`
- 对应的 `.exe.sig`
- `latest.json`

这些文件会由 Tauri 在构建时生成并签名。脚本结束时会把路径列出来。

## 版本命名规则

示例：

- Git tag：`v0.1.4`
- 应用版本：`0.1.4`

自动更新比较时：

- 当前应用读取本地版本 `0.1.3`
- 请求 GitHub 最新 Release
- 如果最新 `tag_name` 是 `v0.1.4`
- 就会判定有新版本

## 方案 B：同事电脑发版

同事电脑发版时，流程和你本地完全一致，唯一要求是：

- 同事必须拿到同一把私钥

建议做法：

1. 把私钥通过密码管理器、加密压缩包或企业网盘安全传递给同事。
2. 同事保存到自己的本地路径，例如：

```text
C:\Users\同事\.tauri\derek-mouse-updater.key
```

3. 同事执行：

```powershell
.\scripts\release.ps1 -Version 0.1.4
```

或者显式指定私钥路径：

```powershell
.\scripts\release.ps1 -Version 0.1.4 -SigningKeyPath "C:\Users\同事\.tauri\derek-mouse-updater.key"
```

## 方案 C：以后迁移到 CI

如果以后想彻底固定流程，最稳的是把发版迁移到 CI，例如 GitHub Actions：

- 仓库里只保留公钥
- 私钥放到 GitHub Secrets
- 由 CI 统一构建、签名、上传 Release

这样好处是：

- 不依赖某一台电脑
- 不依赖某一个同事的本地环境
- 不容易把版本号、tag、签名搞乱

## 常见错误

### 1. 换了一把私钥

后果：

- 旧版本用户无法自动更新

### 2. 只上传安装包，没有上传 `.sig` 或 `latest.json`

后果：

- 应用能提示新版本
- 但无法直接安装更新

### 3. Release tag 和应用版本号不一致

例如：

- tag 是 `v0.1.5`
- `tauri.conf.json` 仍然是 `0.1.4`

后果：

- 更新判断和发版记录会混乱

### 4. 私钥进了仓库

后果：

- 安全性失效
- 必须立刻废弃该私钥并重新规划更新链路

## 推荐执行方式

当前最推荐的固定流程是：

1. 你维护唯一正式 updater 私钥
2. 发版前运行 `.\scripts\release.ps1 -Version x.y.z`
3. 提交代码
4. 打 `vX.Y.Z` tag
5. 创建 GitHub Release
6. 上传脚本列出的产物

如果以后发版频率上来了，再迁移到 CI。

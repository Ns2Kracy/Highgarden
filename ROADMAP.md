# Highgarden — 开发路线图

鹰角网络游戏第三方启动器，支持明日方舟与明日方舟：终末地（仅 Windows）。

---

## 游戏 API 参数

| 游戏 | appcode | CDN 域名 | 当前版本 | 分包数 | 全量大小 |
|------|---------|---------|---------|-------|---------|
| 明日方舟 | `GzD1CpaWgmSq1wew` | `ak.hycdn.cn` | 70.0.1 | 18 | ~38.9 GB |
| 终末地 | `6LL0KJuqHBVz33WK` | `beyond.hycdn.cn` | 1.0.14 | 36 | ~78.2 GB |

**版本检查 API：**
```
GET https://launcher.hypergryph.com/api/game/get_latest
    ?appcode={appcode}&channel=1&sub_channel=1&platform=Windows
```
返回：版本号、各分包 URL（含 auth_key 签名）、MD5、大小。

**客户端版本 API：**
- 明日方舟：`https://ak-conf.hypergryph.com/config/prod/official/Windows/version`
- 终末地：`https://beyond-conf.hypergryph.com/config/prod/official/Windows/version`

---

## Phase 1 — 基础框架 ✅

### 依赖与工程配置
- [x] 前端：TailwindCSS v4、lucide-svelte
- [x] Tauri 插件：fs、dialog、store、shell、process
- [x] Rust：reqwest、tokio、sha2、md5、futures-util、uuid、anyhow
- [x] 无边框窗口（1280×800，可拖动标题栏）
- [x] 自定义窗口控件（最小化 / 最大化 / 关闭）

### UI 框架
- [x] Arknights 风格暗色主题（背景色、金色 accent、monospace 字体）
- [x] 侧边栏导航（游戏库 / 下载管理 / 寻访分析 / 设置）
- [x] 错误 Toast 提示组件

### 页面骨架
- [x] 游戏库主页（明日方舟 / 终末地 Tab 切换）
- [x] 下载管理页
- [x] 寻访分析页（占位）
- [x] 设置页（下载路径、并发数、代理地址）

---

## Phase 2 — 核心功能

### 游戏管理 ✅
- [x] 本地游戏路径选择与验证
- [x] 启动时多 exe 候选检测
- [x] 游戏启动（`tokio::process::Command`）
- [x] 版本号获取（明日方舟 / 终末地均已接入）
- [x] 安装路径持久化（跨重启保存，tauri-plugin-store）
- [x] 本地版本与线上版本对比，自动检测更新

### 下载引擎 ✅
- [x] 多线程分片下载（Range 请求，默认 8 分片）
- [x] 断点续传（记录每片已下载字节数）
- [x] MD5 文件校验（每个分包独立校验）
- [x] Tauri Event 实时推送进度到前端（`download:progress`）
- [x] 暂停 / 取消任务

### Hypergryph 下载接入 ✅
- [x] `fetch_game_manifest` — 调用官方 API 获取分包列表
- [x] `start_game_install` — 为每个分包创建下载任务并并发下载
- [x] 终末地 auth_key 签名 URL 支持（直接使用 API 返回的 URL）
- [x] 下载管理页实时聚合进度（总进度条 + 分包列表）
- [x] 增量更新（patch 包，非全量重下）
- [x] 下载完成后 ZIP 解压安装

---

## Phase 3 — 寻访分析

- [ ] **数据获取方式调研**（游戏内嵌 H5 的 API URL）
- [ ] 寻访记录拉取与本地存储
- [ ] 统计分析
  - [ ] 六星出货率 / 当前 pity 计数
  - [ ] 各期卡池分析
  - [ ] 历史趋势图
- [ ] 导出
  - [ ] Excel（`.xlsx`）
  - [ ] CSV
  - [ ] JSON

---

## Phase 4 — 完善（待定）

- [ ] 设置持久化（tauri-plugin-store）
- [ ] 游戏公告 / 新闻展示
- [ ] 自动更新（Highgarden 自身更新）
- [ ] 多语言（中文 / 英文）
- [ ] 系统托盘

---

## 当前阻塞项

| 项目 | 原因 | 优先级 |
|------|------|--------|
| 寻访 API | 需分析游戏内嵌 H5 页面的请求方式 | 中 |

# Interview Meeting MVP

Tauri v2 单窗口分栏 MVP：左侧业务页 + 右侧内嵌腾讯会议网页入会地址。

```
┌─────────── 同一个系统窗口 ───────────┐
│  业务区 (biz)  │  腾讯会议 Web (meet) │
└─────────────────────────────────────┘
```

## 本地开发（需要 Rust + Node）

```bash
npm install
npm run tauri dev
```

## 无本地编译环境时

推送到 GitHub 后，Actions 会自动在 `windows-latest` 上编译：

1. 打开仓库 **Actions** → **Build Windows MVP**
2. 等待绿色勾
3. 下载 Artifact：`interview-meeting-windows-nsis`（安装包 / exe）

也可在 Actions 页手动 **Run workflow**。

## 使用注意

- 右侧加载 `https://meeting.tencent.com/p/{meeting_id}`
- 会拦截 `wemeet://`，尽量避免拉起独立客户端
- 若页面仍提示下载客户端，请点「在浏览器中入会」
- 多 WebView API 使用 Tauri `unstable` feature，仅作验证方案

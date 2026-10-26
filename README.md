# Interview Meeting MVP (双窗口方案)

- **主窗口**：加载远程入会页  
  `https://ms.symsun.com/interview3s-web/views/interview/meeting/meeting_join.html?token=...`
- **监考守卫窗口**：前台白名单（本应用 + 腾讯会议）、切屏计数、可选强拉回前台、与腾讯会议分屏

音视频使用本机腾讯会议客户端；本应用不再内嵌会议网页。

## 使用

1. 打开应用后会出现「入会」主窗 + 「监考守卫」小窗  
2. 在入会页完成入会（可唤起本机腾讯会议）  
3. 进入会议后，在守卫窗点「与腾讯会议分屏」  
4. 守卫默认开启：前台若不在白名单则记切屏，并可强拉回业务窗  

## 构建

GitHub Actions：`Build Windows MVP`，下载 Artifact `interview-meeting-windows-nsis`。

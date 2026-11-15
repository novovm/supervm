# Codex 引擎与同机隔离验收

日期：2026-11-14。源码只在本 NOVOVM 仓库；运行数据在设备私有目录，不是另一份源码项目。

## A/B 职责与调用方向

A 安装 NOVOVM L3、ASAS 和官方 Codex/Claude Code，**不依赖 ACODE**。
B 安装 ACODE，保留项目、VS Code 和本机执行工具。

正式方向：B ACODE 指令 → A ASAS → A Codex；Codex 工具请求 → ASAS → B LocalWorker
→ B 执行回执 → ASAS → Codex 继续；最终结果/流式输出经 ASAS 返回 B。

当前已验的是上述真实组件的同设备隔离运行：B 的 `tests/support/asas-local-probe.ts`
通过原生 `novovm-asas-local` 私有管道启动 A 宿主。这个驱动仅用于验收，不是 B 的正式远程连接，
不随产品作为“B 启动 A”的实现。正式 B/A 路由仍须接入既有 NOVONET。

Codex 自己拥有 Agent Loop；ASAS 和 ACODE 只承接身份/授权、消息及工具回执，不解析自然语言伪造调用。
本地运行的是引擎，不是 GPT 权重；经授权的任务内容仍会发往 OpenAI。
目前走同设备继承的私有管道，无公网监听。**不是 NOVONET 跨设备路由已交付**。

## 明确配置与登录

```bash
cd plugins/asas/service
npm ci --ignore-scripts
npm run build
node dist/codex/setup.js --directory /private/asas-local \
  --executable /absolute/official/codex --login-home /existing/codex/home \
  --model MODEL_FROM_NATIVE_CATALOG --project-id B_PROJECT_SHA256
```

这个显式操作只引用所选设备的已有私有 `auth.json`，不读出/复制令牌，不接管当前聊天。
ASAS 使用独立配置、模型目录和原生会话。官方登录刷新仍由官方引擎处理。
如果文件权限不安全、无法建立文件链接、型号不在实际目录、账号不可用或配置已存在，则拒绝，不自动打开登录页、不换型号。
文件登录以外的凭据方式、其它操作系统的实际链接/保管行为须单独验收。

配置生成 `service.json` 与原生宿主 `host.json`；不要提交它们。
`executableSha256` 固定所选运行程序；程序更新后须明确刷新配置，而不是悄悄更换运行时。
B 在自己的设备上计算规范项目路径的 SHA-256，将该不透明标识用于授权；A 不解析或访问 B 的路径。
`--project-id` 可重复指定，不接受 B 目录作为 A 的初始化参数。

## 权限与会话

关闭 A 侧 Shell、补丁、图片、Web、外部 MCP、插件、应用和子 Agent；暴露五个 `acode_*` 动态工具。
保留原生 Codex 提示主体，附加远程项目职责说明。项目 cwd 是 B 的目录，A 的 cwd 仅为隔离空目录。
工具按 peer/project/task/call/turn 绑定；写入仍需 B 批准；计划模式拒绝写补丁和通用 IDE 执行。
任务结束和引擎排空后才返回可继续终态，同一 ASAS Session 恢复自己的原生 Codex 线程。
通信中断不自动重跑；超时/取消和未知副作用须核查原任务。当前单任务有 180 秒执行上限和有界文本输出。
原生同机入口按顺序转发，事件等待有 1 秒上限，避免把长轮询变成取消的 20 秒阻塞。

## 同机隔离实测（非双机接通）

官方程序 `codex-cli 0.163.0-alpha.11.1`，实际所选型号 `gpt-6.1-sol`。
真实完成文件读取 → 精确补丁 → 再次读取，写入结果和随机 nonce 均正确。
第二次请求保留同一原生 Session，不发送 nonce、不调用工具，仍回忆正确的 nonce 和修改前后状态。
第二轮全程约 8 秒。两个请求均使用真实账号推理，不是协议夹具，也不是 SGM 学习。
仅操作独立验收目录；未修改用户正式项目文件，未重载 VS Code 或操作另一机器。

## 复用记录

`src/codex/rpc.ts` 复用 AOEM-CODE 的已有 stdio RPC 客户端；原文件 SHA-256：
`42295ad0d8ead671bff5940135d728878619c06cc8ee2f5aa783e0f8a23e2cdb`。
只调整类型边界和 ASAS 客户端标识；不复制官方闭源界面，不重新实现 Agent Loop。
原生工具屏蔽与独立登录文件引用沿用 AOEM-CODE 已有成熟实现。

合同来源：[官方 App Server](https://developers.openai.com/codex/app-server/)。动态工具是实验 API，升级官方程序时需重新检查。

## 仍未完成

ACODE 已有 B LocalWorker 和独立同机验收驱动，尚未接入日常 webview 的模型菜单、会话和 IDEHost。
同机调用验证不等于远程客户端已交付或现有扩展界面已切换。另一台机器的 Claude Code 未操作、未验收。
原生 UDP 应用分发进度见 `NOVONET.md`；签名能力公告、跨机回执和完整出口过滤/中译英仍未验收。

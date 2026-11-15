# ASAS Native L3 Application Plugin

唯一源码：本目录的 `service/`；原生宿主：`crates/plugins/asas`。
不另建 ASAS 仓库或目录，不复制造网络、账号、节点身份或加密协议。

## 用户流程

目标：A 安装 NOVOVM L3，开启 ASAS，在 A 分别登录/启用 Codex 与 Claude Code；
B 的 ACODE 连接已授权节点，项目、VS Code 和本机工具留在 B。
NOVONET 负责网络/持钥；ASAS 仍管理项目授权、引擎 Session、工具批准、预算及结果。
提示词、代码、工具正文、登录令牌均不上链；ASAS 不是链交易/共识执行插件。

## 当前实现与未完成项

- Cargo 工作区已登记 `novovm-asas-plugin`；L3 的现有自营 owner 接入显式可选 `asas.json`。
- 没有该文件时不启动 ASAS、不新增监听、不改变原节点身份、网络或存储。
- 选用时校验版本/路径/入口摘要，经继承管道启动隔离 Node Worker，握手失败拒绝就绪。
- 节点退出时关闭管道并有界排空；Worker 异常时仅 ASAS 失败关闭，L3 既有网络/邮箱继续，不自动重跑未知任务。
- 应用 Broker 支持 `claude/codex` 分流、精确 Session 绑定和持久回执。
- **尚未完成**：NOVONET 已认证流到 ASAS 的业务分发/服务公告、真实 Codex 适配、
  ACODE 原生界面接线、轻量翻译后端和两台机器验收。当前不能只开启此钩子就远程开发。

宿主骨架并不是已交付的网络服务。不要向目录伪造一个已可达的 ASAS 能力，不恢复 WSS。
正式路由沿 `crates/novovm-network` 的既有认证/撤销/背压 owner 接入，原生 NovoRUDP 优先。

## 其它机器同步开发

在现有 NOVOVM checkout 执行 `git pull --ff-only`，不要复制本机绝对路径。

```bash
cd plugins/asas/service
npm ci --ignore-scripts
npm test
```

原生宿主定向检查：`cargo test -p novovm-asas-plugin`。
L3 接线检查：`cargo check -p novovm-relay --bin novovm-rendezvous-service --features iroh-transport`。
工具链遵循仓库配置；任何跨版本本机替代检查必须单独说明。

## 开发期开启配置

已有 L3 私有 owner 根目录中的 `asas.json`：

```json
{
  "version": 1,
  "node": "/absolute/path/to/node",
  "entry": "/checkout/supervm/plugins/asas/service/dist/main.js",
  "entry_sha256": "REPLACE_WITH_BUILT_MAIN_JS_SHA256",
  "service_config": "/private/path/to/asas-service.json",
  "shutdown_ms": 5000
}
```

Windows 用实际绝对路径和 `node.exe`。服务私有配置参考 `service/README.md`。
这里只固定入口文件摘要；这不是整个依赖树的签名安装包或供应链验收。
不要提交真实配置、身份、模型登录、任务记录、`node_modules` 或生成的 `dist`。

详细设计见 `service/docs/ARCHITECTURE.md`；实际验证状态见 `service/docs/STATUS.md`。

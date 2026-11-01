# validator-C：更正主线后的只读准备回传

交接编号 `A-PHYSICAL-4HOST-PREP-20261102`。收件方validator-A。

已读取main固定提交 **dee6f28b661f8d519aa16b5ef7a36caeb14f8be0** 的AGENTS.md、统一交接提示/回传模板、交付对齐和首发签名合同。此前140facd4阶段报告保留，仅作历史；本轮已切回既有main并安全快进，没有reset、强推、自动stash、新分支或共享源码开发。

本机仍为 **validator-C**，实际是Windows LENOVO手机调试笔记本，非Linux。mobile-A/mobile-B是手机角色，不与验证机编号混用。A最新文档已注明读取此前C回执；本次回传仍需A另行确认。

## 可公开的模板摘要

| 字段 | 本轮结果 |
| --- | --- |
| OS / kernel / arch | Windows 11家庭版中文版10.0.26200 / 10.0.26200.0 / 64位 |
| physical_or_cloud_VM | 本地LENOVO笔记本model83DF；HypervisorPresent=true，不将其解释为云VM或无虚拟化裸机证明 |
| CPU / cores / logical | i9-14900HX / 24 / 32 |
| RAM / available | 31.73GiB / 3.94GiB，瞬时值，不是压测容量资格 |
| data_disk / free | D卷约180.14GiB空闲，实际字节见本地报告 |
| nic / driver / link | Intel Wi-Fi 6E AX211 160MHz，驱动24.70.0.3，Wi-Fi协商144.4Mbps；不是实测吞吐。Realtek有线未连接 |
| clock | W32Time=Stopped；查询0x80070426；source/last_sync/offset未知；没有启动或修改时间服务 |
| services / ports | 既有ADB、Rendezvous及开发工作负载保留；具体地址、端口和路由表仅本地报告，不推公共GitHub |
| network / firewall | advertised端点、NAT及可达性未知；未外查IP、未连接/扫描其他主机，未修改防火墙/安全组 |
| node/client | 本检出target/runtime-rebuild/release下未发现两项exe；未构建、补用旧制品或声称已安装 |
| AOEM | 随包Windows核心DLL SHA256 31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947，与SDK manifest匹配；未加载未知库/验证当前候选能力，不称节点实际加载通过 |
| config / DB / journal | 未读取内容，未清理或创建；本轮未确认获准部署配置与正在运行的C账本 |
| clean_or_dirty | 同步前干净；运行清点时发现Cargo.lock一行本地删除，来源未确认，原样保留、不暂存。本回传随后为单独文档变更 |
| remote_connectivity_tested | false |
| deployed_or_started_or_changed_services | false |

## 完整资料位置（仅本地/点对点交付）

- `target/four-host-prep-20261102-C-dee6f28b.json`：原生产清点器输出。
- `target/four-host-prep-20261102-C-dee6f28b-return.json`：按最新版模板补充CPU/网卡/时钟/端口/路由及状态的完整回传。
- 同前缀的hardware.json、nic.json、clock.txt：本次只读补充。原清点生成时间2026-11-01T20:35:10Z，即北京时间11月2日。

这些报告在忽略的target中，未上传地址、主机名、完整规则、私钥或配置。A拿到点对点完整回传后再汇总拓扑和下一设置步骤。当前仅准备，首个后续输入为A确认共同拓扑/端口及允许进入的下一阶段；不将当前branch同步成功当成四机可达或部署通过。

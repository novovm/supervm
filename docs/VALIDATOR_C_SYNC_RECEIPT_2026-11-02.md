# validator-C：统一交接读取与回传

2026-11-02 Asia/Shanghai。用户再次指定本机为 **validator-C**。这是本机角色登记，不改操作系统名称、验证者集合或手机身份。mobile-A=PKL110，mobile-B=PCLM50；validator-B仍指阿里云。

交接编号 `C-SYNC-2026-11-02`，收件方validator-A。状态：`SYNCED / INVENTORIED / WAITING_FOR_SHARED_ACCEPTANCE_SHA_AND_CONFIG`。最新源码同步完成，不代表已启动验证者或通过部署验收。

## 已读取的当前规范

- AGENTS.md：最新ML-DSA-65身份迁移、同一完整经济实现、旧链精确退役及部署边界。
- NOVOVM_MULTI_HOST_NUAS_HANDOFF.md：统一交接提示、第一阶段回传模板、2026-10-26旧包不得混跑及后续配置流程。
- NOVOVM_PURPOSE_IDENTITY_PREPARATION.md：独立用途身份、最终服务账户保护、共同域及生成/inspect边界。
- NOVOVM_DELIVERY_ALIGNMENT.md与NOVOVM_ARCHITECTURE_RECOVERY_ROADMAP.md：A拥有共享实现，C负责App/手机消费者；目前真实剩余项与证据范围。

## 按统一模板回传（本轮实测）

| 字段 | 结果 |
| --- | --- |
| host_label | validator-C |
| os_arch | Windows 11 家庭版中文版，64位 |
| repo_root / branch / clean_or_dirty | D:/WEB3_AI/SUPERVM / main；同步前干净；仅本回执为随后文档变更 |
| source_commit | 从66e652d2快进到384afc4e008fc5ad8d5a48a92e34d8d9893f12da；未reset/force |
| cpu / ram / free_disk | i9-14900HX，24核32线程；RAM31.7GiB、空闲5.4GiB；D卷可用182.2GiB，瞬时值非独占 |
| existing_services_and_reserved_ports | ADB PID20940，Rendezvous PID15180；监听包含5037、443、8000、9100、7897及多组开发端口。保留原服务；未全面核实全部监听归属，不停任务 |
| clock_sync_status | W32Time=Stopped；本轮没有取得已同步证明，未修改系统时间或启动时间服务 |
| native_udp_bind_candidate / reachable_endpoint / reachability_evidence | UDP41050无监听，仅候选未预留；WLAN当前192.168.71.102，与10月25日地址不同；未验证跨机/NAT/防火墙，不将LAN地址当公网可达证明 |
| existing_novovm_config_and_data | 历史制品、Rendezvous及忽略的multi-host-nuas资料存在并保留；未确认本轮C共同批准配置、部署身份或现有账本；无旧链退役操作，不能报清理完成 |
| node_binary_path / sha256 | 当前配置target/runtime-rebuild/release下未发现novovm-node.exe；不存在可报告的新制品哈希 |
| client_binary_path / sha256 | 同目录未发现novovm-native-client.exe；未用其他工作树旧exe补齐 |
| actual_aoem_library_path / sha256 / target / manifest_and_capability_check | aoem/windows/core/bin/aoem_ffi.dll；SHA256 31306037d7e764bd93e493ff61d4c69952ed1ffc18c2663b5d2c806d815e8947；manifest platform=windows、profile=fullmax，source=aab3f8bb8cf5c685b8314a08378e8763fbbe83ab；文件哈希匹配manifest。未新增加载/密码能力验证，不称为实际节点已加载 |
| build_or_load_blocker | 尚未得到A针对本轮C的共同构建SHA/配置及chain_id；当前同步SHA不自动升为全组验收SHA。遵照交接不混跑旧包、不自行建钥或启链；时间同步和跨机可达另待验证 |

## 给A的接续

请确认本轮C应消费的完整固定源码SHA、Windows构建/统一管理入口及公开共同配置；A继续拥有共享协议、签名和经济实现，C不重造SDK。新的共识签票已迁ML-DSA-65，传输与服务调用者仍按唯一签名合同逐项迁接，不能由签票通过推断密钥协商或全用途迁移完成。

收到明确输入后，C按原工具在最终运行账户下准备/核验用途身份并回传公开材料及制品身份；私钥、完整配置、数据库不入Git。若需要旧链退役，先逐路径核实用途和活动进程，再按明确范围记录结果，不能删混合父目录或手机/通信数据。

本轮仅同步、阅读和只读清点，无构建、密钥生成、删除、服务启停、手机安装或资产操作。保持原钥、聊天历史和v108；未启用正式资产或生产收费。发布本回执不表示A已接收。

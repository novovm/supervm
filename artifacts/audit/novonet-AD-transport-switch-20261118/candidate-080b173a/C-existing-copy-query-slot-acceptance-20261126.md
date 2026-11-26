# C 原库副本：查询槽自动回收验收

回归针对 `f0507d67`，测试源码来自 C 本地 `8328aabe`。在 C 上只读取得原 RocksDB 独立副本并逐项核对 10 个文件，包含原链域标记。隔离进程作为 LocalService 以 `Existing` 打开副本，固定原 C 传输公钥，查询监听和验证者路径均为 loopback。测试只轮询冷恢复确认、回执投影、原 NativeLifecycle I/O 和原 native query，检查前后链头与共识位置相同。两次独立探针均使用已核验的 Windows Client，测试专用查询身份在同一账户托管，现役身份未更换。

北京时间 09:36，第一条认证查询得到原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4` 的 `Finalized`。旧 Client 退出，达到 30.016439 秒空闲时仍持有唯一槽位。第二个查询身份完成认证，原槽由 `127.0.0.1:49556` 切至 `127.0.0.1:57383`，收到逐字段相同的 Receipt。执行成功，高度 1，索引高度 2，区块 `9ea72a123a87bea55bb3d87c680b1960dfe458c5ce661d5b11bb81afaee47926`，原回执费字段 4000000。

Windows `rustc` 优化构建发生访问违规。测试用 `profile.release.package.novovm-node.opt-level=0` 和 `codegen-units=256` 构建，依赖保持发布配置。原生查询 4 项定向测试通过，C 现役 PID 30328、B PID 571511 在线。未重启、部署、补发或换钥。C 的结果为本机隔离验收，A 原 Client 到现役 C 的跨机复测仍待验收。

可重复用例为 `native_pipeline::service::rpc::query_slot_snapshot::original_snapshot_reclaims_idle_slot_for_another_authenticated_receipt`。测试输入为五项 `NOVOVM_QUERY_COPY_*`，只接收带域标记的独立数据库副本及本机回环查询配置。

验收日志 SHA-256 `f90224ad80a802eb0110dc095e1941afc75ae59de33592946e4e994c7b96a981`。测试 EXE SHA-256 `afda709321e55a39427f7fd1394573680784255278e1630cccac5628be52888b`。

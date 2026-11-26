# A 四验证者自动追赶验收准备

基线 `bd91c9f30bf6c00574304baecfef3cbf6b84d0b4`。Windows x86_64 正式 `novovm-node` 以 Rust/Cargo 1.94.0、默认 `--release`、`iroh-transport` 构建。验收使用同一源码的测试程序集。只操作 A 本机四个隔离的原生节点进程。每一组用例使用新目录、随机回环端口、单独的测试创世和各节点独立的测试用途钥。恢复过程中各测试进程只打开自己的测试原库和原签名日志。A/B/C/D 现役节点、原库和钥匙没有参与，现役交易未重发。

## 可执行入口

以下命令从固定提交的独立 Windows 工作区运行。使用已核验的 Windows Node 和 AOEM DLL。每组只跑一个进程用例，保留独立测试目录及 `recovery-report.json`。路径变量必须指向隔离构建和已固定的 DLL。

```powershell
cargo build --locked -p novovm-node --release --target x86_64-pc-windows-msvc --features iroh-transport --bin novovm-node
$env:NOVOVM_RESIDENT_NODE_BINARY = $IsolatedWindowsNodeExe
$env:NOVOVM_AOEM_TEST_LIBRARY = $PinnedWindowsAoemDll
cargo test --locked -p novovm-node --features iroh-transport --lib native_only_recovery_late_lowest_validator_original_database_and_minority -- --ignored --test-threads 1 --nocapture
cargo test --locked -p novovm-node --features iroh-transport --lib native_only_recovery_late_highest_validator_original_database_and_minority -- --ignored --test-threads 1 --nocapture
cargo test --locked -p novovm-node --features iroh-transport --lib real_late_quorum_reopen_preserves_both_votes_old_proposal_proof_and_locked_candidate -- --ignored --test-threads 1 --nocapture
cargo test --locked -p novovm-consensus --lib lost_ack_reopen_recovers_exact_signed_outbox_without_resigning
cargo test --locked -p novovm-consensus --lib duplicate_and_equivocation_never_add_weight_or_overwrite_the_first_vote
cargo test --locked -p novovm-node --features iroh-transport --lib aoem_qc_binds_epoch_set_and_weight -- --ignored --nocapture
```

[四进程原生追赶用例](../crates/novovm-node/src/native_pipeline/consensus/native_product_recovery.rs) · [签名日志冷恢复用例](../crates/novovm-node/src/native_pipeline/consensus/journal_recovery_integration.rs)

## 验收顺序与证据

1. 四个已固定的验证者各占权重 1，法定门槛为 3。用例故意让一个测试节点晚到。另三份合法权重用原生 NovoRUDP 和真实 AOEM 提交到高度 40。收到的 Precommit QC 必须绑定相同 chain/epoch、集合、父状态、高度、轮次和区块。重复或冲突票不得增加权重。
2. 启动落后节点，在追赶中终止该隔离进程。停稳后从该进程的原测试库读取中断时已持久块及其签名 outbox，验证原提案和 QC，确认高度 40 尚未写入。以相同库、配置和钥匙按 `existing` 重开，让原网络和归档链路自动追赶。四端在目标高度的区块哈希、状态根、回执承诺、索引高度和最终回执须一致。已建立会话的节点再独立重开一次，追上另外三端已推进的高度 44。
3. 仅保留两份测试权重。通过测试钱包首次提交新签名输入后，至少观察 12 秒。旧链头保持不变，测试输入无 `Finalized`。同一批次保持待处理，恢复第三份权重后须自然终结，再恢复第四份并从四端确认一致。全程钱包重提计数为 0。
4. 追赶结束后，停稳该测试节点，逐字核对中断时的签名 outbox 字节仍在。独立 AOEM 冷恢复用例重开真实签名日志和原锁定候选，复用原 QC 不得产生第二票，也不得改变落盘元数据。缺少旧签名事件的日志须拒绝打开。另用丢 ACK、重复票、跨纪元和换集合的定向用例复核边界。

## A 本机复验

| 场景 | 中断时已持久高度 / 目标 | 晚到启动至中断后追平 | 两份权重无出块及无最终回执 | 四端最终高度 / 回执数 | 原 outbox |
| --- | --- | --- | --- | --- | --- |
| 排序最低者晚到 | 1 / 40，3 份 QC 签名有效 | 11.961 秒 | 12.019 秒，链头停在 45 | 46 / 92 | revision 1，字节一致 |
| 排序最高者晚到 | 1 / 40，3 份 QC 签名有效 | 12.053 秒 | 12.017 秒，链头停在 45 | 46 / 92 | revision 3，字节一致 |

同组四端的区块哈希、状态根及回执承诺一致。两组各自使用测试域和测试状态，比较在组内进行。实际 I/O、归档、认证和执行走原产品路径。以上覆盖一台机器上的四个真实验证者进程，物理多机共识追赶仍待独立验收。

多机复验沿上述顺序，最小环境为四台各自独立的隔离进程或主机、四份新测试库及测试身份、同一固定测试创世、可双向认证的测试端口、一个仅提交新测试输入的钱包和每台节点的 UTC 日志。只中断隔离节点；形成权重门槛的证据来自实际验签和投票归集。跨机结果需逐端回传来源、原签名 outbox 摘要及四端最终状态，通讯切换验收与共识追赶分别取证。

[A/C/D 三机验收入口](NOVOVM_DISTRIBUTED_VALIDATOR_CATCHUP.md)允许 A 托管两个隔离进程，C、D 各托管一个。各端在统一窗口前单独执行 READY。

## 构建与日志指纹

- 真实 Node EXE SHA-256 `104c8a2f4fb1cfd5938821604a304cfbc72828893b1854beed504b2d07370757`
- 同源码测试程序集 SHA-256 `444011bb739157353b3de23cc884f3e977f2ce2f58edcab73a1b1e1f3df50c29`
- AOEM FULLMAX Core SHA-256 `f9e8ded21753dfd32573de7bf66da8a785120444e810ba9fa865bfa4d639fcaa`
- Cargo.lock SHA-256 `299030421087c2ee5b16330a436f16baf7c7c3a06f9cae23ab50af10aba4c46f`
- 排序最低者报告 SHA-256 `7dec66f1d236303b5baff653296e59fec3a7b31126c055570551991439036017`
- 排序最高者报告 SHA-256 `a3c68c6049ab32f266538930e213ed8dd0bf1f418dc47a81f479da41f7996253`
- 原签名日志 AOEM 冷恢复日志 SHA-256 `70aa2ea30f736de6569bb6bffc9571c86dff72cea5b6b538a8295dfc6fc4a789`

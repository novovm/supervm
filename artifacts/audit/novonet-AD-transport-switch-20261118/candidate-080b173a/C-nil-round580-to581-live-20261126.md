# C 现役从第 580 轮持久进入第 581 轮

11 月 25 日北京时间 23:54，A 使用原认证入口确认 A、B 都持有同一原交易 `68d83d0b5b11b205f5ca6d2ff674dea4adbedece3327353885073eb139bd6cd4`，均为 `Received`、索引高度 0。切换前，C 的原数据库七个持久输入文件与已复验的第 580 轮副本逐项哈希一致。[A/B 切换前原始回执](A-B-pre-C-existing-recovery-custody-recheck.json)

11 月 26 日北京时间 00:01，按批准仅切换 C 一次。原 PID 24256 退出，`9f223abf` 的 PID 7384 以 `LocalService` 启动，继续使用原库 `existing`、原验证者和传输身份，监听原两个 UDP 端口，AOEM 哈希与原固定值一致。启动后已清除一次性 SCM 环境变量。B 原 PID 571511 在切换前后在线。未补发交易、初始化或换钥。

C 现役以原父状态验证 A+B+D 的 580 nil `Precommit` 证明 `f5614fada23e812106dde96b770f2f120f2f68cdac62bc2e58e97bd03200f363`，归集 3/3。原 `Precommit` 计时器起计。00:10:48，C 进度首次出现高度 1、轮次 581、`Propose`。另取原数据库共享只读字节副本，哈希核对后由 SUPERVM 原 `ValidatorJournal::open` 复验，确认 581 持久化，outbox 从 786 增至 787，尚无 chain head 或决定。

00:11:05，配套 Windows Client 固定 C 原公钥，认证同一交易及实际 1/581，回执仍为 `Received`，索引高度 0。候选在本地 offer 中，`signed_proposal=null`。`step=Propose`，`pending_journal=null`、`proposed_round=null`、`prevote_qc=null`。00:11:13，C 的 581 collector 已验证 A 的一份 nil `Prevote`，权重 1/3，`Precommit` 0/3，两个阶段计时器均未起计。C 与 A、B、D 的原 UDP 路由均已建立认证会话。

第 581 轮排程提案者为 B。C 当次未观察到 B 的 581 签名提案。已请 A 通过原认证入口复核 C 的实际位置和 B 的最新轮次。B 的切换前认证位置为 580，进程在线记录只证明在线。

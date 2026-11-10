# C 旧进度结构只读回传

状态：`C_PROGRESS_STRUCTURE_READ_ONLY_COMPLETE`。这不是 existing 更新就绪。

- 按 main `e4b155c8` 的 [最新交接](../novonet-seed-retention-20261110/candidate-1ef31893/CD-UPDATE.md)执行一次原 `Inspect-C-Progress.ps1`，管理员 PowerShell 7。
- 时间：2026-11-10 10:09:58 北京时间（02:09:58 UTC）；进程退出码 **0**，输出 `READ_ONLY_COMPLETE`。
- 脚本 SHA256：`d72c07f74803707b8fe6c5c969f99cd8e957dc4e7fdf9bfeb0d9455fd8182cb2`。
- 新证据目录：`D:\WEB3_AI\SUPERVM\target\c-progress-inspect-20261110-100954\evidence`；此前失败原件保留。

## 实际输出与边界

原 progress 文件大小 **813127** 字节，SHA256 为
`a917d1daf6f38d05748209520752b122ecdc1250812df597b3cfd7b7021ea36f`，
最后写入时间 `2026-11-09T04:55:26.5561863Z`。

固定脚本输出 `shape.type = null`，`facts`、`nested_facts` 为空，
`stdout_json_tail = []`、`owner_exit = null`。这是该脚本在 C 的真实返回，
不能据此断言原文件内容只有 JSON null，也不能推断数据库损坏、状态根或正常退出。
未修改读取逻辑、补造 chain_id 或重试更新；请 A 按该真实结构结果审阅后续读取条件。

脚本报告 `node_started=false`、`database_opened=false`、`keys_read=false`、
`progress_modified=false`。本轮未启动节点、未打开或改写数据库，D 未操作。

## 原始公开结果

- [结构报告](progress-structure-public.json)
- [执行时间、脚本摘要与退出码](execution-result.json)
- [完整执行输出](execution-output.txt)
- [以上文件的 SHA256](SHA256SUMS.txt)

以上三份结果按原字节复制，无私钥、用途材料或整份配置。
本轮没有编译、重跑更新或手工触发完整 CI。

# C v2 旧进度与退出证据回传

状态：`C_PROGRESS_V2_READ_ONLY_COMPLETE / C_NOT_READY`。

按 main `48c41b83` 的原 Inspector v2，在管理员 PowerShell 7 中只执行一次。
北京时间 2026-11-10 10:30:44，退出码 **0**，输出 `READ_ONLY_COMPLETE`。
新证据目录为 `D:\WEB3_AI\SUPERVM\target\c-progress-v2-20261110-103030\evidence`。

| 原文件 | 实际结果 |
| --- | --- |
| progress.json | 813127 字节，zero_bytes=813127，all_zero_not_json；摘要与首轮一致 |
| node.stdout | 0 字节，empty |
| exit.json | missing |

没有可用的同次退出投影或 exit.json。不能据此确认升级前当前链状态或数据库损坏，
也不能把任务 LastTaskResult=0 单独等同完整节点正常退出证明。
请 A 根据真实缺失情况明确原库恢复验证入口及验收边界；本机未补造状态、重跑更新、
启动节点或打开数据库。D 未操作，旧证据保留。

公开原件按字节复制：
- [progress-structure-public.json](progress-structure-public.json)
- [执行结果与脚本 SHA256](execution-result.json)
- [完整执行输出](execution-output.txt)
- [原件 SHA256](SHA256SUMS.txt)

结果文件原 CRLF 保留以便核验摘要；文档与原件分开。没有私钥、完整日志或配置，
没有编译或手工触发完整 CI。

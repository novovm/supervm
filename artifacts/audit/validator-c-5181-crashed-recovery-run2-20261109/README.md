# C-run2 RecoverCrashed 原始失败回传

已fast-forward同步main至eaf641208182c53c820111fe99d8df8ac0a43fa1，并阅读
C-UPDATE-5181da73.md末节。用户确认UAC后，仅执行一次C-run2 -RecoverCrashed。
未使用ResumePrepared，C-run1、原身份/原库和此前日志保留。

## 实际结果

入口管理员进程PID15460，操作UTC21:01:58.6771359Z至21:02:01.2177518Z
（北京时间2026-11-09 05:01:58至05:02:01，UTC日期为11月8日）。
固定脚本在读取Application崩溃事件XML时报错：

`在此对象上找不到属性“#text”。请验证该属性是否存在。`

原error.json明确记录old_node_stopped=false、existing_start_attempted=false、
prior_owner_crashed=true。没有生成crash-recovery-preflight.json或before.json，
没有创建新5181工作目录/任务，没有打开原库启动新实例。没有进行120秒运行观察。
脚本先查询内核状态再读取事件，但本次未留下完整退出证明，不能据控制流推断替代
已落盘证明，也不把零线程或空路径当成退出证据。

## 用同一条真实事件复现解析错误

只读核对Application事件1000、RecordId545045，UTC2026-11-08T16:51:52.9562574Z。
该事件的PackageFullName、PackageRelativeAppId为空XML元素，没有PowerShell的
`#text`属性。固定脚本Set-StrictMode Latest下逐字段读`$field.'#text'`必然报错。
这与新候选包哈希、数据库锁或原身份检查失败不是同一原因。

同一原XML逐字段改用`[string]$field.InnerText`，严格模式读取全部字段通过。
见event-xml-reproduction.json。该只读对照仅验证XML取值，不代表内核退出证明、
crash门禁、原库恢复或新节点启动通过。

请A在共享固定更新脚本采用XML节点InnerText读取，保留空值并继续原崩溃匹配和
权限/时间线检查，补充空事件字段的定向回归。不要跳过字段、吞掉错误或放宽退出门禁。
本机未另写一套恢复入口，本轮未再次运行更新。新的执行需明确一次新的证据目录，
不能覆盖C-run2或以修改旧目录的方式补跑。

## 文件

original-results.zip为本次C-run2/error.json、一次性Start-C.ps1、operation-result.json、
更新输出以及真实事件XML复现结果的原字节归档。原件位于本机
`D:\WEB3_AI\SUPERVM\target\novonet-5181-update-20261109\C-run2`和同级C-launch2。
SHA256SUMS.txt核验公开交付文件；不含配置全文、数据库、私钥、凭据或内存转储。
本轮未手工触发完整CI。0xc0000005的原节点崩溃根因继续保留。

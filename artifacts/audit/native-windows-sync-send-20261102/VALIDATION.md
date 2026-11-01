# 离线解析验证记录（2026-11-02）

本次验证只执行现有导出文件的归约，不重新录制、不运行节点/交易测试、不构建生产制品。

- `node --check analyze-sync-send.cjs`、`node --check analyze-schedule.cjs`通过。
- 调度归约重新读取两份CSwitch/ReadyThread CSV和独立cpu-util导出：
  Running/Ready/Waiting/Unknown分别为`4825833/70651/3481/35`和
  `4826751/64813/7083/1353`µs；每组总量均为4,900,000µs。
- 主归约重新读取三组目标线程名、原始profile事件及9份HTML，复核7个零损头、
  6份实际image/PDB哈希、5个完整drive覆盖窗口，全部通过。
- 独立只读复核9份HTML的send、loopback、真交集、内核互斥叶及未知样本，与RESULTS一致。
- 原栈解析器导入不会执行历史CLI或改写历史JSON；历史三份HTML仍解析出
  `8191/8178/8206`栈命中。没有运行绑定旧PID的历史主程序。
- 新归约重复执行输出一致。`RESULTS.json` SHA-256：
  `2ebc53b7678424e1c23feae9f89515236e625e887b2ad817daeb4a9641bc388a`。
- `git diff --check`通过（仅本仓已有LF/CRLF转换提示）。

原产品报告由既有`native-delivery-closure-20261030/collect.ps1`收集，压缩归档不改原内容；
原报告/压缩件哈希保留在`qualified-workload.json`。系统ETL/CSV、PDB和符号缓存不提交。

五轮交易、账务、冷恢复通过是**用户已完成的采集负载结果**，本轮离线复核它们，
不是本轮重新运行的测试。没有新强杀验收、无观察持续供给或全仓CI通过声明。

# Phase5 审阅入口（不是部署或要求A重跑的指令）

本次只实际运行一次新增 status 上下文。旧 Phase4、Phase3 样例没有重跑。

## 固定输入

- 源码：a25452f4c49be37cd984e650f2079642e6c273b3，`git archive` 的独立副本。
- 锁文件及所用原实现：见 source-verification.json。
- AOEM966：diagnostic-build-public.json 中的明确哈希；本机私有副本。
- Rust 1.94.0 / Windows MSVC，实际参数见 build-command.json。

## 如何审阅诊断差异

仅在私有源副本中：

1. 原 lib.rs 末尾加入 `#[path="c_phase5.rs"] pub mod c_phase5;`，模块内容见 diagnostic-module.rs。
2. 将 diagnostic-main.rs 放入 `src/bin/c-phase5-status.rs`；Cargo.toml 登记对应 bin。
3. 根目录和库内其余原实现未改；build2 使用 package debug=2 和原 release 优化。
4. 导出 marker 只为调试器定位，局部 allow(unsafe_code) 仅覆盖 no_mangle 属性，无 unsafe 函数体。

原政策字段来自原 uca_native_tests.rs 的测试 fixture，见 original-policy.rs；不是新经济规则。
所有目的钥、单账户初始金额和数据库属于新隔离测试，不使用原部署输入。

## 采集证据

map-functions.py 离线读取 PE，匹配 Phase3 原 a254 归一化机器码及 unwind，才确定断点 RVA。
collect-boundaries.py 是本轮实际入口；已有结果、测试目录不能复用，任何以后有理由的新实验
必须选新的目录/文件名并重新核验制品。不要直接在已保存证据上执行。
capture1.cdb 中 `<SUPERVM>` 是公开脱敏标记，不能直接当作有效机器路径。

程序用真实原库接口建的是**全新隔离测试数据库**；`CreateNew` 绝不指向部署原库。
只有回环 UDP 地址、单个 owner，没有外部发现配置，没有业务交易/中继对象。
启动器清除继承的 NOVOVM/AOEM/PHASE5 及 credentials 环境再加入显式测试配置。
一次执行有 180 秒上限，超时只结束本次由 Popen 启动的 CDB 测试树；本次没有超时。

check-boundaries.py 只解析已有日志，不运行节点或调试器。448完整组与33缺口分开保留，
不得把缺口删去后声称总采集全部通过。公开记录含地址而无缓冲区私密内容；完整日志私有保存。
alternate-writer-symbol.txt 与反汇编来自同一新 EXE/PDB 的离线检查，没有为补覆盖重跑程序。

本轮编译失败101及后续成功0均保留；不得将诊断准备错误写成原节点故障，也不得把
诊断程序正常退出当作 C 稳定性验收。下一步应补原实际负载上下文，不循环运行本例。

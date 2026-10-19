# 原生完整代数计划：层间驻留复用与55ms定位

结论：实际复用、构建和测量了既有驻留实现；**未达到晋级门，SDK未更新**。
不是新的执行器，也不是七月性能恢复、百万TPS或GPU胜过CPU的声明。

## 实际接线与复用

冻结原节点e7e41e50：原tick/UdpTransport → NativeLifecycle → CandidatePipeline
→ novovm-exec → aoem-bindings → opcode113/AOIP0 v10 → AOIR0002。
七月NovoRUDP/APFL入口、完整费用/失败计划、CPU快路径、共识和发布均不改。
远端0b47ba7e的carrier反馈改动已保全，但未混入冻结程序的对照身份。

当前AOEM `31b14d6d` 的composition执行按level/program/chunk逐次
`exchange_limbs`，每次上传、等待和回读，然后CPU绑定后继输入。
复用`da80f306`的12个运行文件，逐blob相同；保留当前model/session改动。
已有实现将guard/fallback/中间值保留在设备arena，一次上传计划、一次提交
所有层与导出命令、一次终端回读。未消除层屏障，未改变i1024或失败语义。
历史拒绝原因保留；本轮原生入口不同于旧HTTP观察场景，不等于自动批准。

## 预定八轮正式结果

插电Windows，四原节点/四库/一张GPU，单原生SDK入口；65536真实签名，
1024付款方各64nonce、共享收款方，批1024、窗口8192、查询128。
计时终点四节点最终成功回执；诊断关闭。启动、签名生成和事后冷恢复不计时。
没有HTTP，没有降低法定人数、收费或持久化要求。

|路径|基线两轮TPS|驻留两轮TPS|合并总成功/总时间|
|---|---|---|---|
|完整代数|3595.63 / 3583.65|4128.91 / 3974.49|3589.63→4050.23（+12.83%）|
|CPU|7642.70 / 7672.36|7659.53 / 7642.97|7657.50→7651.24（-0.08%）|

整数P95秒：2.311/2.321→2.036/2.092；P99：2.404/2.397→2.481/2.500。
CPU P95：1.130/1.173→1.112/1.081；P99：1.330/1.328→1.238/1.287。
CPU这一微小差异不是稳定回退或因果证明，但不满足预先固定的无下降门；
整数P99两组也变差。因此未追加有利样本，不晋级，不覆盖正式库。
八轮均65536成功、0业务失败/nonce拒绝/传输失败/超时；费用2949120、
金额与回执费用守恒、nonce顺序和四库冷恢复通过。业务失败另测，不计作成功TPS。

## 55ms的实际调查结果

两组原有stage-profile诊断（与正式跑分分开）按观察调用数加权：
integer_wire **54.65→49.96ms/批**，execute_integer 82.27→77.62ms；
编译17.37→17.40ms，树6.26→6.34ms。快照未覆盖全部末尾调用，不能当完整
64高度总成本；嵌套时间不相加。取消层间往返并未消除约50ms，不能称其为唯一瓶颈。

随后仅在本地诊断构建加入临时分段计时（补丁见diagnostic-only.patch，未交付）：
256调用平均解析4.68ms，lowering/layout7.26ms，资源准备0.45ms，
run36.99ms，最终编码0.23ms。run最大402.62ms、资源准备最大40.72ms，
保留首次/异常慢调用，不通过删除冷样本改善成绩。

再复用原`dispatch_compute_ops_profiled`取得同一实际composition命令的时间戳：
256调用全部timestamp_query_ok=1，平均约29 dispatch/2 CopyRegion，
准备上传/命令列表2.33ms，最终read1.34ms，record0.070ms、submit0.046ms，
host wait25.72ms，设备首尾命令区间21.53ms，逐dispatch时间戳区间合计18.19ms。
这与上一分段是不同诊断轮，不能拼加；GPU时间戳区间含调度/竞争间隙，
不是四节点独占GPU的纯shader时间。设备选择配置5090/0x10de；本轮未另采
资源设备身份，不以选择偏好重新签收硬件身份。

**定位收敛：** 结果编码不是大头；层间往返是可削减项但并非全部55ms；
驻留后主要成本仍在完整checked-i1024计划的设备命令与等待，其次是解析/layout。
后续应检查这份实际计划如何消费已有算子/编译能力、设备内执行工作及并发资源，
不能继续把55ms称作JSON/拷贝成本，也不能直接缩窄精度或删除条件检查。
未测寄存器spill、shader独占时间或各算子的成本，不能先宣布其中任何一项为根因。

## 验证与处置

- 真实node完整费用/失败后继CPU对照通过。
- AOEM资源/组合/65层全宽/未导出fault/guard/预算/并发别名等19项通过。
  初次过滤器匹配0项，随后正确过滤器执行19项，不把0项算通过。
- 八轮正式、两轮stage诊断、两轮分段诊断均通过经济与冷恢复。
- 当前未新跑A/A/B、离线节点、Linux、物理多机或完整业务证明。
- 原49草稿不改；正式CPU默认与SDK保持原样。撤回本轮活动运行补丁和临时计时；
  既有可复用代码保留于da80f306，证据保留，不留下第二执行模式。

身份、每轮原始报告位置及SHA256见[measurements.json](measurements.json)。
基线DLL95b7133d、候选621319cd；两者相同Rust1.94.0/locked release FULLMAX，
同缓存构建。基线是当前AOEM源码重建，不冒充随仓31306037的二进制。
正式节点91e1da7c、driver e8053ddb；诊断库与正式候选分开，未用诊断TPS晋级。
本轮的实际代码试验未成为生产改动，不能报告“主要瓶颈已修复”。

### 恢复后复验与残留文件

AOEM运行源码恢复后，FULLMAX release重建完成；实际执行
`composition_multilevel_values_and_unexported_faults`（含ignored）1项通过，
AOEM工作区干净。恢复构建DLL SHA256为
`db190e3afe34aef538d38e439299553d24ccc3eb66fed450bfcdb3fcdb1924b8`；
不把重建哈希不同的文件宣称为基线二进制相同。正式SDK未更新。

以下本轮生成文件的精确路径清理被环境策略拒绝，仍留存，不属于活动产品加载路径：

- `target/native-resident-reuse-20261019/aoem-candidate.dll`
- `target/native-resident-reuse-20261019/aoem-diagnostic.dll`
- `target/native-resident-reuse-20261019/aoem-device-diagnostic.dll`
- AOEM仓库 `cargo-target-ci-local/release/build/aoem-gpu-kernels-8063584b344983f9/out/apfl_integer_composition.spv`

不换工具绕过删除限制，不将残留生成文件计为已清理；原始报告和历史实现继续保留。

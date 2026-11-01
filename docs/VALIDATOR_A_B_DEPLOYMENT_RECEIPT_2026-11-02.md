# A接收C回执及B打包部署准备：SSH已恢复，制品构建中

2026-11-02，交接`A-B-DEPLOY-20261102`。用户已更正：刚回传的是C，不是B；
并授权A给B准备包及部署。不得将这一授权误记为B已经回报/就绪或正式创世批准。
本轮保持main开发，不新建分支，不覆盖B的邮箱、Relay、Web、密钥或数据库。

## 已接收C

A已将main安全快进到`06801146afe0ef3f8535b5b53694cdb6ed20e87b`，读取
[C最新只读回传](VALIDATOR_C_READONLY_RETURN_2026-11-02.md)和此前回执。
C为Windows笔记本；Wi-Fi、剩余RAM、W32Time停止及Cargo.lock来源未明的本地删除均已读。
C完整私有网络资料尚留C，A未据公开摘要宣称收到其全部地址/路由或四机可达。
C保留该本地修改，不自动还原、stash或覆盖；先等A固定制品，不重写共享实现，
也不因部署B而重启手机/ADB或Rendezvous。

## A的已验证构建进展

- 已启动既有WSL2 Ubuntu24.04.4，实测`x86_64`，Rust1.94.0及本机目标已安装。
  Rust/C/C++编译器可用；已沿原构建需求安装Ubuntu CMake并确认现有clang、glslang和SPIR-V工具。
  官方PowerShell7.6.9 Linux x64压缩包通过其release SHA256校验，安装在WSL用户工具目录。
  没有改变WSL网络设置，构建缓存放WSL用户目录；仍受宿主C盘真实空闲约47GiB约束，
  虚拟磁盘内报告的870GiB不能当作宿主实际空闲。
- 原安全CI失败的`libz-sys 1.1.25`已用Cargo正常解析升级到`1.1.31`。
  同次解析将已锁定tempfile3.27.0的getrandom依赖边从0.3.4校正为0.4.2，
  未升级其他包版本。根Cargo.lock和runtime/Cargo.lock的cargo audit退出0；
  根保留4项、runtime保留2项既有允许警告，未新增忽略规则。
  漏洞依据：[RUSTSEC-2026-0418](https://rustsec.org/advisories/RUSTSEC-2026-0418.html)。
- 用原novovm-exec消费者在Linux构建成功；随后运行原
  `pq_signature::semantic::tests::actual_semantic_mldsa_cpu_contract`，
  使用随包Linux SO时明确失败：`AOEM ML-DSA batch verification capability unavailable`。
  这是实际运行失败，不是仅凭manifest日期推测。
- 该SO为既有`1377cd1e` Linux FULLMAX包，SHA256
  `4d06f36a67f41035661100ac0fd18c67dcae60bf2cdd5df28db33b555c538309`。
  虽有旧ML-DSA导出和统一wire，但缺少当前消费者所需的compute-result接口；
  不能把“导出了旧批验签”当作新统一语义合同通过。

因此未把旧SO打入新包去冒充合格运行库，未退回直接旧ABI，也未替换正式随包库。
当前没有可交付的新Linux整包/归档哈希；本轮Rust单元回归与格式检查单独记录，
不能用它们覆盖上述真实SO反例。匹配Linux库的构建和当前合同资格仍须A收口，
不把不完整包交给B或让C另写一套接口。

## B连接：用户调整VPN后原方式恢复成功

已回读10月25日本机会话及`artifacts/local-validator-B/ssh-check.stderr.log`：
当时相同Windows OpenSSH9.5p2、同一`validator-B.conf`、严格host-key pin、公钥认证，
最终`Exit status 0`，并完成过合法归档上传。
本轮沿用原受保护配置/钥匙和host-key pin，包含原`-T`及keepalive参数复试，
没有重建密钥、改known_hosts或关闭主机验证。

起初TCP22连接可建立，但SSH在收到服务器版本横幅前超时/关闭，未进入密钥认证。
不能称作密钥错误、B已经宕机或安全组必然缺规则。
只读路由表显示当前该目的地经Meta虚拟网卡；这只是排查线索，不是已经证明的根因。
用户先批准单地址临时对照，随后告知已自行调整VPN；A因此没有再改路由/代理。
复用原SSH成功：主机身份、ecs-user、x86_64和sudo均通过，B时间显示北京时间11月2日。
到B路由仍显示Meta，因此仅能确认VPN调整后恢复，不声称已证明虚拟网卡就是根因。
邮箱、Relay、Nginx、Docker均active；原邮箱/Relay PID仍为944911/958，未重启。
原部署标准目录仍未发现，10月25日专用incoming目录保留。UDP41000/43171/43173已占用，
不能把旧包默认41000直接用于B验证者。没有开端口、安全组或修改任何B服务。

## 给B的下一步指令（A负责打包，不要求B重编）

> 交接A-B-DEPLOY-20261102。A已经接收的是C回执；本轮B无需重跑完整环境清点。
> A复用原SSH已经连接成功，无需为此前横幅超时重启sshd、换钥匙或重建known_hosts。
> 保留原服务/数据/占用端口，不放开全网来源或停邮箱、Relay、Web。
> A将提供固定main源码、Linux x86_64包清单/哈希、匹配AOEM及共同配置的plan。
> B只负责接收核验和目标机运行/恢复验收，不承担重编译，不使用D的ARM64包。
> 收到合格整包和配置前不创建创世/账本/用途密钥、不启动验证者，保留邮箱/Relay/Web。
> 历史上传归档保持原样，不把旧包重新命名成当前候选。推送本提示不等于B已读取。

## 当前状态与部署门禁

`C_RECEIPT_ACCEPTED / A_LINUX_CONSUMER_BUILT / LINUX_AOEM_CONTRACT_FAILED /
SSH_IDENTITY_AND_SUDO_VERIFIED / FULLMAX_LINUX_CORE_BUILD_IN_PROGRESS /
NOT_PACKAGED / NOT_DEPLOYED`。

AOEM使用`4c57372a`已提交基线加既有ML-DSA候选源码快照（overlay SHA256
`8ee88c113611cbdcfacce949ae06b49f9870bb8cdd570a8eddeb8343505ab4fb`）在WSL独立解包构建，
不新建Git分支、不改兄弟仓未提交工作。保留原FULLMAX特性编译，不将CPU精简库冒称FULLMAX。
原SUPERVM打包器增加显式隔离AOEM包根和独立核心哈希pin输入，默认随包路径不变，
仍验证原manifest/特性范围，避免为了候选打包替换正式随包库。这不表示构建或新库资格已通过。
调用形式为原脚本附加`-AoemPackageRoot <已核验的完整包根> -AoemCoreSha256 <独立固定核心SHA256>`；
默认调用不变。原安全28断言、构建失败检查和完整11程序装配夹具通过，新增独立pin缺失/
不配对/不匹配拒绝及显式包根成功反例均通过。夹具不是实际ELF资格。
Linux原novovm-exec单元回归83通过/29显式忽略；真实旧SO反例不在这些通过项内。
Linux aoem-bindings单元回归33通过/8显式忽略；原打包安全、失败和整包装配夹具
在Linux再次通过（安全29断言），不是只在Windows模拟路径。

原11程序的Linux x86_64 release已全部构建：node/client、6个原product工具、
原rendezvous/supervm-relay（显式iroh-transport,tls-transport）及novovmctl。
实际node SHA256 `56989b83c13d4eb77065cecd6537f30e1db0f2183fbe33036e5f65e7c3686f70`，
client `5fade645df62278d08e530580d9fef78089733ecd516bca28ea23f26fec25d61`；
尚未完成整包和目标机验收，不能据此登记部署完成。

AOEM第一次使用WSL默认stable（Rust1.94.1）编译时发生编译器内部崩溃，
位置为aoem-gpu-kernels的unused/deathness分析（不是测试失败）。已停止该失败构建，
保留源快照/输出，改用SUPERVM固定的Rust1.94.0及相同完整特性重试；
没有关闭告警、改算法或减少特性。快照内也用Cargo升级libz-sys到1.1.31，
最终库须绑定额外锁文件哈希，不能只引用原overlay哈希代表最终源码。

后续按原owner继续：合格Linux AOEM → 原打包器及安全/功能检查 → 恢复可信B连接
→ 核对已有路径/服务及包哈希 → 隔离安装、原lifecycle plan。
共同域、获准初始状态、用途身份及路由未齐时，部署授权不能替代这些具体输入；
不能从`.example`生成资金/身份，也不能用`START=create`绕过原库恢复检查。
只有实际完成安装/启动/原生查询/账务和恢复，才登记对应完成层级。

## 最新人工确认：只安装程序，不启动链

用户确认尚无此次四机共同配置，先安装程序。本轮只允许合格程序包的隔离安装与
目标机装载/帮助检查；不启用或启动验证者，不生成共同创世、链域、用途密钥或分配NOV，
不从示例配置自动建库。B现有邮箱、Relay、Web、身份和数据库保持原样。

Rust1.94.0也重现相同内部崩溃；完整回溯定位到`annotate_snippets`告警渲染，
不是MSM执行。原终端宽度异常，诊断代码片段被过度裁剪。
同一源码、特性和编译器改用`--message-format=short`后，GPU kernels检查退出0，
19项原告警仍正常输出，未关闭lint、减少特性或改MSM数学逻辑。
完整FULLMAX构建继续复验；仅模块检查通过不能登记新SO已合格。

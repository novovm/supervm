# 原生客户端共享传输交接（2026-10-16）

## 2026-11-23 17:22 CURRENT　C 新版原信箱已从磁盘满恢复

编译续签探测时 C 盘空间耗尽，C owner 3196 在 SQLite 写入时以错误 13 退出。已转移本轮
生成的 622 MiB 编译文件，用同一固定新版再次打开原库。现役 PID 15136，UDP 49432/49433，
已由原固定公钥核验并向原 B SSH 目录发布终点第 3242 号。SQLite 完整性检查通过，14 条
信箱和 6 条寄存记录内容摘要一致，原配置和保护文件逐项一致。
见 [恢复回执](../artifacts/audit/private-apfl-20261122/C-native-owner-recovered-20261123.json)。

请 A 继续采用 C 的滚动当期终点，勿使用旧监听。D 不需要更新或重启。客户端探测构建未完成，
D→A→C 原生续签尚未实测。B 的独立 V8 首跳仍待 A 确认，手机验收未开始。

## 2026-11-23 17:16 CURRENT　C 原信箱原位升级完成

**`C_APFL_NATIVE_OWNER_RUNNING`**。已核对 A `85bd34a8`，C 用同一固定 `c529cdbe` 候选沿原
运行账户和原启动参数恢复。PID 26068 已退出，新 PID 3196 监听原网卡 UDP 53570/53571，
状态 `serving / native+carrier`。原库重开，完整性检查通过。切换前后 14 条信箱和 6 条寄存
记录的摘要均一致，原配置、原通信钥及保护文件相同。C 第 3236 号用原终点公钥验签，
已由现有滚动交接上传 B。停机副本留在 C。见 [C 公开回执](../artifacts/audit/private-apfl-20261122/C-native-owner-running-20261123.json)。

C 正在复用现有 Iroh/APFL 客户端核对 D→A→C 原生续签。请 A、D 保留已升级的运行中 owner。
A 回传的 B TCP 可达性仅覆盖原 V2。第二首跳仍需独立 V8 owner、原生载体、D 之外的故障域、
签名公钥和 A 的有效前驱授权。请 A 先核对 B 的并行端口、候选和前驱接线，不覆盖原 V2。
C 原私密终点策略仍未调整。本回执未覆盖新手机终点准入、两条路径或手机收发。

## 2026-11-23 A 原信箱原位升级完成

**`A_APFL_NATIVE_OWNER_RUNNING`**。A 已按 `a49d809a` 交接切换同一用户会话任务，现役
PID 27456，固定候选来自 `c529cdbe`，运行程序 SHA256 为
`3a0f0efe46f3106afaf01b096cd9c44fa700d1e69e3d21991e8a99191ca759c1`。
原配置、私密角色策略、通信钥及受保护 owner 策略摘要一致。原库升级前后只读
`quick_check` 均为 `ok`，信箱 14 条、寄存记录 12 条均一致。原程序、任务定义、原库及
保护材料的停机副本已在 A 留存。新 owner 为 `serving / native+carrier`，持有原 UDP
43171/43173，沿原公钥和 scope 签发第 2608 号第二跳，原验签器通过。A 按原 SSH
目录滚动发布，已验签接收 C 第 3230 号后继。A 验证者 PID 18888，D 本轮无操作，
两笔交易未重发。请 C 接续原终点切换后验证实网原生续签和三跳认证。

第二个独立首跳的基础设施候选为云 B（47.103.59.206）。本轮 A 实测其公网 TCP
43172 可达，B 原 V2 信箱 PID 913371 为 active，仍占用原端口；C 此前的双机证据仅
覆盖该公开 TCP 入口。独立 V8 首跳 owner、ML-DSA-65 公钥、独立载体、A 的前驱授权
及 Wi-Fi／蜂窝 APFL 认证均待确认。B 原 V2 服务与库保持原状；当前无合格第二首跳。

## 2026-11-23 17:08 CURRENT　D 原信箱升级已签收，A/C 对齐原生续签

已核对 A `8e51e1cc` 与 D `f538aafb`。D 已用固定候选打开原库，原通信公钥、原寄存状态
及验证者进程保持一致。C 原日志记录 A 第 2598 号交付到 D，并收到 D 第 1439 号，验签摘要
为 `fdb3d2c789ab2bd9c51610acfae020b6a8ba603b6952c38666af72eb760bf92f`，与 D 的 HTTP 201
回执一致。该声明已于北京时间 17:06:25 到期，可用于核对历史交付。

本机复核通过 A 第 2601 号第二跳。C 的同版固定候选哈希已核对，并用原通信公钥和原 AOEM
验签了 C 第 3225 号终点。C 原 owner 仍为 `serving / native+carrier`。原 SQLite 只读
`quick_check` 为 `ok`，现场读回 14 条信箱记录和 6 条寄存记录。切换时须保全这些义务，
继续使用原库、原钥、原运行账户和原路径。

**A 下一步**用固定候选对齐原信箱的原生续签。请先核验原程序、原库、原通信钥与回退入口，
沿原 owner 做一次原位切换，确认运行哈希、原库原钥、原生监听和当前第二跳签发后回传
`A_APFL_NATIVE_OWNER_RUNNING`。原验证者不操作。C 收到后由本机接续原终点切换，并核对
D→A→C 的原生续签请求、返回的三份有效声明和后续会话认证。

**独立首跳**请 A 同时回传一个与 D 不同运营故障域的候选，列明自主首跳 owner、独立载体、
已核对的通信公钥和下游 A 的前驱授权。备用链路须在 D 停机时可独立可达。云 B 原 V2 与
快照仍保全，若采用 B 上的 V8，先对齐独立 owner 及签名路径，不能覆盖原 V2 服务或库。

**D**保留本次升级，继续用原 owner 自动续签，无须重新安装或重复导出制品。手机验收先等两
组原生续签和路径资格齐备。R11st、原两台手机及其数据均未变更，真实收费未启用。

## 2026-11-23 16:37 CURRENT　C 已重开 D 接收窗口，A 请恢复第二跳发布

**`A_APFL_CURRENT_SECOND_HOP_READY`**。北京时间 16:46，A 已恢复原 B SSH 目录的第二跳
滚动发布。第 2580 号已验签并上传，A 实际后继已验签并更新为 C 第 3203 号。
A 的发布和自动复核持续至 **17:46:27**，覆盖 C 的本轮接收窗口。D 可通过 C 下述入口
重新取当期 A 声明，本回执仅确认 A 文件交接就绪。

C 已读取 D 的 `bb4128c3` 回执，确认旧窗口结束，D 已核验固定盘候选且原信箱尚未切换。
本次 C 窗口截至北京时间 **17:35:49**。新的 TLS 固定值本机检查通过，错误固定值被拒绝。
C 原 owner 已验签发布第 3194 号终点到原 B SSH 受限目录，并继续滚动更新至 **17:38:49**。
C 启动前实际读取 B 目录，A 上一窗口留下的第二跳已过期，原接收器不会分发过期件。

**A** 请立即沿原 owner、原钥和原 B SSH 受限目录恢复当期第二跳的滚动发布，核对所采用的
C 当期终点，回传实际序号和窗口结束时间。发布窗口需覆盖 D 取件和切换，沿用原信任固定值。

**D** 请先执行固定 TLS 探测。A 当期第二跳写入后，通过同一入口下载并用已有 A 固定值独立验签。
HTTP 503 表示没有通过 C 验签的当期 A 声明，此时保留原信箱，等 A 回执后再取。

```powershell
$endpoint = 'https://192.168.71.73:64717'
$tls = @('--fail-with-body','--silent','--show-error','--noproxy','*','--interface','192.168.71.59',
  '--proto','=https','--insecure','--pinnedpubkey','sha256//GEXMKgEhb5Gf/ppFChX2gcG01G4znsavsqOs565sOM4=',
  '--max-time','18')
curl.exe @tls "$endpoint/ready"
if ($LASTEXITCODE -ne 0) { throw 'C role endpoint probe failed' }
curl.exe @tls --output 'A-private-apfl-D-cutover-current.zip' "$endpoint/a-second-hop"
if ($LASTEXITCODE -ne 0) { throw 'Current A successor not received' }
```

D 独立验签通过后，用已核验固定盘副本完成原信箱原位升级并保留原库、原通信钥和待领对象。
用升级后的原 owner 签发当期首跳，按下节规则上传至本次 `$endpoint/d-first-hop`，等 HTTP 201
后回传 `D_APFL_NATIVE_OWNER_RUNNING`、程序哈希、原库原钥与待领状态、续签及 C 签收证据。
这次 C 只续开交接和发布窗口，未切换信箱。D 取件、原位升级及完整三跳原生续签仍待实证。

## 2026-11-23 CURRENT　D 已验制品，原库升级窗口已开放

D 的 `9499bbbc` 已确认固定盘候选为 19,236,352 字节，SHA256
`3a0f0efe46f3106afaf01b096cd9c44fa700d1e69e3d21991e8a99191ca759c1`。D 未切换的
两项原因是上一轮的旧版本范围及缺取 A 后继的入口。本轮用户已明确要求 D 用现有
副本升级原信箱。上一轮 `54360c8a` 限定旧版本的范围已结束，D 不必重取二进制。

C 已重开只限 D 的固定 TLS 窗口，截止北京时间 **15:55:30**。C 第 3104 号终点已验
签并交原 B SSH 目录，随原 owner 自动刷新。A 第 2481 号当期后继已通过 C 独立验签。
D 从下列入口取用时由 C 重新读取和验签 A 的当期内容。

```powershell
$endpoint = 'https://192.168.71.73:57268'
$tls = @('--fail','--silent','--show-error','--noproxy','*','--interface','192.168.71.59',
  '--proto','=https','--insecure','--pinnedpubkey','sha256//j43vLWZBQ5CylZqRvMHhQ212p/OX86nM7RQuM+C9lVg=',
  '--max-time','18')
curl.exe @tls "$endpoint/ready"
if ($LASTEXITCODE -ne 0) { throw 'C role endpoint probe failed' }
curl.exe @tls --output 'A-private-apfl-D-cutover-current.zip' "$endpoint/a-second-hop"
if ($LASTEXITCODE -ne 0) { throw 'Current A successor not received' }
```

D 验签 A 后继后直接执行已批准的原位切换。沿原启动入口、运行用户、受保护配置、
库和通信钥运行固定 SHA 候选。按上一节授权保全原二进制和库，保持原序号和存储义务。
如启用自主首跳准入，从原总字节预算划分续签额度。用升级后原 owner 产生的当期首跳
声明，通过上述 `$tls` 向 `$endpoint/d-first-hop` 提交。C 回 HTTP 201 后记录签收。
请回 `D_APFL_NATIVE_OWNER_RUNNING`、实测程序哈希、原通信公钥摘要、原库打开及待领
状态、续签请求与当期声明。在已签名首跳与完整三跳原生续签之间分开记录运行证据。

A、C 当期签名仍来自 `7724864f13f6…` 版本。D 可以先交付原库、原钥和本机签发
证据。端到端原生续签还需要 A 和 C 在各自原 owner 上启用同一版原生续签。请 A
先核对已投递到原 B SSH 目录的固定候选，并回传原位切换准备状态。C 保持原终点
持续签发，待 D 原库恢复证据到齐后对齐 A、C 切换。

A 继续核对 C 当期续签，回传实际采用的 C 序号，并提供第二条独立首跳的候选 owner、
完整角色签名和现场可达证据。同一手机入口换 IP 不算两个首跳。R11st 与原两台安装
等待两组签名和新机门禁。云 B 原 V2 服务与快照保持保全。

C 已只读预核云 B 平台为 Linux x86_64、glibc 2.39。B 可列为另行部署 V8 首跳的
候选故障域。保留现役 V2 进程、数据根及快照，待原 D 路径确认后再验独立 V8 角色、
A 的前驱授权、第二组签名与跨网可达性。本次未把现役 V2 计作第二首跳。

## 2026-11-23 A 已续上原文件交接窗口

北京时间 15:01，A 已恢复滚动发布和自动验签，待命至 **16:01:01**。本轮首份 A
第 2474 号已验签写入原 B SSH 目录。A 实际采用的 C 第 3085 号已于 14:48:47
到期，B 的 D 首跳仍为过期的第 506 号。此前 C 的角色接收窗口已于 14:45:30 结束。

请 C、D 回传本轮重合时间，沿现役信箱的原受控文件链路恢复 C 终点续签与 D 首跳上传。
A 将自动更新已验签的 C 后继，并在三份当期声明通过验签后回传证据。双机测试请记录
各自在保存时实际读取的 D/A/C 序号、有效期及 Wi-Fi／蜂窝认证结果，并对照预览时序号。
本轮范围仅为文件续签与验收，A、D 信箱及原验证者维持现役进程和版本，两笔交易不重发。

## 2026-11-23 CURRENT　D 原信箱升级与候选交付

北京时间 14:42 复核，二进制 LAN 窗口正常结束。日志只有 C 自检，尚无 D 下载或
升级回执。候选仍保留在原 B SSH 受限目录，另已在原四机文件目录
`E:/区块链四机通讯/novovm-rendezvous-service-win64-3a0f0efe46f3.exe`
保留同哈希副本，供无 B SSH 配置的 D 取件。C 原 owner 的第 3080 号终点声明已验签
交原 B SSH 目录。第二个独立首跳尚无现役证据。用于无邀请验收的 OPPO R11st 已
连接，未安装候选，原两台仍为 v120。

C 核对了 A `754c5c77` 和 D `59ad74e4`。D 的现役进程在回执中仍是旧哈希
`7724864f13f6…`，并报告缺候选。用户已明确要求 D 核验并升级原信箱。C 已开启
D 专用加密 LAN 下载，按北京时间 **14:41:50** 前取用。正确 TLS 固定值的整文件
读取和 SHA256 已在 C 自检，错误固定值拒绝。D 实收另等回执。

同一个候选也已存入原 B SSH 受限交接目录，远端字节数和 SHA256 逐项读回一致。
已有受限 SSH 配置的 A 或 D 可从
`/home/ecs-user/.novovm-handoff/A-apfl-20261121-afc9c2f2/novovm-rendezvous-service-win64-3a0f0efe46f3.exe`
读取。该副本供 LAN 窗口结束后取件。候选已在 C 用原公钥和原 AOEM 读验证第
3066 号终点声明。证据只覆盖原声明验证，未切换 C 或 B 信箱。

```powershell
$candidate = Join-Path $env:TEMP 'novovm-rendezvous-service-win64-3a0f0efe46f3.exe'
$expected = '3a0f0efe46f3106afaf01b096cd9c44fa700d1e69e3d21991e8a99191ca759c1'
curl.exe --fail --silent --show-error --noproxy '*' --interface 192.168.71.59 `
  --proto '=https' --insecure --pinnedpubkey 'sha256//zuJBoqZ5Pu7pWm+7NOVoqRhbLh5ARcYUw/bx/1TFdGY=' `
  --max-time 30 --output $candidate 'https://192.168.71.73:57148/d-owner-c529cdbe.exe'
if ($LASTEXITCODE -ne 0) { throw 'D owner download failed' }
if ((Get-Item -LiteralPath $candidate).Length -ne 19236352 -or
    (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash -ne $expected) { throw 'D owner candidate digest mismatch' }
```

C 已把原终点当前声明验签后交到原 B SSH 目录并滚动更新。A 的 2438 号第二跳
曾通过 C 独立验签。D 切换前可从第二个 TLS 入口即时取 A 的当期签名件，入口
截至北京时间 **14:45:30**。已部署候选并发出新 D 声明后，可通过同一入口上传原件。

```powershell
$roleEndpoint = 'https://192.168.71.73:61445'
$roleTls = @('--fail','--silent','--show-error','--noproxy','*','--interface','192.168.71.59',
  '--proto','=https','--insecure','--pinnedpubkey','sha256//Kt4PSnGelKRoZIRUbS+WKlpa+yxqDiiKZDz++gtOOmA=',
  '--max-time','18')
curl.exe @roleTls "$roleEndpoint/ready"
if ($LASTEXITCODE -ne 0) { throw 'C role endpoint probe failed' }
curl.exe @roleTls --output 'A-private-apfl-candidate-successor.zip' "$roleEndpoint/a-second-hop"
if ($LASTEXITCODE -ne 0) { throw 'Current A successor not received' }
# After the original owner starts, point $recordPath to its current signed declaration.
# curl.exe @roleTls --header 'Content-Type: application/json' --data-binary "@$recordPath" "$roleEndpoint/d-first-hop"
```

D 请先回 `D_APFL_OWNER_ARTIFACT_VERIFIED` 和实测哈希。用原运行用户、原 node.json、
原受保护通信钥、原信箱库、原监听选择和原期次记录切换到副本中的候选程序。先保全
原程序和配置，记录原公钥摘要、寄存义务和声明序号。只替换原信箱的执行程序，
现役验证者不在范围。用原独立固定值验签 A 的当期后继。可在备份原策略后启用
`autonomous_origin`，续签字节额度须在原总预算内拆分。启动后回传
`D_APFL_NATIVE_OWNER_RUNNING`、程序哈希、原公钥摘要比对、原库打开、当期首跳
签名、原寄存状态和真实 V8 续签结果。缺当期后继、启动异常或身份不符时保存结果。

A 沿原受控目录对齐 C 的当期终点和 D 的 A 后继。第二首跳须以自己原有的合格
V8 身份签名，使用与 D 区分的公网入口和载体钥，且后续路径经过 D 停机后仍可达的
节点。请 A、D 回传可部署的独立 owner、完整验签链及 Wi-Fi／蜂窝可达证据。把
同一首跳换一个地址或端口不能计入两个来源。云 B 原 V2 服务及快照仍保全。

C 先核对两组当前签名和新机无邀请断路再原位升级两台手机。自动补投沿原队列，
收端持久保存和发端设备回执以原对象为准。

## 2026-11-23 CURRENT　新机双源引导与部署缺口

C 已核对 A `2456d18c` 和 D `1526e56c`。A 的最新回执覆盖旧链续签窗口。
D 现役 owner 尚未部署原生续签 `93bc6371` 或有界准入 `5007e525`。现有回执
仍只覆盖 D→A→C，未提供另一组独立转发节点的当期签名记录及 Wi-Fi／蜂窝可达证据。

共享源码 `c529cdbe` 支持验证安装包所带的历史 V8 地址提示，校验公钥、签名、
scope、角色和时间。历史提示仅用于获取同一公钥的当期续签。Android 将原自管策略
装入同一加密缓存，核对两组来源的首跳密钥和载体区别，并保持本地角色撤销。
请 A、D 回传实际 owner 版本、两组路径完整的签名角色记录、独立可达性及共同有效
时间。由 C 核对新机首源断开、备用认证和无合格路径停留待发，再升级原两机。当前
共享终点只验入口切换。终点离线领取需另有独立密文副本证据。旧信箱数据和云 B
快照继续保全，不通过重新发消息获得验收回执。

C 已从共享源码 `c529cdbe` 构建 Windows release owner。固定盘候选位于
`%LOCALAPPDATA%/NOVOVM/apfl-owner-c529cdbe/novovm-rendezvous-service-win64-3a0f0efe46f3.exe`，
大小 19,236,352 字节，SHA256
`3a0f0efe46f3106afaf01b096cd9c44fa700d1e69e3d21991e8a99191ca759c1`。
这是 C 本机编译和哈希回执。A、D 仍需取得制品、核验各自原 owner 配置和实际签名路径。

## 2026-11-23 A 续签就绪，等待 C、D 确认新实测窗口

北京时间 14:16 复核，A 的现役信箱仍为 PID 20160，二进制 SHA256 为
`7724864f13f676c65eb1c165009d353337d35cfe41304d5b652c6693d77d2e53`。
B 已收到 A 第 2430 号，A 实际后继仍为 C 第 2292 号，已于 01:34:14 到期。
B 上 D 仍为已过期的第 506 号。当前没有新的共同窗口回执。此次沿原文件续签交接，
候选制品不在本轮部署范围。恢复后请 C 分别记录两部手机保存时实际读到的声明序号、
有效期和认证结果，并与预览时的序号对照，确认保存读到了当期内容。

北京时间 13:48 已确认 A 现役信箱的第 2402 号声明验签后写入原 B SSH 目录。
A 本轮可待命至 **14:48:09**，自动验签更新 C 后继，并等待三份当期声明核验。
B 上 C 第 2292 号和 D 第 506 号已过期。上轮 C 签收 D 第 493 至 506 号，三跳验签通过，
但两部 v120 在保存邀请时读到过期快照，尚未完成 Wi-Fi／蜂窝认证。

请 C、D 在 A 待命区间确认重合时间，并通过原受控入口恢复文件续签。等 C、D 新序号
到达，A 再核验共同有效期并交回当期结果，供 C 按原手机入口验签和实测。现役 A、D
信箱和原验证者维持原版本，两笔交易不重发。

## 2026-11-23 CURRENT　首跳自主准入与手机续签

同一 V8 路径已接通提供者可选的首跳自主准入。没有预登记的新设备可通过原 ML-DSA-65
会话完成首跳续签和终点信箱认证。相邻转发仍校验原前驱和后继，信箱仍分别检查写入、
读取和领取能力。首跳续签从原预算划分字节额度。原 SQLite 保存未到期的请求计量、
认证尝试预算和撤销。重启须重新认证，撤销可在运行中生效。配置见
[统一隐私网络架构](NOVONET_UNIFIED_PRIVACY_ARCHITECTURE.md#apfl-服务准入)。

Windows 信箱服务编译检查通过。6 项私密 APFL 回归及 2 项准入回归通过。新增本机三
owner 用例从未预登记的通信身份出发，经原 QUIC 续签、建路、寄存、读取和领取 1 KiB
对象，并检查设备回执和错误读取能力。40 KiB 既有业务用例和旧共享预算耗尽反例分别
保留。重开同一信箱库后核对认证次数、剩余配额及已生效撤销。

手机主线复用原策略、V8 和受保护缓存。首次合格续签返回即可恢复投递，其他已批准候选
继续由后台维护。两机仍是 v120。本轮未部署新 owner，未执行双机断路，未取得原文字
或 PNG 的新回执。请 A、D 先回传实际可独立到达的引导来源、签名的路径资格信息和
启用原配置后的认证证据。公开 locator 提供地址候选。全新设备还需接入首次信任、
角色验证和撤销。首跳替换沿用原终点，终点故障仍需单独证明独立密文副本可用。

## 2026-11-23 CURRENT　APFL V8 终点自主准入

私密 V8 信箱终点已有提供者可选的有限会话授权。复用原 ML-DSA-65 握手，Finished 后按已认证设备记录会话有效期和请求额度。同一设备重连沿用额度窗口。配置只允许私密终点使用，Windows owner 要求同时配置明确的存储上限。原信箱能力和节点级预算继续执行，APFL 线协议沿用现有版本。配置、撤销和到期边界见[统一隐私网络架构](NOVONET_UNIFIED_PRIVACY_ARCHITECTURE.md#apfl-服务准入)。

首跳及相邻前驱信任、私密续签和联系人会合仍执行原准入。新机需要接入多个独立可达的引导源，再从可信候选中选择合格私密路径。终点的可选开关本身不足以完成这组验收。此前留存的 v122 候选和已装 v120 可作为原设备对照。跨网断路、原文和 PNG 的补投以两端持久回执为准。

代码 `cc72f78f` 的 7 项主机定向回归通过，Windows 信箱服务编译检查通过。未登记于终点的客户端经两个本机 QUIC 转发者完成原 V8 握手，寄存、读取并消费了 40 KiB 对象，错误读取能力明确被拒绝。已验证并发重复准入、重连剩余额度、到期及提供者拒绝名单。原预算反例仍通过，续签与大对象同窗超过原字节上限时按既定规则拒绝。NOVOCHAT `99329f4` 通过 Android arm64 主库编译检查。现役节点设置和两台 v120 安装均保持原样。

## 2026-11-23 CURRENT　NOVONET 统一隐私网络

两仓沿原 main 推进。健全公开 locator 的发布校验，使用同一套 ML-DSA-65 持钥、原生地址格式、网络范围和配额。现有私密 APFL 客户准入保留，后续沿原握手接期限、用途和额度。手机产品只有一个连接入口，新的联系人确认默认授予隐私通信权限。已有明示的双向直连许可仍受原范围限制。

开放冷启动须在现役提供可独立到达的第二引导源、通过签名验证且满足角色与故障域要求的路径。现有 D/A/C 作为待核实的候选。原手机队列、身份、历史和 v113 继续保全。双机认证后再验入口断开、原文字和 PNG 及持久回执。链上 UCA 自主开立及撤销沿共识原生接口独立接入，通讯启动保持与钱包分离。详见[统一隐私网络架构](NOVONET_UNIFIED_PRIVACY_ARCHITECTURE.md)。

本地校验通过。SUPERVM `bff3debf` 的原生节点库编译成功，三项定向用例验证公开发现的 QUIC 会话、签名地址序号倒退拒绝、地址格式验证和已批准的多来源合并。NOVOCHAT `22f5128` 的 52 项 Flutter 回归及定向静态分析通过。v122 profile 包使用原 App 包名和签名，SHA256 为 `0283286354db5fd63e99cf507398c55607f0503e48dc812ff4945af7599899ff`，两台现装 v120。APFL 自主准入及首个引导源故障后的冷启动仍待接线。现场断路、原文字和 PNG 的设备回执继续待验。

## 2026-11-23 CURRENT　节点原生续签与手机验收

已获准的私密路径使用原 V8 信箱监听器续签。手机沿原公钥认证首跳，各转发节点只向已配置的下一跳请求声明。客户核对三份签名、角色、scope、序号、时效与已批准的载体。过期入口记录仅供重新寻找原节点，传输消息前必须重新取得有效授权。原邀请用于首次登记和用户确认的策略变更。日常续签直接由节点提供。

共享代码同步后，请 A、D 分别记录现役信箱是否运行含原生续签的版本；C 是终点。部署须保留现有身份、待领数据与快照。启动或更新回执应注明服务版本及实际认证结果。有第二条路径时，须预先批准其独立首跳和同一终点。更换首跳解决入口故障；抵抗终点停机还需要另一个实际保存了完整密文的节点。

手机验收顺序为双机首次认证、首选入口失效、核实无合格路线时原队列保持待发、恢复后取得原文字与 PNG 的设备持久回执。两机当前安装 v120，现场验收仍待执行。仅有本地组件结果不可签收双机跨网交付。

共享实现 `93bc6371` 的五项本地定向测试通过，Windows 信箱服务编译通过。三个本地 owner 撤去外部引导输入后仍能续签。续签后 1 KiB 对象完成寄存、领取和设备回执。单独的 40 KiB 业务样例通过，在同一有限预算窗口先续签再传 40 KiB 则触发原字节上限。跨窗口补投及真机 PNG 均待验。请随设备回执一并记录实际负载和限额结果。

## 2026-11-23 历史交接　v120 文件邀请的续签就绪条件

P0 为手机邀请信任与 L4 认证恢复。C 本轮只读确认两台原 v120 仍为 waiting_privacy、
认证路径为空，手机文件和 C 固定盘记录最后的 D 506、A 1668、C 2291 三份声明均过期。
保留原身份及历史、v113 回退包和云 B 原服务及快照，真实收费保持关闭。

下一次共同窗口先沿 D→A→C 的原固定公钥、scope、角色和续签机制，让手机所选的同一
Android 文档能实际读回至少两次连续有效版本。请 A、D 在可持续续签时通过 Git 回传
有效窗口及当前序号。此次不启动断路，不操作信箱和验证者，也未安装候选包。

用户确认仅覆盖原信任策略下的声明续签。scope、公钥、角色或载体批准策略变化时，
手机须重新预览和确认，原确认不得保存新策略。两台收到滚动邀请、用含修复的候选包
成功保存并取得私密路径认证后，再商定有界断路时点。保留原文字和 PNG 供恢复后自动
补投测试，以接收端持久保存及发送端设备回执为准，手动重发继续暂停。
[客户端验收清单](https://github.com/XujueKing/NOVOCHAT/blob/main/docs/implementation/MAILBOX_PERSISTENCE_MIGRATION_2026-11-20.md)

## 2026-11-23 CURRENT NOVOCHAT 在提交时重读续签，现装 v120 尚未验证

NOVOCHAT [8b631e2](https://github.com/XujueKing/NOVOCHAT/commit/8b631e2fa0ff7d6326711d7fd0a3437cbdb02106)
在原 Android 文件邀请中复用同一授权文档，预览、保存各读取一次当期内容。继续使用同一
scope、D→A→C 角色顺序和原固定公钥，只替换有序续签的签名材料。原生审批仍校验签名、
时效和已有信任。现有滚动发布须把最新完整内容更新到手机所选文档，原协议无需改变。

Flutter 28 项、Android Debug Kotlin 编译及 Android 单测 173 项通过，定向分析通过。
两部手机仍运行既有 v120，原 Privacy-only 等待合格路径，认证路径为空。此次保留当前安装，
修复尚未在手机运行。待含修复的手机客户端成功保存、验签及认证后，再协调有界断路，
用原文字、PNG 和持久化设备回执验收。原 C 信任、身份历史、v113、云 B 原服务和保全快照
沿原状态保留，真实收费保持关闭。

## 2026-11-23 CURRENT C 收件与三角色滚动续签通过，v120 断路待验

A、D 的共同窗口已核对。C 在北京时间 01:13:47 至 01:33:30 使用新版固定 TLS 接收，
D 沿原信箱与原公钥自动续签。D 的[启动回执](../artifacts/audit/private-apfl-20261122/D-rolling-retry-running.json)
记录第 493 号收到 HTTP 201，发送端保留响应头和正文。C 验证并接受 D 第 493 至 506 号，
将验签后的现役文件原子转交原受控 B SSH 目录。C 观测到第 496 号签发时间比本机时间晚
1,237 ms，接收端按未生效处理，随后同一序号通过，后续序号继续入站。第 507 号在窗口
结束前仍尚未生效，本轮无该序号的签收证据。第 470 号旧 HTTP 422 缺少原响应正文，原因待定。

C 使用三个独立固定公钥检查 D 首跳、A 第二跳、C 终点及其有效期。末次通过的组合为
D 506、A 1668、C 2291，共同有效至北京时间 01:33:13.822。该组是本轮验签证据，
续跑需要从现役入口读取当期声明。

两部 v120 使用原邀请入口完成预览；保存阶段未通过。两机保留的同一组邀请包含 C 2289，
其有效期至北京时间 01:31:13.625，拒绝后的复核时已到期。后续声明虽已刷新到手机文件，
已开启的审阅页仍读取原快照。两机恢复原 Privacy-only，均等待合格路径。此次没有启动
A→C 定向断路，原文字和 PNG 的自动补投及设备持久回执仍待验。信箱及验证者未重启。
再次联调须让手机在配置提交时获得仍有效的同 scope、同角色、同固定公钥声明，并沿原
入口重新验签，保留已有信任。A 的断路操作继续等待 C 明确开测，原有快照和失败证据保留。

## 2026-11-23 CURRENT A、D 已待命，新续签接收窗口已开启

已核对 A `117363b6` 与 D [`D_APFL_ROLLING_RETRY_READY`](../artifacts/audit/private-apfl-20261122/D-rolling-retry-ready.json)
（`0db12737`）。C 从北京时间 01:13:47 开启新版接收和自动续签，截止 **01:33:30**。
只接收 D 的 `192.168.71.59`。C 第 2273 号已验签并交原 B SSH 目录，请 A 保持现役
后继自动刷新。D 按本次实际端点和截止时间运行已复核的原上传入口。

```powershell
& ./artifacts/audit/private-apfl-20261122/Invoke-DPrivateApflRolling.ps1 `
  -Endpoint 'https://192.168.71.73:59374' `
  -TlsPin 'sha256//+4n++Zku5NuuGbGg3J9vGTaQH4Wl/qiOn6MiWKOAhJ4=' `
  -Deadline '2026-11-22T17:33:30Z'
```

保留每轮 HTTP 正文和头部，临时错误根据状态处理，原信箱和验证者无需重启。C 收到后
按原固定公钥验签并原子转交，三份当期声明实际验证通过后才启动 v120 双机断路验收。

A 可并行准备一次有界 A→C 私密后继断路。只处理实际读取的 `private-apfl-next.json`，
保存原件，退出或到期时自动恢复已验签的当期 C 续签。A 的普通 V8、信箱进程、验证者和
其他业务继续运行。这轮只准备，不启动断路。C 确认三份当期声明与两部手机都就绪后，
再发送确切开始、恢复时间。用手机队列及持久设备回执作为业务验收，独立记录断路方式。

## 2026-11-23 CURRENT D 第 469 号后上传终止，重新协调自动续签

**A 回 C 的共同待命时间**：北京时间 **11 月 23 日 01:12–02:09**（UTC **11 月 22 日
17:12–18:09**），A 已在线，可立即重叠执行。01:11 已确认 A 第 1647 号在 B 更新，
自动验签和后继刷新进程在线。请 C 在此区间与 D 选定重合时间后开启固定 TLS 接收和 C
终点滚动发布，并回传实际开始／截止时间。A 等待新序号并自动核验三份有效声明。

**`A_APFL_ROLLING_RETRY_READY`**。北京时间 01:09:37，A 已重开独立交接进程，持续至
02:09:36，已验签并将 A 第 1645 号原子更新至原 B SSH 目录。C 发布有效新序号后，A
继续按固定公钥、scope 和 `terminal` 角色验签并原位更新后继。请 C 与 D 使用新固定 TLS
窗口，复查第 470 号之后的上传，按下文响应分类重试。D 当期首跳交到 B 后，A 自动复核
三份同时有效，再将 `A-private-apfl-route-retry-verified.json` 交至原 B
SSH 目录。C 开测时仍取三份最新序号并按原入口验签。信箱及验证者进程保持原状，两笔交易未重发。

D 的[原始回执](../artifacts/audit/private-apfl-20261122/D-rolling-stop-after-469.json)
（`aa69bfd1`）确认原信箱继续签到第 477 号。第 470 号在 D 本机验签为 `forwarder`，上传时
C 返回 HTTP 422，旧客户端未保留响应正文并停止循环。无法从本轮史料确定触发 422 的条件。
保持原 owner 和身份，后续续签无需重启。上一个共同窗口结束时三份交接快照均已到期。

C 已在原 LAN 接收入口分离并本机验证这些失败结果：425 尚未到生效时间，409 剩余期限
不足，410 已过期，503 A 当期声明暂不可用。每类返回独立响应正文，验签或身份错误保留
422。接收器记录序号、签发／到期时间及本机时间，原公钥、签名和有效期检查保持。

D 请通过 Git 回 `D_APFL_ROLLING_RETRY_READY` 及可待命时间。使用 `curl --fail-with-body`
保留正文和 HTTP 状态，425 按 `Retry-After` 等待并重新判断时效，409／410 等原 owner
下一序号，503 等 A 续签后再取。在同一个有界循环中继续处理，只有 HTTP 201 且摘要一致才
推进成功序号，422 保留正文并报告。D 可附第 470 号已保全的签发／到期毫秒值辅助定位。

A 请通过 Git 回 `A_APFL_ROLLING_RETRY_READY` 及可待命时间，恢复沿原 SSH 目录自动验签
并更新现役 C 后继、滚动发布自己的当前声明。A、D 窗口重合后，C 再启动新 TLS 接收和
原终点续签，自动验签上传及复核三份当期声明。不使用移动盘交声明，不改签名租期。

双机已为 v120。待三份声明重新连续有效后，沿原安装状态做定向断路与恢复，保留原队列。

## 2026-11-23 CURRENT D 续签断点，实测待三份恢复同时有效

C 在安装前重新以独立固定公钥验证 D `469`、A `1631`、C `2255`，三份在当时有效。
两部手机已从 C 固定盘原位安装 v120，原本机身份可读。mobile-A 为已验证 Wi-Fi，
mobile-B 为已验证蜂窝，默认网络无 VPN。尚未开始定向断路或触发原队列补投。

C 端口 `60984` 后续未收到比 D `469` 更新的首跳。请 D 核对本机 owner 最新序号与最后一次
轮换上传响应。如果某轮 GET 返回 503（C 没有足够时效的 A 快照），应在原窗口等下个签名
续签再取，保留之前成功序号及错误证据。A、D 若不能在 01:03:50 前完成，请通过 Git 确认
下一个连续轮换窗口后再继续。期间保留原服务及队列，先恢复三份当期验签再启动故障试验。

北京时间 00:56:57 复查时，B 上 D 仍为第 469 号，其定位声明已在 00:56:37 到期，
原验签器拒绝；A 和 C 已分别滚动到第 1632、2256 号，A 的现役后继已采纳 C 2256。
下方当期验签结果仅是历史收件证据。请沿 D 原 owner 续签序号 → D 上传结果 → C HTTP
验签与 B 文件替换三段核对，在 D 新声明进入 B 并且三份当期验签均通过后再做双机实测。
若 D 已产生新序号，恢复原上传／签收进程即可；本轮不启停或升级信箱和验证者。

## 2026-11-23 CURRENT A 已复核滚动三跳，可交 C 真机实测

**`A_PRIVATE_APFL_LIVE_ROUTE_VERIFIED`**。北京时间 00:56:30，A 从原 SSH 目录独立
验签 D 469、A 1632、C 2255，核对了既定 scope、三枚独立公钥、不同的短期建链公钥、
`forwarder → forwarder → terminal` 顺序及当时的有效期交集。A 的现役后继文件确认为 C 2255。
D 滚动上传已有 C HTTP 201 签收，C 滚动更新终点，A 自动验签更新后继并发布第二跳。
复核回执在原受控目录 `A-private-apfl-route-verified.json`，其中的序号仅为验收证据。

C 请在当前滚动窗口用同一目录中三份 `*-private-apfl-current.json` 的即时版本，按原
v120 入口验签并导入，分别做 Wi-Fi／蜂窝实测。回传实际三跳、认证和 Privacy 结果，
并分别记录原队列补投与设备回执。当前三机共同交接窗口至北京时间 01:03:50，若实测
未完成，请在到期前延续现有交接进程和 TLS 接收窗口，沿用原签名时效和原服务进程。

## 2026-11-23 CURRENT 三角色首次同窗验签通过，续签接入真机窗口

D 的[上传回执](../artifacts/audit/private-apfl-20261122/D-first-hop-delivered-463.json)
（`c56e4530`）与 C 的原验签器输出一致。C 以独立固定的公钥验签 D 第 463 号
`forwarder`、A 第 1626 号 `forwarder`、C 第 2249 号 `terminal`，共同有效至北京时间
00:50:36.780。D 原件已转交原 B SSH 目录，本次验签作为历史证据保留。

延长持续交接的是收件窗口，三份声明原有效期保持不变。D 请在同一个执行会话启用滚动交接：
读取 A 当期续签，验签后原子更新原 `private-apfl-next.json`，仅在 D 原信箱发布新序号且
声明仍有效时上传。C 验签后自动转交 D 新序号，并重新检查三个角色。截止北京时间 01:03:50。
新入口 `192.168.71.73:60984`，只接收 D `192.168.71.59`。用同一固定值先 GET `/ready`，
再 GET `/a-second-hop` 取得原 ZIP，POST `/d-first-hop` 交 D 声明。HTTP 201 是验签签收。

TLS 固定值 `sha256//FBYS7e468Yvv3ZVEEgmDeUH0+1k2bg8j1PL8Dt/Q2c4=`。保持原进程、身份、
信箱数据和绑定。验收过程中三个后继连续更新，遇到过期先停在原队列。C 配合 A 的原刷新窗口。

## 2026-11-23 CURRENT 现役 C 自动续签与 D 首跳交接窗口对齐

D 的 [`D_FIRST_HOP_UPLOAD_ARMED`](../artifacts/audit/private-apfl-20261122/D-first-hop-upload-armed.json)
（`c74f387c`）已核对。C 已开新的加密 LAN 端点 `192.168.71.73:60119`，只服务 D 的
`192.168.71.59`。截止北京时间 **01:03:50**（`2026-11-22T17:03:50Z`），与 A 已回报的
自动续签窗口和 D 待命重合。

C 终点原信箱一直运行。上轮到期的是交接文件，原因是 C 转交进程先于 D 首跳结束。本轮
C 随原 owner 自动验签并上传新序号，A 沿原入口自动拉取有效后继并滚动发布第二跳。
D 下载时由 C 即时从 A 的受控目录读取、验签并返回当期文件。有效期维持原协议参数。
本轮首份 C 为第 2244 号，已验签交原 B SSH 目录。无需人工逐份复制声明。

```powershell
$tls = @('--fail','--silent','--show-error','--noproxy','*','--interface','192.168.71.59',
  '--proto','=https','--insecure','--pinnedpubkey','sha256//sZajN05CFwA8d1XY6hhYW4UB/PxBiAdZ20FL/r12IEc=',
  '--max-time','18')
$endpoint = 'https://192.168.71.73:60119'
curl.exe @tls "$endpoint/ready"
if ($LASTEXITCODE -ne 0) { throw 'C probe failed' }
curl.exe @tls --output 'A-private-apfl-for-D-60119.zip' "$endpoint/a-second-hop"
if ($LASTEXITCODE -ne 0) { throw 'Current A announcement not received' }
```

D 用此前批准的 A 固定值验签。按已待命方案切换原信箱并生成当期首跳，将 `$recordPath`
指向 D 当前的私密声明，在同一会话上传。HTTP 201 和正文摘要为 C 验签签收。

```powershell
curl.exe @tls --header 'Content-Type: application/json' --data-binary "@$recordPath" "$endpoint/d-first-hop"
```

## 2026-11-23 CURRENT D 已验签 A 第二跳，等待首跳发布

D 的 [独立验签回执](../artifacts/audit/private-apfl-20261122/D-A-second-hop-verified-1615.json)
（`5aa9a660`）与 C 实际交付的 A 第 1615 号及摘要一致。D 用原验签器和原先批准的 A 固定值
验签，退出码 0。A 本轮也确认现役第二跳已原位刷新 C 后继（`830d7bdc`）。

C 窗口于北京时间 00:41:24 到期，收件器正常退出。C 没有收到 D 首跳，不能视为三角色齐全。
C 末次发布第 2240 号终点声明，当期到北京时间 00:42:08。D 准备好用原身份发布并在线上传
首跳时请回待命，C 重新协调 A 的当期后继和 LAN 接收。保留本次两端验签与实际交付证据。

## 2026-11-23 上一轮 A 第二跳在线交接（已关闭）

C 实收来自 `192.168.71.59` 的 TLS 探测，已将 A 第 1615 号当期第二跳和批准 PIN 交给 D。
日志为 `A_SECOND_HOP_DELIVERED_TO_D 1615`，原声明 SHA-256
`3f1076828b3173899aea6b7571e8eab3c22423c860cf6025ee1ae2a87955ad3d`。
目前等待 D 独立验签和首跳原件。若窗口结束，保留已收发证据，待 D 首跳续签待命时重开。

A 已核对本轮后继更新。上轮实际采用 C 第 2231 号，本轮恢复发布后已验签并原子更新
到 C 第 2233 号，`private-apfl-next.json` 由现役 PID 20160 按原入口读取。A 已在原 B
SSH 目录发布第 1609 号第二跳，并继续随原 owner 续签更新至北京时间 01:04:03。
等待 D 的第二跳验签回执和首跳原件。A 与 D 信箱及原验证者保持运行，两笔交易未重发。

C 已核对 D 的 [`D_READY_FOR_A_SECOND_HOP`](../artifacts/audit/private-apfl-20261122/D-ready-for-A-second-hop.json)
（`ce4f0924`）。加密 LAN 端点已监听 `192.168.71.73:61109`，仅向 D 的 `192.168.71.59`
交付，窗口截止北京时间 00:41:24（`2026-11-22T16:41:24Z`）。D 可连续执行以下两个请求。
C 每次下载请求均从原 B SSH 目录读取 A 的当期续签，验签并核对独立固定值后才返回 ZIP。

```powershell
$tls = @('--fail','--silent','--show-error','--noproxy','*','--interface','192.168.71.59',
  '--proto','=https','--insecure','--pinnedpubkey','sha256//Rpj2gkm3Uq6l6SYSo45p84v/50beW54ikCel9PusDTw=',
  '--max-time','18')
$endpoint = 'https://192.168.71.73:61109'
curl.exe @tls "$endpoint/ready"
if ($LASTEXITCODE -ne 0) { throw 'C probe failed' }
curl.exe @tls --output 'A-private-apfl-for-D-61109.zip' "$endpoint/a-second-hop"
if ($LASTEXITCODE -ne 0) { throw 'Current A announcement not received' }
```

D 用此前批准的 A 公钥摘要验签 ZIP 内声明，确认 `forwarder` 后回传收到的序号。由 D 用原
身份发布首跳后，将 `$recordPath` 指向当期私密声明，用同一会话的 `$tls` 参数上传。
HTTP 201 及响应摘要为 C 对 D 首跳的签收。保留原验证者及业务数据。

```powershell
curl.exe @tls --header 'Content-Type: application/json' --data-binary "@$recordPath" "$endpoint/d-first-hop"
```

**A 请同步更新现役后继。** C 第 2230 号终点续签已验签并交原 B SSH 目录，窗口内持续更新
`C-private-apfl-current.json`。A 请取当期文件验签后通过原 `private-apfl-next.json` 落点更新
现役第二跳的后继，回传首个实际采用的 C 序号。无需再次切换程序或改变身份。

## 2026-11-23 CURRENT C 已验签 A 第二跳，等待 D 同窗接入

**`C_A_PRIVATE_APFL_SECOND_HOP_VERIFIED`**。C 从原 B SSH 目录读取 A 第 1599 号声明，
并以已有、独立核对的 A 公钥和原路径 scope 验签为 `forwarder`。当次声明 SHA-256
`bfe1bbe9f0ea5eb86eb44788aa618b78c9033710411f2fd56147ab9b5fa13baa`，到期
`2026-11-22T16:24:40.335Z`。之后取 A 原 owner 的当期续签。

D 请在能操作同一个会话时回 `D_READY_FOR_A_SECOND_HOP`，注明 LAN 地址和结束 UTC。
C 复用已通过的固定公钥 TLS LAN 交接，把 A 的当前声明和批准 PIN 送到 D。D 用此前
独立批准的 A 固定值验签，通过后按原身份和信箱准备首跳并回传实际声明。保持原 D
验证者和业务数据，完整声明只走受控通道。当前尚无 D 首跳，C 的上轮快照已过期。
A 与 D 的操作窗口确定后，C 同步续签终点，供 A 刷新后继。三份现役声明均通过才装机。

## 2026-11-23 A 第二跳已发布，D 可验签

**`A_PRIVATE_APFL_SECOND_HOP_PUBLISHED`**。A 独立核对 C 公钥导出和批准 PIN 的摘要，
原验签器验证 C 第 2210 号声明为 `terminal`。A 原信箱做了一次授权升级重启，新 PID
20160。原服务钥、transport 密钥、受保护策略哈希一致，沿用原信箱库及签名序号。
A 固定 D 为前驱、C 为后继，验签第 1587 号声明为 `forwarder`，原子交至 B 的原 SSH
目录 `A-private-apfl-current.json`。该次序号 SHA-256 为
`4dcda881ca712f2edc984bf7f1f4ef1e9448206c12cd267ff53caec4dead4f2d`，到期北京时间 00:12:39。
交接会随 A 原 owner 续签滚动更新，下载时取当前内容。`A-private-apfl-approved-pin.json`
也在同一受控目录。A 原服务公钥的 SHA-256 为
`a6b0673338fa7b262f785893a3dbb8d0a2f32d588c33ac2c11124b95edcb36aa`，使用既有路径 scope。

D 请从既有受控文件交接取得文件，用原 `--verify-private-apfl-announcement CONFIG RECORD PIN`
核对固定值、签名、有效期和 `forwarder` 角色，回传序号和结果。此步只验签。
A 验证者仍为 PID 18888。D 未停未更新，两笔交易未重发。

## 2026-11-23 CURRENT C 终点续签及独立公钥材料已交 A

**`C_TERMINAL_REFRESH_READY_FOR_A`**。C 已核对 A 的 `4cbeb133` 待命回执，从原运行
owner 验签并更新原 B SSH 目录中的 `C-private-apfl-current.json`。首份为第 2207 号，
有效至 `2026-11-22T16:09:04.822Z`，SHA-256
`0baa61b60315ce9ff51970b289e43eb880c698e3d53dea94a5b787d4b5f33162`。
此轮短窗口随原 owner 的新序号更新，A 请取当前文件并核对有效期。

独立公钥来源为 C 原 `native-identity` 创建 transport 用途身份时的公开导出，是同一
测试链域 `167999907280566` 的 ML-DSA-65 公钥，与原批准 PIN、现役终点声明逐字节一致。
Peer ID `novovm-mldsa65:822d60b2de9504d8b66e50d1a6ba1e8f24cbf89986c9fbf2e4d6e49a837297a1`，
公钥 SHA-256 `d52fdb6e06017389394ce39bbd5cdddb6969c652ccd01348714dd94fe9aa8c46`。
同一受控目录的 `C-terminal-native-identity-public.json` 和 `C-terminal-approved-pin.json`
已核对远端摘要，依次为
`e307d5556231b0d3099a55e09472a13136c9eceb6195a076a2300c89f4b90753` 和
`f58e9dbde7b6e8ad6ab38d97938d1ca42f6d931fcdec71ce810b3818ead600e5`。只交接公开数据。

A 请先独立核对上述导出和固定值，再验签当前终点声明。通过后按已批准范围切换 A 信箱，
发布第二跳。C 原服务、数据库和密钥均未改动。

## 2026-11-23 A 已核对 D 绑定，等待 C 终点续签

**`A_READY_FOR_C_REFRESH`**。A 已核对 D 的交接 ZIP、内件摘要与第 406 号原件摘要。
D 的原 `native-identity inspect`、绑定文件及第 406 号声明中的 1,952 字节通信用途
公钥逐字节一致，公钥摘要为 `b2f3f787661fd53303e503694218362cc3a9f5beba443f3cee52a941dfbac5aa`。
第 406 号沿用 C 当期验签回执作为历史证据。A 已备妥经摘要核对的 Windows 原验签器。

B 目录上的 C 第 2196 号在北京时间 11 月 22 日 23:58:03 到期，A 的原验签入口已
确认拒绝。请 C 在现有 SSH 目录续传同一终点身份的当前声明，并通过既有可信交接给出
独立核对的终点公钥标识，供 A 绑定和验签。A 的现役信箱二进制仍是普通 V8，私密角色
在新版 owner 启动时加载。第二跳待 C 终点验签与 A 信箱受控升级，尚未发布。

## 2026-11-22 CURRENT D 的现役 V8 公钥绑定已验签并交 A

**`D_V8_BINDING_VERIFIED`**。D 在已确认的 LAN TLS 窗口中交付原信箱、原 transport 钥的
第 406 号普通 V8 声明。C 用独立公钥与原 `inspect-record` 核对了签名和当期有效性，
接收器返回 HTTP 201 并正常退出。完整声明已通过原 B SSH 目录交 A，文件
`D-self-owned-record-current.json`，SHA-256
`28a983881dcce0c8b50fc98063863e627c03f63200bcdce9b815e743814c8f8b`，到期
`2026-11-22T15:55:33.886Z`。同目录的 `D-V8-binding-for-C-20261122.zip` 已核对原摘要
`521839a61f0a2158fc9bde2545a99d2ee7b7562ee901824cc18bbb176c167b2a`，供 A 独立核对公钥。

C 终点续签已同步刷新。第 2194 号用原终点公钥验签通过，已发布到同目录的
`C-private-apfl-current.json`，本序号到期 `2026-11-22T15:56:03.026Z`。取用以该文件
当期序号和有效期为准。A 先核对 D 的公钥绑定，再验签 C 的当期终点声明并发布第二跳。
若 C 快照已过期，请在验签会话待命时回 `A_READY_FOR_C_REFRESH`。D 之后验签 A 并发布首跳。
第 406 号的验签是当期证据，过期后继续保留该次核验结果，现役定位仍取原 owner 的有效声明。

## 2026-11-22 上一轮 C 在线接收窗口（已验签收件并关闭）

**`C_APFL_D_PROBE_SEEN`**。C 本窗口实收 `TLS_READY_FROM 192.168.71.59`。请 D 在当前
待命会话，从原运行信箱读取当期 `self-owned-record.json`，核对原公钥和有效期后立即上传。
`$recordPath` 指向原件。HTTP 201 和正文 SHA-256 为 C 验签签收。

```powershell
curl.exe --fail --silent --show-error --noproxy '*' --interface 192.168.71.59 `
  --proto '=https' --insecure --pinnedpubkey 'sha256//5PntrTzlzyfn8DDN29eX2CG6d6+tpBefUd32TdLOnEc=' `
  --max-time 18 --header 'Content-Type: application/json' --data-binary "@$recordPath" `
  'https://192.168.71.73:64280/d-record'
```

D 的 [`D_APFL_UPLOAD_ARMED`](../artifacts/audit/private-apfl-20261122/D-upload-armed.json)
（`80ef9d31`）已核对。C 接收器实际监听 `192.168.71.73:64280`，只收 D 的 `192.168.71.59`。
截止 `2026-11-22T16:01:26Z`，北京时间 11 月 23 日 00:01:26。此为新窗口与新 TLS 固定值。

D 探测命令保留如下，探测回执已在本节顶部。

```powershell
curl.exe --fail --silent --show-error --noproxy '*' --interface 192.168.71.59 `
  --proto '=https' --insecure --pinnedpubkey 'sha256//5PntrTzlzyfn8DDN29eX2CG6d6+tpBefUd32TdLOnEc=' `
  --max-time 8 'https://192.168.71.73:64280/ready'
```

## 2026-11-22 上一轮 D→C TLS 探测通过，文件窗口已关闭

C 实收来自 `192.168.71.59` 的固定 TLS 公钥探测，已通过 `214ea855` 回传
`C_APFL_D_PROBE_SEEN`。接收器于本次北京时间 23:45:25 窗口到期后正常退出，C 没有收到
当期 D 声明，也未验签、转交 A 或刷新 C 的交接声明。先前端口和 TLS 固定值均已失效。

下一轮请 D 先回 `D_APFL_UPLOAD_ARMED`，确认操作者或前台会话可以在同一时段完成
探测、等 C 回执、从原 owner 读声明并上传。C 收到待命确认后再开新端点并在收到实际
探测后回 Git。D 在同一个待命会话中同步回执并立即上传。继续沿 LAN 加密交接，不走移动盘。

## 2026-11-22 上一轮 C 在线接收窗口（已关闭）

C 已实际收到 `TLS_READY_FROM 192.168.71.59`，与 D 的 [TLS 探测回执](../artifacts/audit/private-apfl-20261122/D-C-TLS-probe.json)
（`9712eac2`）一致。**`C_APFL_D_PROBE_SEEN`**。当前接收窗口仍在，D 可以从同一运行信箱
读取当前 `self-owned-record.json`，核对原公钥及有效期后立即上传原始字节。使用下列命令，
`$recordPath` 指向本机实际 owner 的当前声明。以 HTTP 201 和正文 SHA-256 为 C 验签签收。

```powershell
curl.exe --fail --silent --show-error --noproxy '*' --interface 192.168.71.59 `
  --proto '=https' --insecure --pinnedpubkey 'sha256//mNxBD5+X8djeTK/OuIaGaIYOtvxQyspNT3Sv/KwrW8Y=' `
  --max-time 18 --header 'Content-Type: application/json' --data-binary "@$recordPath" `
  'https://192.168.71.73:60273/d-record'
```

C 已同步 D 的 [`D_APFL_ONLINE_READY`](../artifacts/audit/private-apfl-20261122/D-online-ready.json)
（`a0e5b0a7`）。原 D 待命窗口已结束，C 根据最新转交开启新的限时窗口，供仍在线的 D 探测。
接收器 `C_APFL_RECEIVER_LISTENING` 已实际监听 `https://192.168.71.73:60273`，只收来自
D 的 `192.168.71.59`。保守截止时间为 `2026-11-22T15:45:25Z`（北京时间 23:45:25）。

D 请仅执行一次探测，接受响应正文 `D-to-C APFL receiver online`，回传
`D_APFL_C_TLS_PROBE_PASS` 或实际错误。下列命令使用固定 TLS 公钥识别 C，固定值必须保留。

```powershell
curl.exe --fail --silent --show-error --noproxy '*' --interface 192.168.71.59 `
  --proto '=https' --insecure --pinnedpubkey 'sha256//mNxBD5+X8djeTK/OuIaGaIYOtvxQyspNT3Sv/KwrW8Y=' `
  --max-time 8 'https://192.168.71.73:60273/ready'
```

截止后本窗口失效。C 的探测回执已在本节顶部，当前等待当期声明。

## 2026-11-22 CURRENT D→C 在线交接预约

移动盘交接暂停。请 D 先回 `D_APFL_ONLINE_READY`，注明 D 当前用于 C 的四机 LAN 地址
和能实际待命的 UTC 窗口。此步骤只确认双方在线，不提前导出短期声明。

C 准备只限四机 LAN、限时的加密收件入口。C 在本机确认原验签器可用后，才回传接收
端点、TLS 公钥固定值和截止时间。D 先核对该固定值并完成接收探测，C 核对 D 实际来路后
发出可接收回执。D 届时从同一运行信箱导出当前普通 V8 声明，并立即在线交 C。完整原件
不进 Git。D 的现有信箱、钥和验证者保持原状态。

C 本机准备已完成。在线接收工具的 TLS 自检中，正确固定值通过，错误固定值拒绝。C 已在
本机非移动盘重建原 `inspect-record`，SHA-256
`b90893594ffd8f42005060d843f00049b1b477f51c1413983b76a2d6a8354eb5`。实际 A 普通声明
第 1478 号通过，D 历史过期记录及 A 错误公钥均被拒绝。待 D 回 `D_APFL_ONLINE_READY`，
再开双方能同时操作的窗口。以上是 C 本机自检，尚无 D→C 接收探测或当期文件。

C 验签成功后用原 B SSH 目录转交 A，并配合 A 的窗口发布 C 终点续签。A 发布第二跳后，
D 接收并验签 A 声明，再发布首跳。手机装机仍以三份当期声明验签通过为准。

## 2026-11-22 CURRENT D 的原生 APFL 公钥绑定待验签

D 的[安装回执](../artifacts/audit/private-apfl-20261122/D-binding-pending.json)（`e8b19448`）
记录 D 已启动自己的 V8 信箱，原验证者保持现状。当前状态为
`D_V8_BINDING_VERIFICATION_PENDING`。C 已通过原四机文件渠道收到并保全
`D-V8-binding-for-C-20261122.zip`，SHA-256
`521839a61f0a2158fc9bde2545a99d2ee7b7562ee901824cc18bbb176c167b2a`，内含三项摘要全部一致。
D 原 `native-identity inspect` 的 transport 公钥在正确测试链域中，与绑定文件及第 7 号声明
内的 1,952 字节公钥逐字节一致，公钥 SHA-256 为
`b2f3f787661fd53303e503694218362cc3a9f5beba443f3cee52a941dfbac5aa`。
第 7 号声明被原验证器以过期拒绝，不用于现役绑定验收。

D 已回传[第 297 号导出回执](../artifacts/audit/private-apfl-20261122/D-current-record-seq-297.json)
（`f1786121`）。D 报告仍为原进程与原钥，公钥摘要和 C 已核对的绑定一致。该声明在
`2026-11-22T14:06:19.544Z` 到期。C 于 `14:17Z` 同步时已超过有效期，E 盘未挂载，
原 B SSH 交接目录也无该声明。本轮未读到原件，签名尚待验证。

下一次先由 D 回传现有可即时读取的受控交接渠道，C 确认接收工具就绪后，D 再导出
同一 owner 的 `self-owned-record.json`，保留原 ZIP 和第 297 号回执。如果只有移动盘，
先协调同一接收窗口，再导出。当期原件先到接收端，Git 回执并行记录序号、到期 UTC 和
SHA-256。A 如已有可转接 D 文件的受控渠道，可协助转交。D 保持原进程及原钥。
C 使用原 `inspect-record` 核对独立公钥、签名和时效，通过后转交 A。在 A 准备验签时，C
从现役终点同步刷新声明。A 发布第二跳，D 再验签发布首跳。

C 已在原 Windows 制品目录找到支持 `inspect-record` 的原工具，SHA-256
`545be9658847cf5da0054da8584ec0d28dffd2ed422dbd81aab46d40a52e6773`。
同轮 A 的有效普通 V8 声明验签通过，其错误公钥对照被拒绝。此前的验签工具交付请求结束。

## 2026-11-22 D 的原生 APFL 首次接入

D 已回传 [`D_V8_OWNER_ABSENT`](../artifacts/audit/private-apfl-20261122/D-owner-absent.json)
（`ffee33bc`）。管理员核对任务、服务、进程及全部已注册用户的标准配置目录，四项均为零。
D 据此进入首次部署，后续实际结果见上节。

C 已备好 `D-V8-APFL-first-enrollment-inputs-20261122.zip`，7,708,170 字节，SHA-256
`0851fd1bb59bd0ec514f656bb148b05855f376a5a527f6e65c3e65b67cf1899f`。包内为现役同版
Windows 信箱程序、两把已批准的手机通信用途公钥、A 的固定后继公钥及 AOEM 校验值。
通过原四机受控文件渠道交付。D 从原 Windows 发布包核验并复用 `novovmctl` 和 AOEM，
填写自己的信箱目录及已核验的载体。保留原验证者服务所用端口和配置。

1. 首次部署使用[原 Windows 安装入口](NOVOVM_RENDEZVOUS_ENTRY.md#windows-新节点接入原生-apfl-信箱)。
   预先核对原包的 `novovmctl`、AOEM 库、手机准入、D 独立信箱目录和可达载体。C 已将
   本文下节的现役 Windows x64 版二进制放入原四机文件交接目录，供 D 按原 SHA-256 核验。
   确认首次部署后，由 D 的安装账户在本机生成并封存服务用途钥。已有资料一律保留，
   验证者服务、数据库和身份保持原状。实际启动、验签和公钥绑定通过后才回 READY。
2. 文件可经原四机受控交接渠道交 C。D 提交原始签名声明、完整公钥及其独立核对来源，
   Git 只记录回执、指纹和文件摘要。C 核验后通过已经实连的 B SSH 交接目录转交给 A。
   D 可沿同一渠道领取已验签的 A 第二跳。D 无需取得 B 的登录密钥。
3. `D_V8_BINDING_READY` 先完成普通 V8 身份交接。A 可据此固定 D 为前驱，协调 C
   当期续签并发布第二跳。D 验签 A 的第二跳后再发布首跳。首跳声明、完整路径及手机验收
   各自留独立结果。

## 2026-11-22 CURRENT 私密 APFL 三角色现役验签交接

路径为 **validator-D 首跳 → validator-A 第二跳 → validator-C 终点**。
`06:48Z` 再查 `main` 和原 B SSH 目录，仍未收到 A、D 的本轮回执。请 D 先独立完成
第一项交接，回传 `D_V8_BINDING_READY`。回执包含现役 V8 身份的可独立核对来源、
交接文件名和 SHA-256，完整材料通过原 B SSH 目录交付。此回执可以在 A 第二跳
声明就绪前提交。A 核对 D 绑定后，领取并验签 C 的现役续签，再发布第二跳；D 验签
A 的第二跳后发布首跳。若 C 的交接快照已过期，请 A 在准备验签时回传
`A_READY_FOR_C_REFRESH`，由 C 当次发布新快照。

`06:49Z` 已发布 C 终点续签序号 1649，原验证器验签通过。当次快照有效至
`2026-11-22T06:49:56.635Z`，SHA-256
`3c339f6f85d62a951a5eab76fffe49620692b432dc7ef9e1f3a4032dcc503460`。

现役 C 已用独立终点公钥验签并通过原 B SSH 交接目录发布续签快照。复核时序号 1622
有效至 `2026-11-22T06:22:53.438Z`，SHA-256
`b6a9bf161145f186588dc6e2584a2f18bc8b179215a833710f287bb669581c64`。
这是当时快照，取用时须检查当前文件的有效期，过期则沿原认证元数据流程取同一 owner
的续签。`06:21:26Z` 复核时，Git 尚无新回执，原 SSH 目录仍缺 A、D 的私密声明及
D 的 V8 公钥绑定。请 D 先发布独立可核对的身份绑定，便于 A 在不等待首跳声明的情况下
完成前驱配置。云 B 原 APFL 服务仍为 active，进程保持不变。

原 B SSH 目录中已提供 C 现役同版 Windows x64 制品
`novovm-rendezvous-service-win64-7724864f13f6.exe`，18,950,144 字节，SHA-256
`7724864f13f676c65eb1c165009d353337d35cfe41304d5b652c6693d77d2e53`。
各机复用既有 APFL owner、身份及存储。私密声明、完整公钥及入口资料沿原受控
SSH 目录交接，Git 回执只写下列文件名、角色、序号、到期 UTC 和 SHA-256。

| 责任机 | 当前动作 | 原 SSH 目录交付 |
| --- | --- | --- |
| D | 先回已批准的本人 V8 身份绑定供 A 独立核对。使用已批准的手机通信公钥作为前驱，固定 A 为后继，领取 A 验签后的私密续签并发布首跳。 | `D-private-apfl-current.json` |
| A | 用独立批准的公钥固定 D 为前驱、C 为后继，验签 C 的当前续签后发布第二跳，供 D 安装验签。 | `A-private-apfl-current.json` |
| C | 终点固定 A 为前驱。待三份现役声明齐全，按已批准的公钥和角色顺序验签并接入手机。 | `C-private-apfl-current.json` |

验签使用 `--verify-private-apfl-announcement CONFIG RECORD PIN`，两个中继应为
`forwarder`，C 为 `terminal`。有效期、scope、批准公钥、角色、载体和路径顺序齐备才
进入 v120 双机验收。手机继续保留 v119 原消息队列和 C 的原信任，云 B 原服务和快照继续保全。

## 2026-11-22 C 现役 V8 接任私密终点

云 B 的 V2 活动公钥是 32 字节 Ed25519，V3 信箱和私密声明使用 1,952 字节 ML-DSA-65
公钥。当前 V3 入口没有能保留原公钥和旧在途寄存凭证的无损兼容路径。B 继续使用
原服务和原库，复核有 12 个未到期未消费对象。云端保全快照哈希一致，16 个记录保留。

按既有 V8 终点备选方案，本机 C 使用原配置、原签名身份、原库和同一 Windows 用户
启动新版。三份保护材料的哈希一致，终点声明序号 1593 验签通过，错误公钥被拒绝。
Windows 二进制 SHA-256 为
`7724864f13f676c65eb1c165009d353337d35cfe41304d5b652c6693d77d2e53`。
服务仍按原周期续签。序号 1593 的声明已交到原 B SSH 目录，文件
`C-private-apfl-current.json`，交接摘要
`f22f07f468df31e72da6594828e667fb607df483c3fed1922fa87ab732668053`。
该文件仅为当时快照，取用前需重新核对有效期并领取续签版本。

内部路径选 D（首跳）→ A（第二跳）→ C（终点）。实际 scope 为
`e753cd5b82899694f6994f583ba826dbc6be5869672d6fd814cfedbcc62b80db`。
C 已将原批准的 A 服务公钥固定为私密前驱。A 需沿原 native+carrier V8 入口启用第二跳，
从自己的信任配置固定 D 为前驱、C 为后继，并用已验签的 C 声明填入后继文件。
D 需要可达、现役 V8 的身份和实际签名公告，按原批准的手机通信公钥配置前驱，A 为后继。
A 现役普通 V8 定位记录已观察到，A、D 私密公告尚未取得。

NOVOCHAT 的既有批准入口接收可选 `private_apfl`，其中 `scope` 为 32 字节数组，
`pins` 和 `announcements` 按 D、A、C 顺序排列。C 仍须在普通信箱信任簿中，
两跳公钥由该私密路径审批绑定，沿原信任簿更新规则保留 C 的记录。用途、角色、
载体定位、签名和有效期都检查通过后才允许启用。完整手机断路、自动补投和设备回执待验收。

## 2026-11-22 前一阶段：原 APFL 服务接纳私密角色

原 native+carrier V8 服务可按 owner 本机配置接纳私密首帧。两跳各使用本机批准的前驱、
后继和原连接预算，只有后继公告验签通过才从同一监听 owner 拨号。终点复用原 APFL V8
业务、固定客户端公钥和原 SQLite。缺少有效公告、前驱或链路时，私密会话关闭。
普通监听入口和版本检查继续沿原协议。私密公告沿原正常发布周期更新，短期密钥有一代
有界重叠，并用原受保护的服务签名身份签发。[接入与验证](NOVOVM_RENDEZVOUS_ENTRY.md#私密-apfl-接入)

Windows 19 项定向回归通过，其中新增用例启动三个原 APFL 所有者和一个客户端，
分别读取、验签公告，按真实监听分发经两跳处理 40 KiB 寄存、领取及测试回执。
Linux x86_64 原信箱 Release 已生成，SHA-256
`17e007b2c77c606b8daac5f5da16e04df9a16667c19c191dd4df864eab814e14`。

云 B 的候选文件已传输并复核哈希。现役二进制以原服务用户执行 `--check` 通过，新候选在
同用户、同配置下拒绝，原因是 V2 配置缺少 V3 必需的 `transport_identity_path`。
切换前停止，未写私密策略、未切换当前符号链接或重启。原 PID 913371 运行，在线库
有 12 个未到期未消费对象，保全快照完整性通过、原哈希一致、16 个未消费对象。

内部联调曾选 C（首跳）、A（第二跳）、B 云信箱（终点）。B 的兼容门未过，三方还没有
实际发布可供手机使用的私密路由。A 同步原 native+carrier V8 执行端及验签入口，沿既有
SSH 签名交接回报可部署的现役角色；以可保留身份和寄存义务的节点作为终点，或在 B
完成原身份和在途义务的兼容接续后再使用该拓扑。C 负责本机、云信箱及 NOVOCHAT。
分别以三方在线公告、手机验签、拨号、收件持久化和最终设备回执验收。

## 2026-11-22 前一阶段：私密载体复用 APFL V8 业务处理

已确认的私密终点以同一个载体接口进入原 APFL。终点从密封会话派生绑定，V8 仍核对
原服务身份、固定客户端公钥、请求时限、原资源控制器和服务所有者的实时授权。终点
复用现有签名发现记录、信箱实例和数据库。私密入口只接受会合发现和 APFL V8 选择器。
[共享实现](../crates/novovm-network/src/product_rendezvous_owned.rs)

六项多跳与 APFL 定向回归通过。本机通过四个实际网络端点和两次中继核对签名入口查询、
40 KiB 对象的寄存回执、领取、确认清理，以及原设备投递回调返回的不透明结果。回调采用
测试数据。手机持久保存后签发的设备回执仍待验证。

现役服务的入口分发尚未启用私密角色。公开服务声明继续沿原机制；首跳、第二跳和终点
仍需要可达、经受信身份签署、角色和有效期相符的声明，并提供运营独立性的验收证据。
NOVOCHAT 在得到并验证该编排前继续保留 Privacy-only 待发。待完成的验收为一台 Wi-Fi、
一台蜂窝、无外部 VPN，切断首选地址和端口后不降级，合格路径恢复后原文字和 PNG
自动补投，并以手机持久保存及设备回执结束。寄存回执仅确认信箱接单。

## 2026-11-22 前一阶段：密封 NovoRUDP 多跳与手机接入边界

共享库新增独立的私密建链入口，复用原认证载体、NovoRUDP 帧及节点字节预算。
本地固定的 ML-DSA-65 身份验证短期建链公钥与角色，构建两个中继和一个终点。
每一跳只解开自己的前驱检查、后继与转交指令，使用各自的随机令牌和到期时间。
中继转发不透明的下一层和端到端记录。终点确认通过后，发起端才能写业务帧。
HPKE 与记录加密沿现有 X25519／HKDF-SHA256／ChaCha20-Poly1305，ML-DSA-65 用于签名。
长度、时间、相邻连接及可预测的拓扑仍能泄露信息，各角色独立性需由运营和客户端信任策略确认。
[共享实现](../crates/novovm-network/src/relay/hop/private.rs)。

Windows 定向回归 5 项通过。新用例在本机打开四个真实载体端点，经两次经认证的中继
传递双向密文记录，并拒绝错误的签名声明、前驱、密封层、重放与终点证明。该证据覆盖
本机载体和协议边界，公网部署、APFL 使用及手机设备回执另行验收。

普通 NovoRUDP 路由继续服务原有用途。私密载体尚未加入现役 APFL／L4 服务分发和
NOVOCHAT 业务选路。手机 Privacy-only 仍在等待，不能把相邻确认、终点确认或多跳数
当作资格或设备回执。后续接入要使用原发送、领取、附件、删除与回执任务，并检查发现
和签名声明分发本身的元数据。两部 v119 的原文字和 PNG 仍待发，没有新提交或重发。
C 信任、历史、v113 回退包及云信箱快照保留，真实收费关闭。

## 2026-11-22 C 接入 Privacy-only 待发，合格多跳仍待接线

NOVOCHAT 沿原 Application 投递所有者保存 Privacy-only 会话选择及两方向权限版本。
缺少合格路径时，原 outbox 继续保存文字和附件，普通直连、单中继和普通多跳均不得
接管该会话。现有路径资格不足，合格路径恢复、补投与设备回执需要独立验证。

复核现有 NovoRUDP 多跳的 `HopRoute`。每跳解开自己的会话封装后，路由结构仍包含
原发送端公钥、完整节点列表、共享 circuit 和最终目标。入口可以将发送端与最终目标
关联，现有多跳所提供的是经认证的转发和端到端载荷保护。
[当前路由线格式](../crates/novovm-network/src/relay/hop.rs)。

共享实现的下一步沿既有 NovoRUDP 载体及认证会话隔离各跳的路由元数据，保护发现、
寄存、领取、附件和回执的同一会话。路径资格由客户端验证并绑定当前授权、连接及寿命。
验收须检查入口、转发与出口各自可见的字段及其可关联性，并测试错误、断线和恢复。
此条仅更新交接；未更改共享运行时或启用真实收费。

## 2026-11-22 C 核对 v118 Privacy-only 验收前置

两台仍安装 v118。10:11 至 10:13 北京时间，Android 确认两台默认网络为已验证的无 VPN
Wi-Fi；mobile-B 的蜂窝已连接，但目前未承载默认路由。邀请可以登记 Privacy，v118 主包
的自动会话入口仍要求显式工程许可，当前未接入能驱动补投的合格隐私路径。已核对本机
五项权限、路径策略及持久化组件用例通过。C 未执行断路或发送，实机 Privacy-only
待发、自动补投及设备回执均待验。

全天候云 B 信箱仍在运行。只读核对在线库有 12 条有效待领，保全快照共 16 条、其中
4 条超过原有效期，快照摘要一致。C 信任、身份历史及 v113 回退包保留，真实收费关闭。
后续接受合格隐私路径须复用原会话、发现和投递所有者；全球接入以运营商能够识别并尝试
封锁服务为验收条件，范围见[共享传输策略](NOVONET_TRANSPORT_POLICY_V1.md)。

## 2026-11-22 A 续签交接与首轮双机导入历史

### 2026-11-22 C 双机首次导入回执

`APFL_A_IMPORTED_BOTH host=C`。北京时间约 03:32，mobile-A（PKL110）与
mobile-B（PCLM50）的 v118 均通过现有邀请检查和确认，保存 A 的第 347 号声明。
两机增量配置保留 C，未退役旧钥。已用原 AOEM 复核完整 ML-DSA-65 签名及固定标识
`novovm-mldsa65:afc9c2f20c026f32dae86c03a56cdba7b97657f9418e890a05a5dfac3dbe5f5c`。
A 可以结束本次声明文件滚动发布，原信箱继续运行。

- mobile-B 蜂窝，无 VPN。03:33:51 记录 L3 认证和公网 Relay 载体，03:33:52 原生
  信箱会话就绪，随后通过原认证路径刷新声明。C 的 LAN 入口没有配置 Relay，这条
  已认证 Relay 会话归属 A。
- mobile-A Wi-Fi，无 VPN。03:33:46 已有原生 APFL 会话就绪。此时 A 与 C 均在配置中，
  日志记录了直连和 UDP，尚未将该机的会话独立归因于 A。首选地址故障和原队列补投、
  持久设备回执、Privacy 防降级仍待实测。

### 2026-11-22 C 双机跨网认证及首选信箱断路回执

`APFL_A_WIFI_CELLULAR_AUTHENTICATED host=C`。03:55 北京时间，暂时停运 C 的独立会合信箱，
两端原监听归零，信任簿保留 C。v118 的 mobile-A 以 Android 已验证、无 VPN 的 Wi-Fi
认证 A 的原生 APFL 信箱并刷新声明；mobile-B 以已验证、无 VPN 的蜂窝经公网 Relay
重新认证 A。两机均报告活动 L3 会话。随后沿原钥恢复 C，并以原两个可信公钥和当前签名
声明更新 mobile-A 缓存。断路准备前 mobile-A 报告两路活动 L3。

按原 App 入口将一条测试文字和一张 PNG 各提交一次。此时 mobile-A 暂时离线，原会话
待发各一份，蜂窝端 mobile-B 尚未出现对应内容。04:03:02 停掉 C 会合实例，核对其
两个原 UDP 监听及原有端口占用均归零。随后 mobile-A 恢复原 Wi-Fi，mobile-B 继续走蜂窝，
两条网络均无 VPN。无人工重发、未重启 App。B 自动持久保存原文字和图片，各显示一次，
图片可打开；A 在 04:03:14.244、04:03:14.268 记录两次设备回执提交。恢复阶段仅有一条
活动 L3，且 L4 也保持可用，当前证据覆盖客户端补投及收件，未将每个对象归因到同一载体。

整个真机业务场景沿用原会话显式获准的工程模式。两个手机的联系人授权未因断路而降低；
默认 Privacy 关闭无资格线路的三项组件回归与联系人权限桥接四项回归通过。尚未以
两部手机的 Privacy-only 会话执行断路，公网单一 Relay 仍不是合格匿名路径，Privacy
实网防降级不能由本轮业务验收替代。

04:04 以原配置、原库和原封存钥恢复 C，核对两个 UDP 监听；mobile-B 已恢复原 Wi-Fi。
原聊天身份历史、v113 回退包保留。云 B 的在线库与保全快照均有 16 个待领对象，两库
完整性、快照及 v113 摘要一致。正式收费关闭。

### 2026-11-22 C 核对 mobile-A 独立 Wi-Fi 入口

03:42 北京时间，暂时停止 C 的独立会合信箱，核实该进程原两个 UDP 监听均消失；
两机信任库未改。同期 mobile-A 的 Wi-Fi 已打开但未取得 Android 默认网络，
mobile-B 的已验证默认网络为蜂窝。mobile-A 在 Wi-Fi 重连和选用可见的已保存网络后，
仍未获得已验证的 Wi-Fi 网络。该窗口未建立 mobile-A 到 A 的独立 APFL 会话。
03:43 用原配置、原库和原封存钥恢复 C 会合实例，核对两个 UDP 监听。

03:46 复核 mobile-B 仍有无 VPN 蜂窝默认路由；当前运行快照为 `l3=0`，原生入口尝试
出现地址超时和候选授权拒绝，日志未归因到具体节点。早先的蜂窝通过记录保留，
下轮需重新取得两机同时在线认证证据。未发送新测试内容或开始故障后的自动补投验收，
未手动重发。云 B 在线库与保全快照各有 16 个待领对象，完整性和快照摘要一致。
两机原身份历史、C 信任、v113 回退包继续保留，真实收费关闭。

**03:25 后当前状态：APFL_CURRENT_RECORD_PUBLISHING host=A，持续到双机导入确认。**
用户已明确要求不再反复错过短窗口。A 只读原运行信箱自然续签声明，原子交接同一
`A-self-owned-record-current.json`，逐次验签并核对 B 端 SHA256；不延长声明有效期、
不重签、不更换服务钥，不重启或更新信箱。03:25:43 已发布第 341 号有效声明。
下方 03:11:53 截止时间是上一轮历史，不再是本轮停止条件。
C 请直接继续两部 v118 首次导入，保留原 C 条目；回传 mobile-A / mobile-B 各自的
导入结果、使用的 A 声明序号及服务公钥标识，然后分别回传 Wi-Fi/蜂窝 APFL 认证的
实际阶段与结果。A 只在双机导入证据确认后结束本次文件交接，信箱原 owner 继续运行。
截至此次核对，共享 Git 仍无双机导入/认证的新回执，不能将其描述为已认证或握手失败。
A 信箱 PID 25740、验证者 PID 18888 及启动时间保持；D 未操作，两笔交易未重发。

**APFL_CURRENT_RECORD_PUBLISHING host=A：已开启，不是待开启。** 北京时间
2026-11-22 02:11:55，A 从原运行信箱只读导出第 268 号自然续签声明，独立验签后
原子更新 B 原 SSH 目录内 `A-self-owned-record-current.json`，B 端文件 SHA256 与 A
一致，当时剩余有效期约 72 秒。此轮持续跟随原 owner 新序号发布，最晚至 **03:11:53**。
C 请在此窗口下载当前文件，先验 pin/签名/时限，保留 C 条目并分别导入两部 v118，
回传每部手机的导入结果、实际 Wi-Fi/蜂窝网络、APFL 认证结果与失败阶段。
目前 C 最新回执仍为未导入/未认证；这是首信任交接未完成，不是已定位手机握手失败。
A 信箱 PID 25740、验证者 PID 18888 及启动时间均保持；D 未操作，两笔交易未重发。

**A：只读正在运行的信箱状态文件，不停信箱、不重签、不改监听。** C 已核验
旧 ZIP，第 16 号声明到期。请在 A 的信箱所属 Windows 用户下打开 PowerShell，
用下列方式把自然续签的原件送到既有 B SSH 目录。C 从
`A-self-owned-record-current.json` 下载，同一公钥下分别导入两部 v118。
请回报 `APFL_CURRENT_RECORD_PUBLISHING host=A` 与最晚结束时间。

```powershell
$config = Join-Path $env:LOCALAPPDATA 'NOVOVM/Rendezvous/config.json'
$record = Join-Path ((Get-Content -LiteralPath $config -Raw | ConvertFrom-Json).root) 'self-owned-record.json'
$sshConfig = Join-Path $env:USERPROFILE '.ssh/validator-B.conf'
$sshArgs = @('-F',$sshConfig,'-o','BatchMode=yes','-o','StrictHostKeyChecking=yes','-o','ConnectTimeout=8')
$publish = 'set -eu; umask 077; d="$HOME/.novovm-handoff/A-apfl-20261121-afc9c2f2"; test -d "$d"; t=$(mktemp "$d/.current.XXXXXXXX"); trap ''rm -f -- "$t"'' EXIT; cat > "$t"; test -s "$t"; mv -f -- "$t" "$d/A-self-owned-record-current.json"'
$deadline = [DateTimeOffset]::UtcNow.AddMinutes(10)
$sequence = 16
while ([DateTimeOffset]::UtcNow -lt $deadline) {
    $source = Get-Content -LiteralPath $record -Raw
    $r = $source | ConvertFrom-Json
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    if ($r.relay_peer_id -ne 'novovm-mldsa65:afc9c2f20c026f32dae86c03a56cdba7b97657f9418e890a05a5dfac3dbe5f5c') {
        throw 'A mailbox identity mismatch'
    }
    if ($r.sequence -gt $sequence -and $r.issued_at_ms -le $now -and $r.expires_at_ms -gt ($now + 25000)) {
        $source | ssh @sshArgs validator-B $publish
        if ($LASTEXITCODE -ne 0) { throw 'B SSH handoff write failed' }
        $sequence = $r.sequence
        Write-Output "Published current signed sequence $sequence, window ends $deadline"
    }
    Start-Sleep -Seconds 3
}
```

C 侧先校验固定公钥、声明签名和有效期。保留原 C 条目，先验 A 的双机 APFL 认证，
再安排首选地址和端口故障。云 B 信箱与其已有寄存不在本次声明更新范围。

C 侧复核：B 目录已出现第 160 号声明，收到时已超过北京时间 00:25:03 的有效期。
本机独立验签工具对原过期声明返回拒绝。C 在多轮短窗口内未收到新的有效声明，
未改双机配置或启动断路。两台 v118 均在，v113 回退包摘要一致。B 在线库与保全快照
均仍有 16 个有效待领对象，快照摘要一致。待 A 开启上面的发布窗口后继续双机验收。

## CURRENT A/C/D 统一配置启用原生 APFL 信箱

2026-11-21 A 已从 B 的 SSH 交接目录签收手机公钥文件，13,956 字节，SHA256
`489f99e71b2d83725e533c18e67a316c15e1d906363a1412b5bf20141f393b1a`，两把完整钥均为
1,952 字节且互不相同。A 独立信箱已沿原用户会话任务启动，实际为 `native+carrier`，
服务公钥标识 `novovm-mldsa65:afc9c2f20c026f32dae86c03a56cdba7b97657f9418e890a05a5dfac3dbe5f5c`。
完整声明和 Relay 配置经原 B SSH 受限交接，未提交公开 Git。最新声明独立验签通过，
篡改签名拒绝；声明端点强制 Relay 传输连接通过（首次30秒超时，后续复测成功），
尚不是手机 APFL 认证。任务注册已修复同一 Windows 账户的 SID/名称表示问题；
固定 AOEM 程序树仅增加当前账户读取执行权限，内容哈希不变。新信箱 UDP 规则仅限
新程序及两个监听端口，验证者 PID/启动时间保持，D 未动、两笔交易未重发。
`owner_ready=true / record_and_pin_ready=true / phone_wifi_cellular_authenticated=pending`；
C 需保留原 C 信任条目，增量导入 A，并分别回传 Wi-Fi/蜂窝认证、原队列及 Privacy 结果。
声明会续签，静态副本过期后只能按原认证元数据刷新路径取得当前声明，不授予过期权限。

C 已同步 `33984fd7` 并经 B 的原 SSH 下载 A 的交接文件。C 独立核对了手机准入文件
SHA-256、声明文件 SHA-256 和声明中完整服务公钥对应的固定 peer ID，均一致。
收到的声明序号为 16，已于北京时间 22:00:45 到期。v118 的增量信任必须用当前有效
声明；原认证 `ServiceRecordV7` 只沿已在本机受保护信任簿中的节点刷新，无法为尚未
纳入信任的 A 首次登记。两个手机当前保持 v118，原 C 信任保留。

`APFL_CURRENT_RECORD_REQUEST host=C`。请 A 在既有 B SSH 受限目录交接当前
`self-owned-record.json`，另存 `A-self-owned-record-current.json`。声明有效期约 90 秒，
可在 C 进行两机导入的短窗口内随原 owner 续签滚动更新该文件。沿原钥签名并原子更新；
C 按原带外 pin、时间和完整签名验证，导入时保留 C 信任。原 A 信箱安装已确认，
本轮未更改手机配置、未触发首选地址故障、未发送新消息，手机跨网认证、自动补投、
设备回执及 Privacy 仍待。云 B 原服务运行，在线库和保全快照各有 16 个未到期未领取对象，
两库完整性和快照摘要复核通过。真实收费保持关闭。

2026-11-21 A 已用现有 NOVONET 端点完成公网 Relay 的有界传输前置：两端关闭 IP
直连，实选 `relay`，核对传输对端公钥与随机挑战往返，9,535 ms，PASS。探测两端
同机，不是手机跨网、ML-DSA 服务认证或 APFL 信箱验收；未生成持久身份/签名声明。
A 的安装计划改用原 owner `native+carrier` V8，保留 LAN 原生 UDP，并复用原显式
Relay/DNS；不再把手工公网端口映射作为必要前置。需 C 回传两部 v118 的完整现有
ML-DSA-65 通信用途公钥后，安装并启用同一信箱，再交接正式声明。此时仍为
`APFL_BACKUP_PENDING host=A`，原验证者未动，D 未操作，两笔交易未重发。

C 已从两台正在运行 v118 的设备只读取通信用途公钥，每把 1,952 字节；与 C 已验收的
信箱准入表精确匹配。已生成仅含 `allowed_clients` 的本地交接 JSON
`NOVOCHAT-v118-APFL-allowed-clients-20261121.json`，经四机原文件交接交给 A。
公开 Git 记录仅确认 `APFL_CLIENT_PINS_EXPORT_READY host=C`；完整数组留在交接文件。
A 实际收到后，使用该数组安装、启用同一 UDP＋V8 信箱，并交接其未过期签名服务记录和
已核验中继配置。C 用保留 C 节点的增量信任配置进行蜂窝／Wi-Fi 分别认证。A 下载签收、
信箱启用及双机认证的状态仍为待确认。

三机复用同一主线。首次安装能力已接入原 Windows 用户会话安装器，使用各机自己的
配置、密钥及 APFL 信箱存储。直接按 [Windows 首次接入](NOVOVM_RENDEZVOUS_ENTRY.md#windows-新节点接入原生-apfl-信箱)
使用统一 Windows 包，满足公网路径的机器先接成备用节点。部署后向 C 交接当前有效的
签名服务声明、公钥指纹和对应入口。C 负责蜂窝与 Wi-Fi 的 v118 原队列、设备回执及
Privacy 验收。没有实际声明和公网可达性之前，状态仍是待验收。云 B 原有寄存义务保持。

## 先前交接：给 validator-A 的一条任务：确认一台可用的手机 APFL 备用入口

**A 先在本机核对，A 没有就请 D 归属人核对。找到一台即可。** 查现有原生 APFL
信箱服务进程、实际监听和最新签名声明。合格候选需要原生 APFL 信箱能力、未过期的
ML-DSA-65 签名声明和公钥指纹，以及独立于首选地址的公网路径。确认这两部手机的
既有通信用途公钥在准入范围内。只有验证者进程和认证查询时，回报手机信箱未核实。

- 有一台符合条件，在本节下回 `APFL_BACKUP_READY host=A|D`，附服务版本、
  公钥指纹、声明到期时间和已核实的可达范围。通过原可信交接把**完整签名声明及实际
  入口**给 C。由 C 在 mobile-A 的 Wi-Fi 与 mobile-B 的蜂窝分别完成实际认证。
- 两台都没有，回 `APFL_BACKUP_UNAVAILABLE`，各写一行 A 和 D 的实际情况及缺项。
  若 D 尚未实际核对，请写明 D 待核对。若有候选但未证实公网路径，回
  `APFL_BACKUP_PENDING host=A|D` 并写明缺项。

这个回执只需要现有服务的核对。**C 负责手机导入、首选入口故障注入、原队列补投、
设备回执和隐私验收。** 核对阶段保留 A、D 验证者现状和云 B 的 16 个待领对象，
公网上写入的 Git 回执限运行结论及公钥指纹，保留原身份历史和关闭的真实收费。

## 2026-11-21 validator-C 公网换址前置核对与 A/D 请求

本轮核对 SUPERVM GitHub `main` 的 `16cae4d7` 及同步后的共享交接。
新增为验证者原生提案只读查询。A/D 已有的原生认证及对象交付证明适用于验证者链路；
本轮未找到 A 对 v118 交接的信箱回执，也未取得 A 或 D 正在服务的手机原生 APFL
签名声明、手机可达路线和适用客户端授权。当前没有把 A 或 D 加入手机信任簿。

请 validator-A 协调 A、D 中**一个具备独立公网路径的现有节点**：
核对其原 APFL owner 正在运行，交接现行的 ML-DSA-65 签名服务声明及相应公钥指纹，
注明客户端可用的公网载体、有效期，以及测试窗口内首选地址和端口可同时失效的范围。
备用入口需能在首选地址失效后，从蜂窝和独立 Wi-Fi 继续取得有效、已批准的服务声明。
服务准入沿用现有手机通信用途钥。公开 Git 仅回传节点角色、版本和结果；签名邀请
与端点经原可信交接传递，不包含任何私钥。请回传 `host / owner_ready /
record_and_pin_ready / primary_change_ready / public_paths_ready`。

12:37 北京时间，C 实机核对 v118 的 mobile-A 默认 Wi-Fi 和 mobile-B 默认蜂窝，
两台均为 Android 已验证的无 VPN 默认网络。随后恢复 B 的原 Wi-Fi。
这次仅完成接入网络前置，未切首选入口、未发送测试文字或图片，也未验收隐私路径。
阿里云原服务保持运行，同一只读检查中原库和备份均仍有 16 个未过期未领取对象；
备份摘要一致、`quick_check` 正常。原身份、历史、v113 回退包及关闭的真实收费保持。

13:01 北京时间复查 GitHub `main` 至 `77aab38d`，该增量只有 C 验证进程退出证据。
仍未收到 A 或 D 的手机 APFL 服务声明及受信公网路线。两台手机仍为 v118，v113
回退包摘要一致。只读复核云信箱为 active、enabled，在线库和保全快照各有 16 个未到期
未领取对象，两库 `quick_check` 正常，快照 SHA-256 未变。未启动换址或发送测试消息，
自动补投、设备回执和 Privacy 选路仍待验收。

## 2026-11-21 validator-C 回传 v118 原消息补收及 L4 双机直连

mobile-A 和 mobile-B 已用同签名的 v118 从 main 原位升级，保留原聊天身份、历史和寄存数据。
两个 v114 旧入口已退出。C 使用合格的 Windows `1b6808f6` 原生 APFL 和独立服务用途钥
协助两机重新会合。12:10 北京时间，A 持久保存 B 在 10:05 发出的原消息，B 的同一条历史
记录取得设备回执；未手动重发。原消息存放于手机 B 的 L4，未动用云信箱。

12:13 停止 C 此次会合实例，原验证者未停。客户端配置中没有云信箱，两台手机均为
`l3=0`、`l4=1`。同一 Wi‑Fi 下两部手机互发文字和 PNG，两端都显示保存对方的图片，
并记录 L4 设备回执提交。12:19 用同一封存身份和原库恢复 C 会合实例，确认两个 UDP
监听及新签名记录。重开的记录包含变化的端点；至 12:21 两台手机仍通过 L4 连通，
观察窗口未见重新认证 C。该结果保留为局域网观察，不算旧路径全部失效时的自动换址通过。

云 B 原 systemd 服务与 16 个待领对象的受限快照继续保留。独立 Wi‑Fi／蜂窝换址验证仍需
一条可同时到达并通过验签的独立备用声明路径，随后复测原队列补投、设备回执和隐私约束。
本轮未改验证者和生产收费，详见 [NOVOCHAT 双机验收](https://github.com/XujueKing/NOVOCHAT/blob/499dfae/docs/implementation/MAILBOX_PERSISTENCE_MIGRATION_2026-11-20.md#2026-11-21-v118-原消息恢复及双机直连)。

## 2026-11-21 validator-C 可信入口轮换与云信箱保全

共享自有信任簿允许本机持有者明确批准入口换钥。旧公钥停止授权，原签名记录及撤销标记
在同一受保护的检查点保留。替代公钥必须附有效的新签名声明；仍获批准的公钥保留其
序号下限。租约更新、候选查询和后续重开均不得使用退役记录授权。一次换址若使用同一
公钥，继续走现行签名声明更新流程。

阿里云原 `novonet-mailbox.service` 的活动 SQLite 库已完成同机在线备份。快照
为 1,679,360 字节，`quick_check` 通过，包含全部 16 个当时未过期、未领取对象。
快照仅保存在云端受限目录。原 v2 进程保持 active、enabled；原库、封存身份和寄存义务
继续由它承担。V3 已基于原单元准备受限 drop-in，`systemd-analyze verify` 通过；
配置仍留在云端受限准备目录，未装入 systemd，现有单元无 drop-in。Linux 现有
owner 入口无法把 V2 寄存
凭证改绑到新 ML-DSA-65 公钥，需等原对象履约完毕或有可验证的按原授权接续机制再切换。
手机 B→A 原对象属于手机 L4 寄存，单独验收。

## 2026-11-21 validator-C 常驻云信箱 ML-DSA-65 升级准备

云信箱继续使用同一 `novonet-mailbox.service` 常驻服务。现场旧版 v2 仍为 active、enabled，
按 `Restart=on-failure` 运行。旧库只读查询仍有 16 个未过期、未领取对象，同时在两种监听
上保有原运行进程。待领取义务未处置前保留原服务、原身份和库。

在 B 的数据盘用已冻结的 `1b6808f6` 正式 Linux 制品核对服务、`novovmctl` 及 AOEM 哈希。
复用现有服务账户和 systemd 封装凭据，准备 V3 配置及独立用途钥，准入使用两部手机的
原 ML-DSA-65 通信用途公钥。实际密钥生成、owner 配置核对及原 UDP／V8 设置成功。
此阶段没有绑定新监听或发布新签名声明，旧服务 PID 保持。切换使用原 systemd 单元，
先确认旧寄存义务已接续或自然结束，再验收新声明、连续续期、公网 APFL 认证和设备回执。
首次安装入口同步修正到 V3，现有服务继续沿原升级路径处理。

## 2026-11-21 validator-C 核对手机直连和 L4 恢复边界

v113 在 Windows 信箱退出后仍有双机原生 L4 UDP 交付与设备回执。v114 源码保留手机
L4 监听、直送、临时寄存和回执。恢复任务先找回手机原会话路径，独立 L3／云信箱可并行
提供会合、寄存和备用路径。运行中的合格直连与旧地址全部失效后的重新会合分别验收。
复用原 Application owner、签名声明和 NovoRUDP／APFL，以合法候选实测择优，持续通过
原联系人隐私和授权检查。原生移动端口由系统分配。近期目标是同网两机原消息补收及
设备回执，之后再验收完全失去旧地址的无 VPN 自动换路。

## 2026-11-21 validator-C 回传 v114 原消息暂存与恢复修复

B 原消息已在手机 L4 保管，A 未领取、无设备回执。两部手机保留原进程和身份。两端出现
L4 载体拨号失败和原生握手超时，之后的权限原因码 2 对应本机 60 秒通信任务边界。
[NOVOCHAT efca56d](https://github.com/XujueKing/NOVOCHAT/commit/efca56de8916a52153159d7b0d2863cffaf87cee)
补齐使用已验签的过期 UDP 记录，在同一 L4 公钥下只读取当前联系人声明的路径。Android
ARM64 库检查通过。修复未安装到手机，原消息恢复和无 VPN 换址均待实测。

已用冻结 `1b6808f6` Windows `novovm-rendezvous-service`、`novovmctl` 和核验的 AOEM，
在 C 独立启动局域网测试信箱。记录是 V2，公钥和签名分别为 1,952、3,309 字节。该新测试钥
尚未导入手机，旧两枚测试入口均过期。临时监听已结束，原验证者及原 B 服务未修改。
优先恢复原消息可用的可信入口，保留原寄存和信任记录，然后复核原消息及设备回执。

## 2026-11-21 validator-C 回传 v114 安装及信箱入口不匹配

两台手机原位安装同一 v114 并冷启动读回原会话及图片历史。无 VPN 下，mobile-A 经 Wi-Fi、
mobile-B 经蜂窝连通 validator-B 的公开 TCP。现运行的 `novonet-mailbox.service` 发布的
`self-owned-record.json` 为 v1，`relay_public_key` 有 32 个字节。该记录仍在有效期内，
v114 的 `DiscoveryServiceKey` 要求完整的 1,952 字节 ML-DSA-65 公钥。需要核验可认证的新信箱
服务和 `ServiceRecordV7`；原验证者的 ML-DSA-65 节点身份与信箱服务资格分别验证。

validator-C 原验证者监听正常，旧本机信箱未运行且其声明已过期。本轮尚未取得 A 或 D 的合格
公网 APFL 声明或独立镜像。请在现有发布链路上协调隔离候选，提供支持原公钥换 IP 和端口的
测试信箱，以及首选 IP 失效时两种手机网络仍能访问的可信记录来源。交接内容限公开入口、
经核验的服务公钥或信任材料、可变更窗口。
到齐后由 C 继续双机故障注入及原消息补投、回执、隐私验收。未停验证者或 B 信箱，未发送
测试消息，未启用真实收费。后续优先 App 内网页和手机为电脑节点提供原生中继，暂缓其他 App。

## 2026-11-21 validator-C Android 无 VPN 换址验收协调

按[共享传输策略](NOVONET_TRANSPORT_POLICY_V1.md#2026-11-21-共享抗封抗审查与节点原生-apfl-信箱)
验收各节点原生 APFL 信箱。C 负责 Android 消费签名入口和 Wi-Fi／蜂窝双机验收。四机既有
验证者服务保持原运行状态，复用现有节点前先对照当前签名信箱能力、对外地址与备用入口。
已通过的链上认证查询和诊断对象转发分别保留原验收范围。

validator-C 已只读核对 B 原 `novonet-mailbox.service` 和公开声明，运行模式为
`native+carrier`，声明含原生 UDP 与 V8 TCP。mobile-A 用 Wi-Fi、mobile-B 用蜂窝
分别连通声明中的 TCP 地址，测试期默认网络无 VPN。当前证据为 TCP 可达。
首选节点公网地址和端口的受控变更、独立声明恢复路径及 Android 业务回执仍待联调。
B 的原生 UDP 和 V8 TCP 公告使用同一公网 IP，当前 Iroh 声明没有另一个全球可路由直连
地址，也没有公布转发服务。整个 IP 失效的验收需要另一个仍可达的已批准声明来源。
通过网络出口访问公网内容已列入共同目标，使用场景与原有 APFL 业务分别验收。
客户端已接原签名 UDP 地址变更通知并验证缓存冷重开，见
[NOVOCHAT e125c9a](https://github.com/XujueKing/NOVOCHAT/commit/e125c9a)。

## 2026-11-20 C 回传：mobile-B v110 旧 L4 已进入排空

NOVOCHAT [4b0edda](https://github.com/XujueKing/NOVOCHAT/commit/4b0edda402789810c364d5f2edbbbe89ef439c26)
记录了 mobile-B 原进程共享暂不可用、保留现场后一次原版 App 重开、恢复运行，
以及通过原设置关闭新接单。数据库两次确认关闭，同一进程和原密钥封装保留。
恢复到关闭前出现新寄存，以关停后快照为准。14 个未到期信箱授权，
有效待领取从 11 降至 10，其中 1 个到原期限。关停后旧服务历史仍更新，
另一个手机的实际领取与设备回执尚待验证。mobile-A 未连接，未覆盖安装；
C 继续客户端升级，四机节点与生产收费保持原状态。

## 2026-11-20 C 回传：v110 旧数据升级门槛

NOVOCHAT [788f5e7](https://github.com/XujueKing/NOVOCHAT/commit/788f5e790626d927cad220322d71af6cb8e88e5a)
将旧节点缓存扩容延后到首笔有效更新，与新密文在同一事务提交。只读恢复和
重复确认保留 schema 1。缓存 5 项、实际 AOEM 服务配置及 L4 持钥 6 项通过，
Android arm64 库检查通过。旧节点签名规则仍需独立确认新版信任来源。

20:14 北京时间，mobile-B 原 v110 的 L4 原库只读核对有 14 个未到期信箱授权、
7 个未领取且未到期的对象，仍允许新接单。旧 L4 须先保全领取路径和既有义务，
本轮不覆盖安装。mobile-A 未连接。候选 APK 和回退包留在本机，手机版本、
身份、历史、四机节点和生产收费保持原状态。客户端交接由 C 继续。

## 2026-11-20 C 回传：Android v111 通信持钥恢复与候选包

NOVOCHAT [c6e5c55](https://github.com/XujueKing/NOVOCHAT/commit/c6e5c55584db3a8136d889df286336d3c5f3db58)
已推送。启动先在原身份工作队列恢复已有通信钥及审定验签器，再恢复节点配置。
启动、后台和配置检查不会建钥。用户明确发起连接或投递，且当次操作仍有效，
才允许首次登记。原身份、聊天密钥与资产权限继续各走既有入口。

Kotlin JVM 回归 172 项、APK 校验器 15 项通过。Android arm64 全量构建
生成 v111 Debug 与 Profile，包验收范围为打包。Profile SHA-256
为 4baa46da853376a023ecdd4bc0f90f81acbefa8356b9cdf7fbb82e92aec3e2ad，
签名证书与保留的 v108、v110 安装包一致。两包 AOEM 文件均匹配审定哈希
7f283b8152eb30b8873e75be4b764905d37db5c1afaec88238b30146f264bfca，
并启用系统解包供原生加载。候选包留在 C 本地。

C 继续核验旧节点配置和旧 L4 寄存义务的升级路径，两部手机本轮未安装。
双机收发、自动恢复和长期待机仍待验收。SUPERVM 仅同步客户端交接，
四机节点与生产收费保持原状态。

## 2026-11-20 C 回传：Android L4 持钥与发布通过库检查

NOVOCHAT [78ed42e](https://github.com/XujueKing/NOVOCHAT/commit/78ed42e1c53da94c332fd2122545c77cfbbe0635)
已推送。新版 L4 通过原 Keystore 材料保护独立 ML-DSA-65 服务钥，重开沿用同钥，
发布和持久服务状态复用共享 SDK。旧 L4 目录缺新版封装时拒绝启动，原寄存数据
保留。Android arm64 Rust 库从 10 项错误到通过，实际 AOEM 封装回归、定向
严格 Clippy 和 168 项 Kotlin 测试通过。主 App 启动尚待接现有通信持钥入口，
旧 L4 义务迁移和双机仍待验收。本轮未生成或安装 APK，只更新客户端代码和交接。

## 2026-11-20 C 回传：V3 联系人控制和数据握手

NOVOCHAT [c0f0c8c](https://github.com/XujueKing/NOVOCHAT/commit/c0f0c8c1c727be19d3bd7b46b9554d410dda9d05)
已推送。受保护发现预约绑定原会话、方向和存储归属。控制与数据流分别使用
V3 完整 ML-DSA-65 公钥走原共享握手。消息入口复核载体绑定和两端公钥，原
控制计划及持久设备回执规则继续适用。Windows 通信库检查、实际 AOEM/Iroh
隔离回归及该测试严格 Clippy 通过。Android 16→10 项编译错误，剩余 L4 服务
持钥和发布路径。真机及实网业务尚待验收。本轮只增客户端与交接文档。

## 2026-11-20 C 回传：联系人任务持钥与 V3 发现

NOVOCHAT [1a7e0a5](https://github.com/XujueKing/NOVOCHAT/commit/1a7e0a5a5950f92d4d4fcc1a0a6ff1e611282954)
已推送。Android 受保护发现为每次任务生成独立 ML-DSA-65 密钥，原生持钥
保留到任务结束。当前联系人授权、身份代次、任务和父持钥会话继续约束借用。
V3 使用原 4 KiB 发现报文、修订高水位、期限和条件 ACK 递送完整公钥。
Windows 实际 AOEM 持钥回归 11 项及定向严格 Clippy 通过。Android 全量库仍有原
16 项编译错误，V3 公钥尚未提交给控制和数据握手，未安装 APK 或操作四机节点。
本轮 SUPERVM 只同步交接。控制和数据握手的消费端接线继续由 C 负责。

## 2026-11-20 C 回传：手机持久寄存证明接入

NOVOCHAT [51edb51](https://github.com/XujueKing/NOVOCHAT/commit/51edb5192cc0029422c3e08b4c8f359ac6aaf4e1)
已推送。原文字、媒体和待交回执的续发读回同一份加密日志中的 ML-DSA-65
寄存证明。重复寄存保持首份有效证明。两种本地证明表容量在原事务内迁移，
不更换加密密钥、密文或服务义务。历史寄存投影仍可离线读取。
L4 联系人声明已使用共享只读验签器和完整服务公钥。

实际 AOEM 与 SQLite 隔离回归 5 项通过，隔离严格 Clippy 通过。Android
从 20 降到 16 项编译错误。Windows 完整网络测试仍未编过，库 6 项、库测试
182 项错误。设备 L4 持钥、消息与控制会话身份及联系人缓存兼容仍待接续。
[实施边界与原始证据](https://github.com/XujueKing/NOVOCHAT/blob/51edb51/docs/implementation/MAILBOX_PERSISTENCE_MIGRATION_2026-11-20.md)。
本轮为客户端增量。四节点、手机现有版本和数据保持，生产收费关闭。

## 2026-11-20 C回到手机端：邮箱通信钥及直接节点回执迁接

用户说明A/B/C/D就绪，C继续独立NOVOCHAT，不操作四机节点。NOVOCHAT
[151a191](https://github.com/XujueKing/NOVOCHAT/commit/151a191f2216fb94984c9b772f123a3a7502494d)
已推送：邮箱/通知/发件及原native驱动显式消费App原持钥owner的ML-DSA通信身份；
chat signer保留既有E2EE/历史用途，不转为通信或资产钥。完整服务钥与slot分离，
L4 Offer保留完整钥；6处直接Stored检查绑定已选slot和原对象后调用原ML-DSA验签。
不把节点寄存当设备送达，offline持久证明回读及L4持久服务owner仍待迁接。

本轮核对并消费85de114d输入：新增UCA应用编码/查询/签验、原NativeObjectLink可选
应用lane、Iroh显式上限/诊断及typed时间不足；这些不等于手机已登录UCA。三处guard
新增直接依赖ASAS/netwatch路径，复用上游WMI无chrono补丁；24项发布版netwatch源码
与build.rs逐字节一致。没有启用ASAS、另写应用RPC或共享协议，dirty/diff门禁保留。

实际默认core245通过/1原有忽略；6隔离target共29次通过（27个不同测试，2个服务钥
回归重复执行），严格Clippy通过。新增两项寄存回归用真实AOEM+SQLite，覆盖重复/
重开及错服务/slot/对象/密文/期限/签名拒绝；TCP loopback只记本地fixture。
完整Android46→20项错误、exit101，App0warning，原共享purpose_keys有2条unused
参数warning。完整App网络回归未编译通过，未生成/安装主聊天APK，未做双机验收。

[实施范围、剩余缺口](https://github.com/XujueKing/NOVOCHAT/blob/151a191/docs/implementation/MAILBOX_COMMUNICATION_MIGRATION_2026-11-20.md)；
[原始输出、20项清单、命令及7项提交后hash](https://github.com/XujueKing/NOVOCHAT/tree/151a191/docs/implementation/evidence/mailbox-communication-20261120)。
原身份/历史/附件/已安装v110及v108回退材料保留，正式资产/生产收费关闭；Flutter/
Kotlin未改动未重跑，旧结果不扩充。C下一项L4持久owner、联系人声明verifier、持久
寄存证明和消息/控制会话身份；全部编译/迁接回归通过后才构建和安装手机候选。
A继续节点主线不重复写App；这是Git通知，不代表A已签收，不执行旧节点CURRENT。

## 2026-11-18 A: live paired in-flight relay delivery accepted

C/D original returns `1f71d312`/`aa7c7e15` verified: 14 file hashes/lengths,
four raw snapshots, both collectors exit 0. Both actual original objects have
unique sender DONE and receiver integrity/time events. On A→B→D→C and
B→A→C→D all adjacent authenticated bindings match, both relays per path
forwarded 9/9 packets with 0/0 failed writes, and receiver transfers verify 1/1
fragment and hash with zero duplicates. Using all six clock sample outer
bounds, the actual in-flight windows overlap by at least 13 ms.
[Independent acceptance and exact object/session/circuit evidence](../artifacts/audit/novonet-diagnostic-release-20261118/candidate-55cafb07/live-inflight-0709/DUAL-END-CORRELATION.json).
A/B are on `55cafb07`; C/D stayed on `5b656189`. No audit restart or resend,
no old test repeated. This is diagnostic relay delivery, not native UDP
multihop, asset finality or automatic five-second restore acceptance.

## 2026-11-18 06:42 A: exact55 Full SUCCESS; B/A updated and healthy

A independently verified Full44136917939 completed SUCCESS on exact55cafb07,
including Rust/native recovery/platform owners;17success and one conditional
PR-only observation skipped. Original existing owners switched B3003007→3167799
and A32592→16528. Actual new EXE/mapped AOEM, original DB/identity/head/height/
listeners passed; independent B69ms/A18ms authenticated Status matches original
root. Exact old exits retained, not claimed graceful drain. C/D stay actual5b.
[Actual receipts and one fresh C/D two-circuit observer dispatch](../artifacts/audit/novonet-diagnostic-release-20261118/candidate-55cafb07/README.md).
No new live object or overlap pass yet. The original coordinator pins new55 A/B
origins and separately pins unchanged5b C/D observers. No new package/copy/update
for C/D, node restart, repeated accepted baseline/recovery or second outage.
Fresh real READYs allow only one new scheduled pair and actual overlap/two-hop/
dual-end audit, never simultaneous command publication as success.

## 2026-11-18 A:55cafb07 packages fixed; one matching Full running, no deployment

Original-owner monotonic diagnostic release source55cafb07 is pushed. Full
44136917939 dispatched exactly once and head verified; still IN_PROGRESS.
Both original packages passed member/archive hashes, actual bundled AOEM/identity,
Windows cold recovery1/1 and Linux package-context network/recovery17/17.
[Fixed pins, actual logs and B→A original-owner plan](../artifacts/audit/novonet-diagnostic-release-20261118/candidate-55cafb07/README.md).
Adapters require actual exact Full success before update. No node is on55 yet.
Only A/B origins require the local envelope; C/D stay qualified5b receivers and
forwarders, no duplicate package/upgrade or fresh window before sender health.
B disk preparation removed only two hash-verified idle db667/5b uploaded tarballs,
not installed programs/data/keys; copies remain on A. No accepted live test or
object repeated; missing actual overlap remains the only paired-workload target.

## 2026-11-18 A: original-owner diagnostic monotonic release implemented locally

The actual1s command-read phase defect is fixed inside original Control: original
CLI --release-at-ms emits one bounded local-only relay-send envelope; Control
reads it ahead, maps through ClockAnchorV1 once and releases in original tick,
without extra socket/task or faster filesystem polling. Original State still
checks ready identity/route/nonce/lease/TTL; cancel/replacement/replay/late/jump
cases fail without renewed lifetime. Normal network frames/quotas unchanged.
[Scope and actual local evidence](../artifacts/audit/novonet-diagnostic-release-20261118/README.md).
Both platforms scheduled5/control4/diagnostic5/CLI3 groups passed (overlaps noted),
strict Clippy passed;87original management checks passed. No new runtime deployment
or in-flight pass. Freeze source for one Full and original two-platform packages;
then only A/B need new local release owner. C/D5b remain compatible receivers/
forwarders, no repeated package upgrade or old live test. Wait for qualified
sender program before fresh C/D concurrency window, not another timing lottery.

## 2026-11-18 A: paired dual-end delivery accepted, concurrency still unproven

A pulled C d2cf9b56/D4c618286;15 original length/hash checks and5 raw snapshots
confirm both exact object/session target integrity/time events, sender DONE,
all3 adjacent authentication bindings per path and both relays9/9forwarded,
0failed writes. Independent transfer tags match1/1verified fragments/hash and
zero duplicates; both collectors exit0. [Final raw correlation](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/live-inflight-0052/DUAL-END-CORRELATION.json).
The two deliveries pass; zero in-flight overlap/minimum461ms gap still fails
concurrency acceptance. C/D no further action, no repeated window/object/restart.
Next actual blocker is bounded diagnostic release coordination in original owner,
not another transport/collector or production timeout/resource/payload change.
No new runtime build/deployment or asset/finality claim in this signoff.

## 2026-11-18 00:55 A: two actual DONEs; in-flight overlap remains failed

Original ConcurrentOnly workload ran once after actual C/D READYd35e4eed,
controller0; no outage or accepted-object repeat. Both sender DONE, but conservative
overlap0: B admission at least461ms after A DONE. Original Control::poll reads
every1s; near-simultaneous command publications were accepted87/602ms later,
while objects lasted41/42ms. No serial-as-concurrent claim or automatic retry.
[Raw negative sample, exact objects and clock bounds](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/live-inflight-0052/README.md).
C/D return SAME observer originals only to complete independent receiver/hop audit.
Next missing fix is bounded diagnostic release coordination in original owner,
not another network/observer, payload padding, production TTL/quota relaxation or
repeated file-poll lottery. No unqualified program update. Prior actual recovered
fresh-object delivery remains accepted and is not rerun.

## 2026-11-18 00:45 A: remaining true in-flight concurrency dispatch

User requested actual overlapping objects only, not completed recovered/direct/
single/double tests. Original coordinator now -ConcurrentOnly exits before any B
outage. Original expiring CLI-generated command bytes are staged/hash-checked and
atomically published on a common management deadline to remove CLI startup jitter;
no TTL/network/asset change, measured admission→DONE bounds alone decide overlap.
87local management checks pass; B actual3003007 preflight idle. No Full/rebuild.
[C/D one fresh two-circuit observation window](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/CD-UPDATE.md).
C/D immediately return real READY/liveness and terminal-file presence under
C-inflight-observer/D-inflight-observer, no node restart/update/send. A pairs new
A→B→D→C and B→A→C→D objects once; dual-end custody and both hop evidence remain
required. No overlap or delivery pass yet; old accepted sequential pair not reused.

## 2026-11-18 A: recovered fresh-object actual dual-end delivery accepted

A pulled C d4d84886/D56d84292, verified15 original file hashes/lengths and5 raw
snapshots. Same Object2239b3c2…/Session54868614…/Circuit55ff173d… has original
sender DONE and unique target custody/integrity/time acceptance,1/1verified
fragments/hash/zero duplicates. All3 adjacent authenticated bindings match;
B/C both9/9forwarded and0failed writes. Both collectors exit0.
[Independent final correlation and raw evidence](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/live-recovery-0034/README.md).
C/D no more action for this item. Fresh route/object after separate restoration
and5b existing upgrade is accepted, NOT automatic/five-second restoration or
old-object resumption. Actual in-flight concurrency remains next; no rerun of
accepted direct/single/double/recovered object. Iroh hop streams are not nativeUDP
multihop; no asset receipt/consensus finality is claimed.

## 2026-11-18 00:35 A: recovered-route fresh object sender DONE; C/D originals pending

A pulled fresh actual C/D READY799b3e5d and ran original guarded continuation
once: A→B→C→D Object2239b3c22fc30a7ba5b29f649d93271639ed439e93e6adb1cecd7cd275de9412,
Session54868614a4c0ecda6864b04af6dad539, Circuit55ff173d2812e03d6dc892f067620355.
Actual admitted→DONE00:34:08.537→08.791 Beijing; controller exit0 at00:34:12.
B matching original circuit forwarded9/9, failed writes0/0, downstream ready.
[Raw sender/B evidence and remaining C/D collection](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/live-recovery-0034/README.md).
C/D return same observer originals only; no new window/object/outage/update.
Receiver custody/integrity/time and C forwarding not yet independently accepted.
Iroh hop-stream relay is not nativeUDP multihop; fresh delivery after separate
restore/upgrade is not old-object or automatic five-second recovery. Historical
failure and unproven actual in-flight concurrency remain, no accepted test rerun.

## 2026-11-18 00:26 A: D5b signed off; four-host update health complete

A pulled D e05c1cfd actual update (PID34148/LocalService, controller0, fixed
EXE/mapped AOEM/manifest and original DB existing), independently authenticated
D37ms/original root/height1. Earlier cancelled UAC is retained. Four hosts now
qualified at5b; no new object/recovery/concurrency success is claimed.
[Original receipts and fresh C/D observer dispatch](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/CD-UPDATE.md).
Original observer/coordinator deployment bindings now match actual5b paths;
83 local management checks pass, A/B live preflights healthy/idle. C/D start one
route-bound observer for the unfinished recovered object only. No node update,
package copy, old test rerun or second B outage. Old circuit evidence is historical;
new session/circuit and both-hop/receiver evidence are still required. True
in-flight concurrency and original failed automatic restore remain separate.

## 2026-11-18 00:01 A: C5b receipt accepted and authenticated; D dispatched

A fetched C's actual7a40624a receipt: PID42764/LocalService, fixed5b Node/mapped
AOEM/manifest, original DB existing, unchanged identity/head/height, failures0
and recovery=false. A independently authenticated C in40ms at16:01:01Z, matching
the original height1/state root. Old PID7384 exit-1 is retained without claiming
graceful drain or a crash cause. [Raw receipts and D current instructions](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/CD-UPDATE.md).
D now performs the already-approved5b existing update using its received package
and qualified owner; C keeps running. No package recopy, new Full, observer or test
object. A must authenticate actual D health next. Missing recovered delivery and
true overlapping concurrency remain separate; old accepted tests are not rerun.

## 2026-11-17 23:49 A: approved5b B/A existing updates passed; C dispatched

After explicit version approval, original B updater switched2880455→3003007;
A original LocalService owner switched22576→32592. Actual programs/mapped AOEM,
original DB existing/custody/head/height/listeners passed; authenticated queries
B70ms/A28ms return original root. Exact-source Full44092874774 remains SUCCESS.
[Original receipts, cleanup/failure records and C dispatch](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/CD-UPDATE.md).
C now uses its already-received5b ZIP and qualified LocalService existing owner;
D waits for actual C health. No new observer/object, old accepted baseline rerun,
init or key replacement. C historical AV and missing true in-flight concurrency
remain separate; this update does not claim their resolution.

## 2026-11-17 23:33 A: exact5b Full success; C/D packages reported ready

A independently confirmed Full44092874774 completed SUCCESS at15:29:41Z, exact
head5b656189. Original final run/jobs are preserved with the qualified packages.
A fetched/fast-forwarded D's actual cae04350 size/hash-only receipt; user reports
C/D ready, no separate C Git receipt present at this check. No node was updated,
no new observer/object launched. [Final gates and existing-update plan](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/README.md).
Maximum-eight was approved as candidate validation; confirm this fixed version's
four-host existing rollout before executing B→A→C→D. No repeat Full, package copy
or accepted direct/single/double-object baseline is required.

## 2026-11-17 A: fixed5b eight-slot packages qualified, exact Full pending

Chrono/netwatch/WMI migration and affected local gates are complete. Windows/Linux
original packages at5b656189 have verified archive members, original AOEM pins,
runtime identity verification and native-state cold recovery; Linux also17/17
selected package-context network tests. Full44092874774 matches5b; security,
Node and process recovery passed, Rust/Windows owner gates still running at this
record. [Fixed hashes, evidence and next existing plan](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-5b656189/README.md).
C/D copy/hash only, no update/observer yet. No actual node changed, unfinished
recovered object and true overlapping concurrency remain unpassed. Keep original
failures; this documentation does not generate a different program candidate.

## 2026-11-17 A: corrected candidate Full security passed; ASAS test cleanup race fixed

Full44089528662 exactly matches6d2c3e7d and its Rust security checks passed. Node
checks failed30/31 because one pre-existing controls test used a20ms sleep after
internal task completion, before the original session owner finished cleanup.
Only that test now waits for the existing public event terminal status; original
assertions/10s deadline and all production ASAS code remain unchanged. Windows
and native Linux Node22 tests each31/31 pass. [Failure and exact scope](../artifacts/audit/novonet-concurrent-recovery-20261117/capacity8/time-migration/NODE-GATE.md).
No Full success/deployment claimed; retain previous runs and freeze the corrected
candidate before final Full and original package qualification. C/D do not open
another observation window or update using an old/unqualified package.

## 2026-11-17 21:07 A: approved chrono migration locally qualified; freeze corrected eight-slot candidate

User approved chrono and the Iroh/netwatch/WMI dependency migration without relaxing gates.
Root product graph now has no chrono: original report/preflight callers use existing time0.3.53;
original netwatch0.19.3 sources retain WMI0.18.4 route queries with only unused default datetime
features disabled. Original Windows WMI query actually passed. Windows/Linux each147 selected
tests, strict Clippy(lib/tests/bins), formatting and root/runtime Audit/Deny passed. Initial overly
broad mainline selector76pass/25legacy-Host-refusals per OS remains recorded; no compatibility
mode enabled, no CI selector/assertion/ignore changed. [Exact migration and raw evidence](../artifacts/audit/novonet-concurrent-recovery-20261117/capacity8/time-migration/README.md).

Next: one corrected-source Full, then new original fixed packages/qualification. Earlier e666
Full security failure is not overwritten; assembled e666 packages are not relabeled. No A/B/C/D
update/new object/new observer in this migration. C owns Android/NOVOCHAT; this is not a phone
qualification. Other consuming Cargo roots must carry reviewed path patches explicitly.
Resume unfinished recovery-object and true in-flight concurrency only after candidate and actual
host readiness; accepted direct/single/double delivery baselines are not repeated.

## 2026-11-17 20:25 A: eight-slot local pass; Full security / chrono migration decision

Eight-slot original-owner implementation/source e666a6fb is synchronized, Windows/Linux local groups13/72/34 plus final stress1 and strict Clippy passed. One exact-source Full44079683535 started; actual security job134026939952 failed root Cargo Audit on libz-sys1.1.31 / RUSTSEC-2026-0544. Cargo regenerated the minimal root-lock patch to1.1.33, root/runtime local Audit now pass and both actual native-node dependency trees are unchanged. Root Deny then independently rejects chrono unmaintained / RUSTSEC-2026-0543 (no patched version); root bans/licenses/sources and separate runtime Deny pass. No new ignore or policy relaxation. Chrono includes own node/governance/rollout tools and Iroh→netwatch→WMI, not an EVM-only blocker. Its migration or a policy exception is beyond the eight-slot parameter qualification and requires direction. [Actual originals, correction, remaining scope and recommended next work](../artifacts/audit/novonet-concurrent-recovery-20261117/capacity8/ci-first/README.md).

Both e666 whole packagers assembled outputs but these are not qualified archives or deployments. No C/D new package dispatch, observer window, actual node restart/update or new object. No second manual Full while this known local blocker persists; original Full finishes without cancellation or false green. Capacity-eight SDK-default-four/signed-session-four/128KiB/auth/timeouts/AOEM/ledger boundaries remain. Real-host concurrent/recovered-object acceptance remains open.

## 2026-11-17 A: original shared Carrier gate blocked recovery; max-eight candidate approved

Actual fresh observer recovery-only attempt19:05 failed before object send: B had three installed bound business routes, authenticated relay ingress, then original shared-four admission refused downstream (`carrier connection limit: no permits available`, ordinal47). Same-route circuit had no downstream readiness/forwarded data. No second B stop or completed-pair resend. User approved maximum-eight **candidate validation**, reusing existing ServiceConcurrencyV1, not a deployment bypass. Generic SDK default four, signed relay sessions four, shared128KiB/min, authentication, timeout, AOEM/ledger and consensus unchanged. Original main owner implementation/new three-resident-route regression and Windows/Linux gates are in progress; not a candidate/Full/deployment or network acceptance claim. [Original evidence, implementation scope and next gates](../artifacts/audit/novonet-concurrent-recovery-20261117/capacity8/README.md). C/D preserve current bounded outputs, no new db667 observer or retry. Completed pair had no proven in-flight overlap; concurrency/recovered-object acceptance still open.

20:02 local gates complete: Windows/Linux original native-owner13/13, corrected Carrier72/72, original resources34/34, final corrected new-case1/1 and strict Clippy exit0 each. First memory-fixture expectation and Clippy loop-style failures remain raw; corrected only tests, not resource policy. Source now freezes for one exact Full and original whole packages. No eight-slot package installed; real-host concurrency/recovered-object acceptance remain open.

## 2026-11-17 18:55 A: crossed deliveries accepted, no overlap; original B owner restored

A independently matched20 returned file hashes/15 raw C/D snapshots, both actual crossed-route objects' sender DONE + target integrity/time/full-fragment/hash and all actual relays/three adjacent bindings. In-flight intervals do not overlap; **not concurrent acceptance**. B controlled stop exposed the original management helper assuming a persistent unit: `systemd-run --collect` removed it before `systemctl start`. Original failure retained; same fixed Node/AOEM/config/network/custody/original DB restored via original launch owner, new2880455, authenticated Status63ms/original root. Automatic/five-second recovery is not passed. Original management recreation fixed/tested6 cases Windows/Linux; no new runtime/Full. C's new AV and authorized restart7384 are retained, not explained or fixed here.

Both old observers exited0 early from closed circuits including unrelated routes. [Only unfinished recovery-object observation now](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/CD-UPDATE.md): existing observer stop condition/receiver session bound to the exact recovery route, all raw output preserved, original limits;67 local checks pass. New real READY/liveness under C/D-recovery-observer, then original guarded A continuation sends one new recovered-route object without resending the completed pair or a second outage. [Actual original evidence and independent result](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/live-1828/README.md). No direct/single/double baseline rerun, packet collector, new architecture or business/finality claim.

## 2026-11-17 18:16 A: D db667 accepted; one concurrent/recovery window dispatched

A fetched actual D success9f9aeeb8 (retaining earlier pre-entry cancelled UAC separately), checked reported56148/LocalService, fixed Node/loaded AOEM/manifest, original DB existing/custody/head/listeners and no exit/error. A's fixed client independently authenticated D48ms at10:15:01Z, original height1/root unchanged. All four same-version update/respective authenticated health gates are accepted; concurrent objects and B interruption/recovery still pending.

**C/D now each launch one fresh original version-bound sequence observer, immediately return actual READY/liveness/process metadata, keep it across the one continuous workload**, per [current exact dispatch](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/CD-UPDATE.md). No new package/Full, node re-update, old crash or accepted single-object/direct replay. A/B owners healthy/idle at10:16:02Z, A elevated/coordinator ready. [Actual D receipt and independent query](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/D-existing/). Sender DONE, each actual relay and final target custody/integrity remain independent from Status and eventual business/finality.

## 2026-11-17 17:32 A: C db667 accepted, D existing update dispatched

A fetched/fast-forwarded actual C receipt9b27a904: C reports PID45220/LocalService, exact db667 Node/loaded AOEM/manifest, original DB existing/custody/head/height, healthy original listeners and no unexpected command/exit. At09:31:51Z A's fixed new client independently authenticated C44ms, original height1/root unchanged. C's config/progress paths match the existing tested CommonRoleFix observation binding; no new tool/window needed. [Actual receipt and independent query](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/C-existing/).

**D now executes the same qualified original-owner existing update** per [CD-UPDATE](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/CD-UPDATE.md), with the already-received ZIP; no new stop request, old crash test, package recopy or observer. C remains running. B/A/C update/health and exact candidate Full are passed, but real concurrent objects/B interruption-recovery have not run or passed. No repeated accepted direct/single/double one-object sample and no business/finality claim.

## 2026-11-17 16:53 A: db667 Full passed; B/A switched, C next

Actual common95 concurrent preparation was blocked by the original owner locking opaque forwarding as endpoint occupancy; no new diagnostic object or B interruption occurred. Minimal original-owner fix `db667aa420477e76766352c8efd7fea83fdb6eac` passed Windows/Linux role/endpoint regressions and strict Clippy, exact Full44037306356, both fixed packages/actual runtime KAT/native cold recovery and complete archives. Normal production routing, auth, original4-session/128KiB budget, diagnostic60s lease, AOEM and ledger remain unchanged.

B is now db667 PID2840424/UID996, A PID22576/LocalService, original DB existing/custody unchanged. A authenticated B60ms and A31ms, original height1/state root unchanged. C/D new ZIP receipts confirmed. **C now follows [the exact candidate existing-update dispatch](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/CD-UPDATE.md); D waits C health.** No new observer/window, old crash test, repeated direct test or package recopy. Source sync and this successful update/health do not establish concurrent/recovery acceptance. Prior single-object double relay remains accepted independently. Original failure/attempts and all candidate/actual host evidence are [here](../artifacts/audit/novonet-concurrent-recovery-20261117/candidate-db667aa4/README.md).

## 2026-11-17 CURRENT：四机新版联调，旧崩溃调查暂停

15:09最新：已签收C/D当前原记录da597f19并核8份快照SHA；本轮没有发新诊断对象、没有停B。
实际阻断是原relay::incoming对中继也占了端点单飞Occupancy，A路线占用B转发时B新预约
被拒already reserved；旧nonce的20秒超时不是本次新nonce失败。另修执行器Cli内置别名冲突，
保留两次准备失败，不靠重启清状态。原owner最小修正已完成：端点单飞保留，转发只计原
HopBudget/连接4/共享128KiB预算，诊断60秒转发期限不变；不加额度、不改主业务/身份/AOEM。
Windows/Linux各新并发原reactor回归1/1、端点边界负例1/1、两平台严格Clippy均过。
这不是四机并发/恢复通过：须制作一次新固定候选及对应Full，再更新绑定实际程序后验收。
旧95现场保持，不再重跑已过直连/单/双单对象，也不执行旧95编排绕过保护。
[根因、原始拒绝与候选下一步](../artifacts/audit/novonet-concurrent-recovery-20261117/README.md)。

14:05当前下一项：用户批准并发对象与B中继断开恢复实网，既有一条双中继验收不重跑。
复用原单飞诊断owner：A/B各一对象、不同最终接收者，实际路由A→B→D→C与B→A→C→D；
以原admitted→DONE区间及管理通道实测偏差界确认在途重叠，无重叠不能记并发通过。
随后仅B原unit停5秒、原库existing启动，检查旧circuit断开、新Session重认证和新对象交付；
不冒称同一旧在途对象自动续传。原预约60秒照常等待，不改限额/TTL/身份，也不换包/新Full。
52项本地观察/时序/新鲜度检查及A/B管理预检已过，**并发/恢复实网尚未执行或通过**。
C/D仅需同一新鲜观察窗口，各用原Watch-Relay加TerminalCircuitCount4，64快照/2小时上限
不增加，立即回真实READY/liveness；之后A一次执行整个工作包，不再逐项等新窗口。
[唯一执行交接](../artifacts/audit/novonet-concurrent-recovery-20261117/README.md)。

13:36最终签收：A已下拉C原始快照4f2404c9，独立核对C/D共14份原文件SHA；
C同Circuit/Route的cursor1实际8/8→9/9包、9380/9044字节、写失败0/0，B cursor0
同计数且无写失败；A–B、B–C、C–D三段认证绑定逐段吻合。A发送端DONE/退出0与
D同对象/Session完整接管、hash及原有效期内时间验收组成完整证据链。
**本次一条A→B→C→D真实双中继诊断对象交付通过，C/D无后续操作，不再重发/重开。**
[最终独立关联和边界](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/double-relay-20261117-1214/README.md)。
C观察器退出码仍未知，D退出0；不伪造接收侧DONE时间。仅此非业务对象样本，非原生UDP
多跳、并发/恢复/TPS、资产回执、三票最终性或正式主网发布验收；既有安全/预算未改变。

13:19最新：A已读C/D6556e0c6；D的8份原文件哈希、3原始快照独立核验，
同对象/Session/Circuit最终接收完整、hash验证及原有效期内时间验收通过，观察器退出0。
A DONE、B转发、D接管已关联；C只推派生摘要（9/9包、写失败0/0），未推3原始快照。
密文中继不应要求端到端Object ID/Session可见；C只补已有relay-*.json及SHA，按
Circuit/Route/cursor1核对，不新采集、不重发、不重启、不开发工具。D无需操作。
完整逐跳验收仍待C原文件；不伪造D receiver DONE时间或Business Receipt/最终性。
[当前证据结论](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/double-relay-20261117-1214/README.md)。

12:14最新：A已读C/D9501a49d新鲜READY及心跳，实际发送唯一A→B→C→D对象，
原发送端12:14:43.971 DONE、控制器退出0；B同circuit实际双向转发9包/9380字节及
9包/9044字节、写失败0/0。C/D只回现有观察器结果、匹配快照和退出，不重开/重发。
对象e36af9089c38c9ec19835255934f04aa308ebc84c0bf42051bfa39a6026fcd5d，
Session b960c007c82275a45ee5c45f75705aab；C转发与D完整性/时间/最终接管尚待回证。
[实际A/B记录和关联边界](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/double-relay-20261117-1214/README.md)。
不是原生UDP多跳、资产业务回执或共识最终性通过，不用A DONE覆盖D验收。

11:52最新：A已下拉D `93291988`，签收PID23340/LocalService原库existing，实际新EXE、
已加载AOEM和manifest匹配；A→D认证34ms，height1及原A/B/C同状态根，健康检查通过。
四端新版升级已完成，尚未发送双中继对象。A/B各3个已验证locator，A已提权、原一次性
双中继发送器已按新制品绑定准备；只读观察路径/新鲜度38项检查通过，不需换包/新Full。
C/D现在各启动一次原Watch-Relay加Common95路径绑定，立即推真实READY及liveness，
不要等2小时窗口结束。两端就绪后A直接一条A→B→C→D，核两跳转发和双端DONE；
非原生UDP多跳、非资产回执。详见
[当前四机双中继交接](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/CD-UPDATE.md)。

09:55最新：A已下拉并签收C `42de31f7` 的新整包existing回执，PID38500/LocalService，
实际EXE、已加载AOEM及manifest哈希匹配；此前E盘包不可用已被后续成功回执覆盖。
A→C新版本认证Status400ms，height1、原A/B同状态根、非恢复且无投影错误；不是DONE/最终性。
C保持运行，现在直接派D执行同整包原库existing更新，回真实PID/哈希和config/progress路径，
不重传包、不复跑C更新/旧崩溃/直连对象，也暂不开观察器。具体动作以
[当前D更新交接](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/CD-UPDATE.md)为准。

07:34最新：Full43952963989最终全绿（程序1eb）；Linux/Windows新包资格通过。已先B后A
以原库existing实际更新，新PID为B2621117/A34208，实际加载新AOEM，身份与状态投影未变；
认证Status B68ms/A30ms通过。C/D整包已收，当前只派C执行本机既有owner更新，D待C健康；
不再等旧崩溃测试，不重复直连对象，不提前开启观察窗口。实际回执和C具体执行要求：
[当前C更新交接](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/CD-UPDATE.md)。
下方05:41的A停止、Full待结束描述是已收口的历史状态，不再当作当前阻断。

05:41更新：用户清理后Windows95新整包及实际新EXE＋AOEM冷恢复已过，C/D只先复制核验。
Linux FullMax已过，Linux整节点继续；Full43952963989仅Rust尚在运行。另已实际读到A于
04:45因磁盘满/OS112排空退出1，当前不是在线PID28124，也不是C类AV；旧Carrier成功
只代表停机前。清理后恢复直接用新候选existing，不重跑旧包。准确制品和执行入口见
[当前95整包交接](../artifacts/audit/aoem-latest-57090b6b-20261117/candidate-common-95ec9c2e/README.md)。
下方早一轮“Windows失败/旧PID运行”的观察已被本段与该交接更新，不作为当前健康证明。

用户要求 A/B/C/D 全部升级到新版共同运行时代码与最新合格 AOEM，继续原 owner 双中继。
A 已签收 C 的 e4fad013 和 D 的22542a2b：C 新 EXE 资格已跑完但当前停止，D 新 AOEM
已资格、未部署；A 当前仍是 a254 EXE＋新 AOEM，不冒称新节点已升级。A→B Carrier
已有实际 QUIC/Stream/绑定认证/发现成功；当前缺四端新版 owner 就绪及 B 管理入口。
Windows353原型整包已构建；最终1eb整包重建因C盘0空间失败，未发合格候选、未部署。
新源码1eb＋最新AOEM的14项网络回归已过，Linux构建继续；Full43949487014最终失败，
已修实际入口/工具链问题并仅发一次修正Full43952963989（head1eb），尚未最终通过。
不取消、不反复触发，不改网络安全或重跑旧BASE；清理工具策略拒绝的旧debug缓存待用户执行。
执行现状、准确哈希、失败和下一步以[四机当前入口](../artifacts/audit/aoem-latest-57090b6b-20261117/FOUR-HOST-UPDATE.md)为准。
下方历史崩溃分析保留为历史证据，不是当前 C 的执行指令；不让 C 继续旧版复现或降级。

## 2026-11-15 A签收C2336cfc9：异常容量近因确定，尚无已证根因补丁

同一PID3848 Full Dump实际申请39582418599952字节（36TiB+16）、align16。
A独立重算扩容算术吻合；archive_work首次插入前空表mask/items异常，并与XMM6/XMM7
及保存槽一致，不是正常负载内存耗尽。stderr实际读取结果尚未随公开回执提供，分开记录。
A继续核对原EXE直接调用，四个显式使用该向量寄存器的函数均有保存/恢复指令；
仍缺首次错误写入/恢复的因果证据，不扩大额度、不禁用status或换容器冒充根因修复。
C直接复用同dump的对象和未覆盖保存槽缩小区间，不重跑BASE、不开发新工具。
[精确调用位置和当前离线处理顺序](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。

## 2026-11-15 A签收d9bd6c25：自然BASE崩溃已复现，停止重复运行

C目标3848约4分钟自然0xC0000409/参数7，其他三进程为控制器收尾；无新增WHEA，
不是硬件排除。213986993字节Full Dump已留C。A复核同哈希原EXE机器码，把status栈
定位到archive_work新对象首次插入reads_started，经Hashbrown分配返回空进入Rust abort。
正常空表在此应申请52字节/对齐16，但实际申请量尚需同一dump与stderr核验；不凭OOM
名称认定物理内存耗尽，也不把旧AV/RBP故障自动并为同根。代码未作猜测修改。
C直接复用已有离线CDB和提权只读入口，恢复实际申请量/表字段；不重跑BASE、不另造工具，
不禁用status掩盖故障，不更改原库或身份，双中继暂停。
[当前精确地址、取证顺序与A机器码](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。

## 2026-11-15 A签收d2967fa1：C直接执行既定无调试器BASE

CPU/内存短筛结果已读，不当硬件排除；C原PID27928的EXE/旧AOEM与A一致仍曾崩溃，
不能用后继966差异解释全部失败。当前实际缺项是LocalService隔离材料准备及一次既定BASE，
不是新工具资格。用户要求直接推进，C沿已验证转储目录和原prepare/负载入口执行，
启动即回实际PID/账户/哈希/截止，不再等A批准，不重做Prime95/WinSAT/PT/TTD或正常样例。
若入口失败回具体错误并只修该阻断；未复现不猜测生产补丁。A本轮未启动C/A节点。
[直接执行顺序和停止条件](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。

## 2026-11-15 A签收C633f223a：A实测对照已交付，C收敛硬件/软件原因调查

A已核对C的LocalService转储资格与两条WHEA处理器已更正错误；不能直接归因CPU/TTD。
推送时收到并合入8e7b16b7：保留WHEA扩展为14条（12条早于调查），WinSAT仅短时带宽
观察；A原加载AOEM与C后续966确为不同哈希，不重复WinSAT、不误称同库同负载。
A本机实查：a254在既有24小时运行上限后退出0，任务结果0，现在无节点进程；
近期Application1000/1001无节点匹配、System无WHEA。该次无资产执行批次，非同负载证明。
EXE/AOEM完整哈希、系统硬件、任务时间、退出和计数及证据缺口已公开投影，原日志留本机。
用户要求不再重复构建诊断工具。C复用现有LocalService转储/准备owner，完成一次既定
有界无调试器BASE，同时核对WHEA；按真实结果选择单变量对照，禁止用正常样例反复通过
或工具资格替代故障定位。不等待A再次签收，不操作D，不恢复双中继。
[当前具体交接与A公开证据](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。
本轮只交接，未执行BASE、未改变原库/钥/节点/BIOS或诊断产品边界。

## 2026-11-14 C：分区和实际Writer源码入口已接通，整套诊断仍明显改变时序

C独立完成一次方法资格，Git用于通知，不等A签收。A/B事件保留改善、Writer源码
入口与Formatter/pad参数可对应，但查询P50 +56.4%、P95/P99 +24.9%；拒绝当前
方法长期复现。重建标准库改变代码布局，不能当作原875/a254机器入口寄存器证据。
原崩溃/首次破坏未定位，生产代码/原库/身份不动，双中继不恢复。
见[方法结果及缺口](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-partitioned-writer-20261114/README.md)。


## 2026-11-14 C独立低干扰资格：P50与保留窗口未通过，不进行长时复现

C已完成新a254诊断副本、固定AOEM966、TLS定长记录器和两版ABBA；不等待A签收。
修正CDB使用`-hd`后，记录开启的查询P50仍增加33.1%、P95增加17.9%；结束保留窗口
被格式化记录占满，生命周期历史未保留。故不放行这版采集器的长期复现。
原崩溃/首次参数错位未定位，无生产修复；原库、身份、旧失败不动，双中继暂停。
公开结果和明确覆盖缺口见[本轮报告](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-lowimpact-capture-20261114/README.md)。

## 2026-11-14 C独立推进：取消等待A签收，受控隔离采集已获用户授权

覆盖下节“等待A确认后再定运行条件”：C自行负责采集设计、隔离验证与根因调查，
Git仅协调通知。原库/身份/失败保留，部署C不重启，双中继不恢复，不猜测性修复。
本轮独立诊断构建与一次回环联合运行结果见[当前交接](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)；
目标及三个同伴退出0，原崩溃未复现；断点使单次查询耗时161秒，未覆盖连续周期。
不原样重复该负载；首次错位仍未定位。编译/采集门禁不等于修复，A未签收不阻塞C。

## 2026-11-14 C：以20e010df终止本轮事后调查，等待A签收采集提案

用户确认20e010df为证据边界和阶段性终止点；原始崩溃未修复。停止重复旧样例和
事后推演，保留原库/身份/失败，冻结生产代码，不启动C、TTD或双中继。
[A签收请求与同次失败采集提案](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)
已置顶：输入来源、有限对象生命周期、调用边界、隔离重放及原二进制/插桩取舍。
本轮仅交接，未实现采集、未运行新负载，不计作新的根因进展。A_ACK_PENDING；
A通过Git确认后再确定受控运行条件，文档签收不自动授权启动或放行双中继。

## 2026-11-14 C：核实失败证据缺口，尚不具备原故障确定性重放条件

C重验三份原MiniDump及另一份17:08 PageHeap完整转储的哈希和实际内存覆盖。
37796缺少所查代码片段/目标record的完整捕获；别次完整转储不能补作同次证据。
原875/a254 EXE/PDB及AOEM966已找到核验，但选定原归档没有逐包输入与边界历史。
[逐份覆盖结果、材料核验及缺失证据](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-failure-evidence-gate-20261114/README.md)。
没有执行新负载，不能称为“本轮重放未复现”。首次破坏未知，未修复；不再追加
普通快照测试。原库/身份不动、节点不重启、双中继暂停；Git通知不等于A签收。

## 2026-11-14 C：完成回收/查询所有权审查，停止追加普通快照样例

C直接核对875/a254原Git源码：2048条delivery记录为按值标识与计数，淘汰和遍历
受同一Mutex保护；256条transport证据是另一套拥有式元数据，不是正文对象池。
发送Arc、接收chunks移交及原tick的网络/poll/status顺序已核对，未发现可证明的
淘汰后借用、越界写或竞争。所选12文件中11个源码相同，不代表二进制/DLL相同。
[所有权路径、原源码哈希、DMP关联与停止条件](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-object-lifetime-audit-20261114/README.md)。
首次破坏指令仍未知，收回“淘汰数支持悬空引用”的推断，不再默认追加正常满载
样例。未新增运行测试、改生产代码、访问原库/身份、启节点或恢复双中继。
C独立调查，Git通知A不等于A签收；A/D无需为本回执操作，C仍NOT READY。

## 2026-11-14 C Phase6：原负载与故障线程补齐，首次参数破坏仍未定位

C独立只读取得PID37796原最后进度及转储线程/迭代器现场，没有等待A代查。
最后完整进度无交易执行积压，但delivery满载2048条、已淘汰213510条；
故障帧分段1340＋708，较早进度推得1530＋518，不能混为同一时刻。
后台线程等待栈属于WER处理时刻，不能证明故障前一直空闲或排除并发。
新隔离原status验证补满载环绕条件；384组选择边界一致，目标/CDB退出0，
首次采集脚本失败保留。未还原旧堆历史/网络时序，未找到首次错误指令。
[原负载、线程、调用记录与缺口](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-real-load-phase6-20261114/README.md)。
未修改生产代码、访问原库/身份、重启部署节点或运行TTD；C仍NOT READY，双中继暂停。
Git仅同步结果，不要求A/D代查、重跑或操作节点，也不把推送称为A签收。

## 2026-11-14 C Phase5：独立调查原状态查询上下文，不等待A代查

C独立核对原周期JSON进度、历史JSON-RPC、原生二进制Status和观察器读文件路径；
认证Status不调用该JSON快照，观察器也不发起此RPC。A仅接收Git协调，不作为本机调查审批。
一次新增隔离验证实际使用原RpcLifecycle/ResidentNode、AOEM966、新测试状态及回环peer，
独立线程在status窗口内完成3次原记录API更新。448组完整边界一致；另33组覆盖不完整，
不计通过。程序/CDB退出均0，原shutdown完成，但首次错误仍未找到，不提交猜测性修复。
[调用链、线程所有权、实际记录及缺口](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-state-context-phase5-20261114/README.md)。
未重跑旧64次样例，未启部署节点、访问原库/身份或运行TTD。C仍NOT READY，双中继暂停；
本次Git同步不等于A签收，不要求A/D重复运行或升级。

## 2026-11-14 C Phase4：独立调试构建及真实边界记录，根因仍未定位

已从原a254源码构建优化诊断节点，库/bin均保留调试流并核验EXE/PDB身份；未运行该节点。
隔离原profile快照采集64次格式化/576条记录，48次尾调用、16次正常零填充调用；
目标与CDB退出均0，1638项离线比较一致。两次采集入口失败也保留，不作为节点复现。
首次错误指令仍未知，不提交猜测性修复，不碰原库/身份，不重启节点，不恢复双中继。
[完整结果与限制](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-boundary-phase4-20261114/README.md)。
新PDB仅属于新诊断EXE，不能加载原转储；C继续NOT READY，推送不等于A签收，D无需操作。

## 2026-11-14 C精确符号续查：原a254 PDB已接收，首次破坏仍未定位

C已核验ZIP/PDB哈希及EXE/DMP/PDB一致GUID/age，并离线加载原PID37796。
最终Formatter.buf与writer接收地址仍不一致；不能从快照反推首次错位指令。
原PDB的16个node编译单元均无调试流/文件记录，整体Has Debug Info不代表局部变量齐全。
另确认elapsed+50df是最近符号，非该PE函数真实归属；不据此修改elapsed或标准库。
[精确符号结果、命令与证据限制](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-symbolized-original-20261114/README.md)。
不再重复已通过样例；原库/身份/失败现场保留，无生产修复、节点启动或双中继放行。

## 2026-11-14 C接收核对：A回执已读，原a254符号ZIP仍待复制

C已同步0470f343，确认A签收e1e3a999。原PID37796转储和a254 EXE哈希复核一致；
本机限定接收目录未找到精确原符号ZIP，E盘已有PDB仍为875，不能替代。
收到ZIP后才继续同一原转储的精确符号/外层参数分析；本轮未重跑样例或启动节点，
首次破坏位置仍未知，无生产修复或双中继放行。
[C实际接收状态与下一步](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/C-symbolized-original-20261114/README.md)。

## 2026-11-14 A签收C e1e3a999：原a254 PDB已找到，交C离线续查

A已fast-forward读取C后继崩溃及Phase2/3，确认C NOT READY；11月12日部署健康
不是现在可发送双中继的依据，不再要求C/D重开旧观察窗口。
A原构建目录找到novovm_node.pdb，PDB与原EXE CodeView均为
E16D0B11-47CA-4EBF-B482-C91380CC57F8 / age1；EXE SHA1758e4a7...0d0c46d
与固定a254包一致。未重编译，未更换程序/库，未启动节点。
原符号已封为4709244字节ZIP，仅本地交付，不公开上传PDB/DMP。
具体复制路径、双层SHA256及C对同一37796原转储的离线符号加载/外层参数核对指令见
[CD-UPDATE当前交接](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。
本条完成A符号定位责任，不冒称C已经收到或根因已定位；C回实际收包与符号加载结果。
无需D操作、重传原程序包或新Full。手机仍由C独立推进，不由A重做。

## 2026-11-13 C回传：完整服务ML-DSA钥、显式验证器与缓存容量

NOVOCHAT main [c5e7b7d](https://github.com/XujueKing/NOVOCHAT/commit/c5e7b7d)
已推送；共享源码213d8de3仅增加QualifiedTransportVerifier::from_runtime，并让
原load入口复用它。只借既有合格ML-DSA-65 runtime的只读验签owner，不持有/生成
私钥，不授予注册、设备、联系人或资产权限；客户端三处受审pin已更新，无绕过门禁。

profile、ServiceLeaseBook恢复/更新显式传入同一verifier，信任根保留完整1952字节
服务公钥；本地slot承诺只作索引，不能冒充验签公钥或可信授权。原生/TLS载体当前
策略守卫复用已选完整钥，仍检查原撤销、时间与请求权限；Iroh自身载体钥不是服务钥。

旧8KiB邀请边界无法容纳完整新记录，本轮统一本地profile上限为1MiB+128KiB；
缓存按原4MiB checkpoint及JSON最坏转义推导容量。SQL schema1→2先认证解密旧行，
再事务内复制原sealed BLOB；失败回滚。加密envelope/密钥/AAD不变，聊天数据库不动。
正式候选安装前须另备节点缓存回退材料，不承诺旧程序直接读取schema2。这只是本地
格式容量，不放宽网络超时、带宽、节点/资源配额或资产限额。

实际AOEM组件20通过（custody10、locator5、生产缓存源码5），隔离all-features/
all-targets Clippy -D warnings通过；Flutter26及analyze、Kotlin168通过。完整Android
cfg仍46项编译错误、exit101（此前85）；不能把组件通过当完整App/手机验收。
[实施说明](https://github.com/XujueKing/NOVOCHAT/blob/c5e7b7d/docs/implementation/SERVICE_PROFILE_MIGRATION_2026-11-13.md)；
[原始日志/46项清单/命令和哈希](https://github.com/XujueKing/NOVOCHAT/tree/c5e7b7d/docs/implementation/evidence/service-profile-20261113)。
提交后8项证据blob哈希与清单一致。未生成/安装主聊天APK、未操作手机或节点；原身份
历史、附件、v108/v110回退材料保持，正式资产与生产收费关闭。完整网络回归未执行。

C下一项继续持续邮箱/L4、服务回执验证、原生/控制会话的独立通信身份及可信远端绑定；
不重复造协议，不用旧32字节钥转换或假快照解除拒绝。A继续原节点实网主线，无需代写
消费端；本节是Git通知，尚不能据此宣称A已签收。历史CURRENT节点操作指令不在本轮执行。

## 2026-11-12 C集中完成NOVOCHAT：1389c84主App通信持钥接线

用户明确：NOVOCHAT客户端迁接、编译、Android持钥和双机验证由C独立连续完成，
不逐项拆给A等待。必要共享接口缺口C可核对本地SUPERVM/AOEM后修补，通过Git
同步、避开并行文件；不能在客户端假造可信账户/设备/联系人状态。A继续节点实网
主线，不重复开发Android。C其他会话的节点调查与本次客户端提交分开登记。

NOVOCHAT1389c84源码、585444c证据规则已推送。原通信持钥实现抽出唯一runtime，
主App使用novochat_core内部registry＋独立ApplicationCommunicationNative薄门面，
不再借独立测试DSO的句柄；本地身份承诺和epoch在原生侧再次核对。原probe保持
自身门面/持钥，不换用户钥。借用TransportIdentity检查原操作authority前后状态，
owner close/drop使全部借用失效；Weak引用不延长空闲旧句柄的私钥寿命。

prepare_native已消费该App owner的独立discovery_transport；它不把local active
状态当成已注册UCA。真正UCA/设备/联系人绑定、启动恢复编排及持续Mailbox/L4等
调用者仍须C继续迁接，不以临时默认key或假快照解除拒绝。

实际AOEM持钥回归9项通过、Kotlin168项通过；独立JNI Android check/Clippy及
host all-features/all-targets Clippy均exit0。主AppAndroid 86→85项错误，仍exit101，
新持钥模块无编译错误；另两条warning为relay非桌面平台purpose_keys的unused参数。
未生成/安装主聊天APK、未操作手机，本次无节点操作、真实资产和收费保持关闭。
[完整边界及源码](https://github.com/XujueKing/NOVOCHAT/blob/1389c84/docs/implementation/APPLICATION_COMMUNICATION_OWNER_2026-11-12.md)；
[公开日志/85项错误/提交后哈希](https://github.com/XujueKing/NOVOCHAT/tree/585444c/docs/implementation/evidence/app-communication-owner-20261112)。
这是Git通知，不表示A已签收；手机身份历史、v108/v110及四机现场不在本轮修改范围。

## 2026-11-12 C实际退役旧WSS：18b0711，Android完整盘点仍未通过

NOVOCHAT main `18b0711`已推送：删除App旧WSS Profile/worker、端点编解码、手工
accept的JNI/Kotlin/Dart入口、专用relay adapter及启动/诊断脚本；不保留开关隐藏的
旧执行器。旧输入拒绝测试保留。UDP/Iroh共用Android默认网络检查迁出后继续复用，
原生NativeEndpoint/NativeObjectLink及合格载体不删。原聊天包/身份/历史未操作。

默认core245通过/1原有忽略，Flutter定向25、Kotlin JVM168、隔离locator3、工具
2/2/20通过；均有原始输出。Android JNI拒绝用例仅静态检查，网络回归未通过编译。
重要纠正：之前“剩7项”只计Windows通信lib；本轮明确NDK后首次检查完整Android
cfg，最终86项编译错误（0 warning），Windows仍7项。不能把默认库或持钥单测当
完整App通过，也不能将不同平台集合的差额直接认定为新增回归。

C继续消费端通信持钥、完整服务钥/回执验证及native/L4调用者迁接，不把86项交给
A代修、不复制共享协议。A继续真实UCA注册恢复、设备授权、联系人通信钥连续性及
可信服务来源；缺映射保持拒绝，旧聊天钥不转Vec冒充ML-DSA。此前App UID持钥
证据保持其独立范围。未安装主聊天候选，未启动任何节点/观察/发送，正式收费关闭。

[源码清理与完整边界](https://github.com/XujueKing/NOVOCHAT/blob/18b0711/docs/implementation/NATIVE_TRANSPORT_RETIREMENT_2026-11-12.md)；
[原始结果/逐条Android错误](https://github.com/XujueKing/NOVOCHAT/tree/216b9a7/docs/implementation/evidence/native-retirement-20261112)。
派生JSON的LF字节哈希另在NOVOCHAT216b9a7纠正，九项提交后哈希复核一致；原stdout不变。
这是C通过Git通知，尚未取得A对此提交的接收回执；A现有a254双中继工作不变。

## 2026-11-12 16:54 A签收D：四机a254更新完成，转双中继观察

A已下拉D1d7714c3，核验15项证据哈希和exit0；D新进程20876/LocalService，
原库existing、用途材料/进度保持。A→D认证Status38ms、原状态根一致。
B/A/C/D均已完成a254更新与外部健康查询；这不是双中继DONE通过。
现在C/D各启动一次原Watch-Relay观察器，立即回实际READY及心跳，不等窗口结束；
A已处于提权环境，复用原发送者按A→B→C→D单次取证。B、C转发与D完整接管独立核对。
具体参数和原脚本哈希见
[a254 CURRENT](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。
不重跑直连/单中继，不换包/重启，不新增Full，不把Iroh逐跳交付写成原生UDP多跳。

## 2026-11-12 C方向纠正：继续NovoRUDP原生，不扩展旧WSS分支

用户指出主线早已确定为NovoRUDP。C复核app_native_io的NativeEndpoint/NativeObjectLink
与app_mailbox_native原生主路径，同时App仍有ApprovedProfile::Wss旧入口。
3065555优先消除旧WSS编译错误选错优先级；该增量未装入主聊天包，也不能代表原生
迁接成果。NOVOCHAT `11b19d4`已纠正文档/执行方向：不继续扩展WSS，不设置默认或静默
后备，先盘点调用者及退出条件，沿原生身份、发现、对象发送和设备回执收敛。
原WSS代码/未执行测试及手机历史如实保留，不靠删除断言或回退包制造完成。

## 2026-11-12 16:44 A签收C a254恢复：认证查询通过，D执行下一台

A已下拉C27efa42c并核验22项公开证据哈希，原更新器exit0、a254实际进程27928、
LocalService、原库existing、用途材料及原进度一致。A→C认证Status31ms通过，
原状态根与A/B一致；C旧875的c0000005未归因，不登记修复。没有发送对象。
D现在执行已批准的a254 existing更新，C保持运行，两机暂不开观察器。
具体命令、固定包/工具哈希及回传范围见
[a254 C/D CURRENT](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)，
[A实际查询](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/A-query-C-20261112/health-public.json)。
手机工作不阻塞该顺序；不重跑已通过直连/单中继，不触发新的Full。

## 2026-11-12 C增量：WSS完整ML-DSA身份与v4短期端点，通信lib剩7项

NOVOCHAT `3065555`已推送：AuthorizedWssConfig/原端点准备消费上游TransportIdentity，
不再现场生成Ed传输钥。完整1952字节公钥纳入原联系人签名/AEAD正文，WSS短期声明
独立NCEPLS04/v4域，旧v3不能继承新语义；外层4096字节、30秒、权限和持久高水位不变。
原聊天身份/历史不重写，这不等于全通信协议已抗量子或真实UCA绑定完成。

core元数据定向1项、隔离消费者4项通过（重复包含该元数据项，不相加计5），严格Clippy
通过；原真实ML-DSA公钥经相同数据类型完整保留。完整通信lib错误8→7/退出101。
WSS网络/签名编解码整链及旧测试调用者尚未迁接执行，不能将局部检查记为通过。
剩余两处peer-id、四处bound-session及StoreReceipt verifier，C继续消费，不用假钥/恒真
状态消除错误。A仍需真实UCA/设备授权、旧联系人连续性和可信邮箱服务来源。
本轮无手机/节点操作、无新APK、收费关闭，先前App UID持钥通过仍保持其原范围。
[详细修改与原始结果](https://github.com/XujueKing/NOVOCHAT/blob/3065555/docs/implementation/WSS_NATIVE_IDENTITY_2026-11-12.md)。

## 2026-11-12 C回传：双机App UID通信持钥及进程恢复通过

NOVOCHAT `b84b9f6`：同7f283b81固定库、dea182e4默认生命周期修复，JNI hash pin和原
独立测试包构建入口已更新。真实App UID/Keystore/JNI下，PKL110、PCLM50各完成缺槽
恢复拒绝、明确建钥、独立测试包force-stop后新PID恢复同公钥、错绑定/畸形密钥拒绝，
拒绝后原钥仍可恢复。不是仅adb shell临时密钥；也不是手机重启或资产签名验收。

首次两轮在创建密钥前分别发现系统noBackup父目录0771误拒、/data/user/0与/data/data
别名误拒；C沿原identity owner的规范路径/父目录处理修复，自建密钥目录仍0700、文件
0600。原失败和最终双机结果保留，无删钥/清数据。14项Kotlin定向回归通过。
独立APK SHA256 0ed29254eaa50dfb0aef777eb3d02f2a4f1cc365c022d8ea4dd60fa4c3092a09。

主聊天包/身份历史和节点未动，收费关闭；完整通信lib原8项错误、真实UCA/设备授权及
联系人连续性仍待接通。C继续消费端迁接，A继续可信状态与四机主线，勿将本机建钥当
UCA注册。详见[结果与原始证据](https://github.com/XujueKing/NOVOCHAT/blob/b84b9f6/docs/implementation/APP_COMMUNICATION_CUSTODY_2026-11-12.md)。
下方尚未安装/未做App UID持钥的旧当前状态由本节覆盖，历史原证据不改写。

## 2026-11-12 16:14 A：a254四机部署获批，B/A actual existing已通过，C下一台

用户已明确批准a25452f4四机existing更新及双中继实测；原Full43155828234成功，
不再重复申请停服。B已由8fed更新a254，PID29088/novovm，原库及密钥不变，
41050/42050正常，认证Status64ms。A系统14:55重启后旧875已无进程/端口；
原停止态检查通过后以a254 existing恢复，PID5136/LocalService，Status32ms。
两机原状态根均f6186d40...5f56333，实际AOEM及程序哈希检查通过；没有发新对象。
A缺少旧exit摘要，不能把任务result0宣称为已证明正常排空。C旧崩溃风险未结案。

现在C执行一次同版本原更新器，D只核验新包，等A核对C健康后再更新D。
工具ZIP仅19935字节已随Git交付，程序ZIP需从A复制，明确命令/哈希/恢复分支见
[a254 C/D CURRENT](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/CD-UPDATE.md)。
[B/A原始公开回执](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/README.md)
与候选Full分别记录；C/D未更新、双中继未验收，不拿Status替代DONE。
刚下拉的C Android共享修复不替换固定a254程序，A不等待/重复开发手机任务。

## 2026-11-12 C → A：共享装载修复及双机默认回归已完成

用户明确授权C直接修复AOEM/SUPERVM，本项不再等待A。共享修复`dea182e4`在原bindings
保留Android DSO代码映射到进程退出，防止线程TLS析构晚于dlclose；密钥/上下文释放不变，
Android环境变量不能再开启不安全卸载，Windows/Linux原语义保留。AOEM固定SO/hash未变。
NOVOCHAT `b1d199e`已消费新受审输入；原139和一次不卸载诊断记录完整保留。

两手机默认配置各5项真实communication_custody通过/退出0，命令明确unset卸载环境，
不靠诊断开关通过。两机原pq_preflight及错hash/缺库/错参数拒绝通过。共享bindings
34通过/8原有忽略、策略定向测试和严格Clippy通过，消费者严格Clippy通过。
B首次预检发起遇到ADB daemon不可用未执行签验，恢复连接后只续跑B未完成步骤，
原中断保留，A成功测试不重复。不是App UID/Keystore跨进程验收，也未安装APK或改JNI库pin。

C继续手机App接入，A继续双中继及真实UCA/设备/联系人可信输入，避免重复修本项。
[手机原始结果及失败对照](https://github.com/XujueKing/NOVOCHAT/blob/b1d199e/docs/implementation/ANDROID_FIXED_RUNTIME_2026-11-12.md)。
原聊天身份历史和节点未动，收费关闭；下方等待A修卸载的当前状态由本节覆盖。

## 2026-11-12 C → A：Android构建完成，默认线程退出139，需共享生命周期修复

NOVOCHAT `86ae1ff`：按723b4a45固定输入构建成功，新Android SO SHA256
7f283b8152eb30b8873e75be4b764905d37db5c1afaec88238b30146f264bfca。
mobile-A原预检/负例通过，原持钥测试首线程退出SIGSEGV/139；仅不卸载库的独立诊断
对照5项通过，不计默认资格。A继续原AoemDyn/统一owner卸载与TLS析构调查修复，C未改
共享实现或重做密码库。B未测试，JNI pin/手机原包身份历史未动，无APK/Keystore验收。
详见[Android CURRENT与公开证据](NOVOVM_MOBILE_AOEM_RUNTIME_HANDOFF.md)。

## 2026-11-12 A → C：Android 构建执行权交回 C；A 回双中继

按用户最新决定，C 复用已有 AOEM/NDK/手机执行 Android 运行库构建与 JNI、验签、
Keystore 实测；A 提供固定来源与脚本，继续承担共享接口缺陷。A 不继续并行开发
NOVOCHAT；本轮未改手机源码或安装 App。C 不必等待 A 的未完成 Android 编译。
固定输入小包位置、SHA256、执行命令和回传内容统一见
[Android 交接 CURRENT](NOVOVM_MOBILE_AOEM_RUNTIME_HANDOFF.md)。私有输入包需要取得
或确认 C 已有同哈希输入；Git 推送仅是交接发布，不冒称 C 已收包、执行或签收。

双中继候选 `a25452f4c49be37cd984e650f2079642e6c273b3` 的原 Full
[43155828234](https://github.com/novovm/supervm/actions/runs/43155828234)
已只读核对为 `completed/success`、`workflow_dispatch`、attempt=1；未触发重跑。
本条和 Android 构建脚本不产生新的四机程序资格，不表示已经部署128KiB候选或
完成双中继实网 DONE。下一步由 A 确认该固定版本部署范围并继续四机主线。

## 2026-11-12 C接收5de4083d：原identity worker本地入口已接，自动启动未启用

C已fast-forward并读取A对fb394aa8/accdcc1b/89064cf1的正式回执，不再称这些轮次未签收。
A认领Android制品资格及UCA/设备/联系人连续性来源，本次回执未提供新运行库或状态。

NOVOCHAT `bf60aea` 已推送：原LocalIdentityOwner单worker新增内部prepareCommunication，
连接原JNI backend与Store的明确enroll/restore，前后检查原本地身份active；承诺及公钥
保持绑定，错误恢复不转建钥。revoke、failClosed和原关闭路径释放非活动持钥对象。
未增加Flutter方法、线程池或自动调用，也未制造UCA可信承诺来源；本地active不等于
已注册/已授权网络身份。Kotlin主源码和14项定向单测通过，生命周期6项属fixture，
不能代替手机Keystore/真实JNI执行。原8项通信编译错误未重验为通过。

下一步仍由A交付合格Android库和可信绑定，C继续会话/回执及原worker消费；不重做JNI，
不以假身份消除接口错误。原手机身份历史、回退包和节点未动，收费关闭。
[原始结果与入口边界](https://github.com/XujueKing/NOVOCHAT/blob/bf60aea/docs/implementation/ANDROID_COMMUNICATION_JNI_2026-11-12.md)。

## 2026-11-12 A接收确认：fb394aa8及两项前序增量，认领共享依赖

`received_commit=fb394aa8440f3121d4a96e9b12e9609f28a0c420`；一并接收
accdcc1b、89064cf1，对应NOVOCHAT的9e6d55f、fff768c、bfaac9f。
A已从远端读取本条、SDK消费／发现迁接／Android通信JNI交接；本机SUPERVM以
fast-forward合入fb394aa8，无覆盖并发提交。这是交接收件与责任确认，不是A重新
执行C全部测试或签收Android运行、完整App、身份迁移、后台恢复及收费。
本节覆盖下方这些交接的“尚无A新签收”；原文与各轮失败保持不改写。

当前按C证据记录：JNI backend已实现，启动worker尚未启用；完整通信lib仍8项
错误，不再沿用旧11项当前结论。Windows真实持钥5项、Kotlin8项、隔离严格Clippy
与ARM64 JNI库构建结果属于C本次范围，不等于手机实际AOEM或Keystore生命周期通过。

A下一项优先交付Android运行库资格，不让C重复签名器或另造网络：

1. **Android运行库，由A主责。** 核对原AOEM构建来源、Android/Bionic ARM64目标、
   manifest/hash及原MldsaRuntime、OwnedMldsaVerifier正负向资格，形成可给C加载的
   固定输入。当前本仓aoem-sdk-manifest列Windows/Linux core，不能拿其中任一库当
   Android制品；本轮没有发现可据此签收的新Android交付。旧5b08缺能力记录仍有效。
   AOEM源码工作区有其他未提交开发，不能直接混入未经固定的手机候选，也不改其工作。
2. **可信绑定与实际状态，由A主责。** 原product_identity的
   sign_network_device_authorization_mldsa65_v2、authorize_authenticated_peer_v1及
   NetworkDeviceAuthorizationStatusV1可以复用；后者要求实际授权状态，不能用恒真
   状态、网络持钥或四机Status代替UCA注册／恢复／设备授权及旧联系人信任连续性。
   服务邀请、信箱声明/回执版本和实际部署证据须分别交付；当前不宣称这些输入已齐。
3. **手机消费者，由C主责。** 继续原worker、版本化持久记录、会话/回执及剩余8项
   调用迁接；不必重做已交付JNI。合格运行库未到之前可做不激活网络/身份的本地接线，
   不填假公钥/默认授权，不改旧身份历史，不以临时建钥作为UCA注册完成。

共享复用入口仍是novovm-exec/src/pq_signature.rs中的MldsaRuntime与
verification_owner_v1、novovm-relay/src/transport_identity.rs中的from_mldsa，
以及novovm-network/src/product_identity.rs。这些源码入口不是Android制品资格。
本次只提交接收与缺口归属，不改生产代码、不更新手机/节点、不重跑或新发Full；
双中继固定程序a25452f4及其独立验收范围不变。

## 2026-11-12 C → A：Android通信持钥JNI backend已交付，启动和手机资格未完成

NOVOCHAT `9e6d55f` 已交付原 NativeCommunicationKeyBackend 的 JNI 实现，复用
MldsaRuntime/from_mldsa和原Keystore封装；五个入口仅open/generate/verify/retain/close。
临时私钥只给原Kotlin封装worker，非活动槽有界、关闭ID不复用，无Dart签名或资产接口。
Windows真实ML-DSA持钥5项、Kotlin封装8项及主源码编译通过，严格Clippy和Android
ARM64实际库构建通过；五项导出符号已核对。不是JNI手机执行或Keystore生命周期验收。

Android仍固定原5b08制品及能力门禁，旧库缺能力的失败未解决，不能靠改pin/删校验开通。
原启动worker尚未接入，完整通信库仍8项错误；新backend不授予UCA或联系人权限。
A请交付/指明合格Android运行库manifest/hash及聊天身份/设备授权的可信绑定来源；
C继续原worker、会话/回执消费，不重复共享实现。原身份历史、回退包和节点未动，未安装
APK，自动恢复与收费未开启。本条为Git回传，尚无A对本轮的新签收。
[详细实现及原始结果](https://github.com/XujueKing/NOVOCHAT/blob/9e6d55f/docs/implementation/ANDROID_COMMUNICATION_JNI_2026-11-12.md)。

## 2026-11-12 C → A：发现服务消费迁接，完整通信lib 11→8，未装手机

NOVOCHAT 已推送 `fff768c`，仍复用受审共享输入 a25452f4，不修改网络/账户实现。
DiscoveryNode 已分开完整 ML-DSA 公钥与本地32字节 slot 承诺；原 RPC/租约验签用
完整公钥，slot 只用于本地发现/缓存/HKDF。旧联系人签名与持久记录保留，不截断公钥
或把旧 Ed 身份转 Vec 冒充新身份。发现握手要求原 owner 授权的独立 TransportIdentity。

- Windows core 无网络功能库249通过、1原有忽略；中继目录测试使用真实AOEM ML-DSA。
- 隔离 locator 消费3项通过（1项真实签验、2项格式），拒绝将slot承诺当公钥；严格Clippy通过。
- 完整通信lib仍8项错误、退出101；Android装配和网络测试调用者尚待迁接，不能把8项
  当全App库存。原首次17项测试夹具/feature错误及中间10项编译失败保留。
- 原媒体恢复测试补上其实际依赖的mailbox feature条件，未删/ignore；本轮未执行它，
  网络迁接后仍须回归。没有Android编译/运行资格、APK安装或自动恢复修复结论。

C下一步继续bound-session/WSS、版本化信箱回执、Android可信服务装配与持钥worker；
A仍负责真实UCA/设备授权、旧聊天身份连续性的可信来源、服务/邮箱绑定及合格Android
AOEM manifest/hash。当前Android调用者不能填假公钥或默认授权来消除错误。
手机原身份历史、v108/v110和节点均不动，正式资产及收费关闭。本条为Git回传，未获A新签收。
详见[源码说明与逐次原始结果](https://github.com/XujueKing/NOVOCHAT/blob/fff768c/docs/implementation/SDK_DISCOVERY_MIGRATION_2026-11-12.md)。

## 2026-11-12 A：a25452f4固定候选本地全部通过，一次Full已提交

两平台原打包器、完整性及归档逐成员核验完成。每平台：真实包内身份7/7、
原网络owner22/22、真实包内节点原库恢复6/6、UCA恢复1/1、启动资格1/1；
单／双中继本地命令各3/3通过。未向实机发对象，不将组件／本地进程结果当四机验收。
Windows ZIP150573543字节，Linux tar.gz359305407字节，固定程序均为a25452f4。
完成上述全部本地门禁后才请求一次[Full43155828234](https://github.com/novovm/supervm/actions/runs/43155828234)，
实际headSha=a25452f4c49be37cd984e650f2079642e6c273b3，workflow_dispatch，attempt1，
04:35:59UTC查询仍in_progress，不能提前签收。先前push绿灯只是轻量同步。
最新授权仅128KiB候选验证，A/B/C/D未更新；不重复直连和已通过单中继。
两平台哈希、包内日志、原失败、同版本existing更新边界及回滚限制见
[固定候选记录](../artifacts/audit/novonet-single-relay-20261111/candidate-a25452f4/README.md)。

## 2026-11-12 C → A：NOVOCHAT消费a25452f4，手机尚未升级

NOVOCHAT已推送`bfaac9f`。C已读取A对2cc97255的接收确认，不再沿用“未签收”结论。
本轮消费范围审阅63e9418e→a25452f4：Android rustix升至1.1.6；原NVLOCR01/02
保留签名额度；hop预算拒绝诊断。未修改共享实现、节点配置、生产额度或费用。

- 实际Windows AOEM SHA256为
  `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`。
- 隔离消费者24项通过（0忽略），含真实ML-DSA的新旧locator读取、额度/链域/公钥篡改
  拒绝，以及inactive通信密钥恢复/替换拒绝；all-features/all-targets严格Clippy通过。
- Android ARM64原生库build通过，保留relay Android分支2项unused-variable警告。
  这不是Android AOEM加载验收；新Rust持钥槽尚未接JNI/启动worker，不授予资产权限。
- 原主聊天库在新输入下仍11项编译错误，未生成或安装APK，未修复/签收后台恢复。
  手机原身份、历史和v108/v110回退不动。首次测试feature遗漏及缓存缺包失败均保留。

A仍需提供/指明合格Android ARM64 AOEM manifest/hash及统一验签资格、ML-DSA服务
邀请/邮箱可信绑定、真实UCA注册恢复/设备授权及聊天身份连续性的消费来源。C继续
TransportIdentity、原持久化版本读取、Android桥接和UI，不重复实现网络/账户协议。
源码同步不代表A已签收本次交接；不更新C/D节点，不重发已完成单中继对象，不开启收费。
详见[消费者记录及原始证据](https://github.com/XujueKing/NOVOCHAT/blob/bfaac9f/docs/implementation/SDK_CONSUMER_UPGRADE_2026-11-12.md)。

## 2026-11-12 A：发现紧凑声明补齐128KiB，原包失败保留

a2d7af7f 两平台包内身份7项通过，但完整网络owner组均19通过／3失败，不能签收候选。
根因是原节点已签入128KiB，发现紧凑编码仍按64KiB重建，原字段一致性检查正确拒绝。
现沿原NODE_PUT/NODE_GET和locator owner增加明确NVLOCR02配置档，完整保留原
NVLOCR01的64KiB字节语义；实际共享预算与新声明引用同一128KiB常量。验签、序号、
期限、四会话及其他应用默认额度不变，切换配置档不能伪造原签名。旧程序拒绝V2，
后续须核对同版本接收者；不把此改动说成“所有发现编码均未变”。
Windows/Linux紧凑声明各2/2、原网络owner各22/22、network/node严格Clippy通过；
CI范围27/27通过。Full原步骤补入三个原失败场景及测试名存在校验，无新增触发。
当前仅修正源码本地通过，须重新固定制品；未派发Full、未更新A/B/C/D、未取得双中继
实网DONE，也未解释C旧崩溃。详见[原失败与修正对照](../artifacts/audit/novonet-single-relay-20261111/locator-profile-20261112/README.md)。

## 2026-11-12 A：双中继候选本地安全门禁发现并修正 rustix

1baa6dbc 的两平台原打包器构建成功，但本地 Cargo Deny 因
RUSTSEC-2026-0486 拒绝 rustix 1.1.4，因此没有为该版本派发 Full 或更新节点。
最小修正只将 network 的 Android 文件锁依赖最低版本提升到1.1.6，并由 Cargo
更新根锁文件对应条目；runtime 不含该依赖，锁文件不变。无网络协议或预算再调整。
Windows22项、Linux20项文件恢复／共享中继预算／存储配额测试通过；两平台
network/storage-budget/node/ctl 严格 Clippy 通过。根与runtime Audit/Deny四项通过，
原有警告及既有忽略规则原样保留，没有新增豁免。原拒绝和一次安全库抓取失败均保留。
这不是C的875崩溃归因，也不是Android实机、固定新包、Full或双中继公网验收。
继续用修正后的固定源码制作候选；A/B/C/D不更新、不重发已完成单中继对象。
详见[安全增量与原始记录](../artifacts/audit/novonet-single-relay-20261111/rustix-security-20261112/README.md)。

## 2026-11-12 A：双中继原 owner 与128KiB共享额度候选，本地验证通过

A→C→D 单中继实网已在 a7e19fc8 按三端原始证据闭合；不重发或重开旧窗口。
下一项复用原 HopRoute/HopStream 的三段路径，补齐原诊断命令、发送目标绑定和
入站已知身份校验，使 `prepare --via <第一中继> --via <第二中继>` 可指定两个
有序中继。单中继命令原字节不变；新双中继命令441字节、独立6/7/8标签，旧程序
明确拒绝。正常业务仍原生 NovoRUDP 优先，不改网络帧、有效期、账本或手机。
初始本地双中继回归在旧入站“仅一个中继”校验处拒绝，原失败已保留并补齐该入口。
后继双平台实测在认证／时间测量后耗尽64KiB共享中继额度；原计数证明无未完成
预留占用。用户单独批准128KiB/分钟候选验证；原共享预算与节点签名locator声明
同步该值，其他应用默认额度、连接数、期限及预算拒绝规则不变，不以此批准部署。
Windows/Linux各23项定向及兼容回归通过，无忽略；两平台network/node/ctl严格Clippy
通过。原Full步骤补入对应测试及测试名存在检查，本地CI调度27/27通过；未另发Full。
旧64KiB失败、初始入站拒绝和中间脚本失败保留，最终结果按独立完成记录统计。
当前仍是本地源码验证，尚无新固定包／Full／四机双中继结果；875接收者不能直接
参与新双中继诊断，须先完成候选资格再更新。C继续独立定位原875内存故障。
详见[同一工作包当前记录](../artifacts/audit/novonet-single-relay-20261111/README.md)。

## 2026-11-11 C：同一v2观察器原窗口已结束，active=false

PID6592在12:50:36.5703516UTC（北京时间20:50:36）自然结束；退出0，result存在、
wait_expired=true、snapshots=0、terminal_observed=false；error及launcher-error不存在。
最新原字节liveness为active=false，不能再用历史READY发送。B恢复未续期C窗口。
[终止状态、现有原始输出及摘要](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-relay-observer-v2-20261111T122035Z/README.md)
已回传：含最终result、liveness、退出/stdout/stderr及read-anomaly首末记录，没有relay快照。
C未重开观察器、重启／换包／发对象或重试，D不操作。A本轮拨号前失败，零快照不算中继通过。

## 2026-11-11 C：v2观察器运行中，使用最新liveness而非旧READY

已按c92d2172启动一次v2：helper PID6592，原875节点PID31296，READY为
12:20:35UTC／北京时间20:20:35，最长窗口约至20:50:35。原Watch及相邻Reader都已
核验使用。旧v1 PID27480在20:05退出1、零快照，原件保留且未被再次终止。
当前原字节[READY／最新liveness及摘要](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-relay-observer-v2-20261111T122035Z/README.md)
已回传；每次补交liveness保留前一快照，不重新启动观察器。A应读取最新版本及终止有无，
仍须D独立新鲜liveness，C不代签D。节点／包／密钥不动，不发对象；退出后立即另行回报。
下节v1窗口是历史，不可据其旧READY发送。

## 2026-11-11 C_RELAY_OBSERVER_ARMED：实际ready已回传，窗口运行中

按4f44244e启动一次原Watch-Relay.ps1：实际helper PID27480，原875节点PID31296，
ready时间12:00:16.9208005UTC（北京时间20:00:16），最长等待1800秒，约至20:30:16。
观察器是独立隐藏管理员PS7进程，保留stdout/stderr及最终退出；不等待采集结束才回READY。
[原字节ready.json及SHA256](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-relay-observer-20261111T120016Z/README.md)
已经保存；未重启／换包／换钥／发对象，C不操作D。等待A收到D独立READY后单次发送。
不重复启动观察器，后续从同一本机目录回传结果；本节不是中继交付验收。

## 2026-11-11 C：v2一次RecoverCrashed成功，C_875_EXISTING_READY_FOR_A_QUERY

已按9bba5135使用新工具目录／证据目录执行一次v2，原更新器退出0，stderr空。
875新PID31296、实际LocalService，以START=existing打开C原chain-state；原custody及
进度保持，实际AOEM哈希不变，监听192.168.71.125:41052／42052。执行失败0、恢复false。
任务／事件／exit三方c0000374经v2原校验通过，没有先启动旧8fed、初始化、换钥或重传ZIP。
详见[C成功回传和原始公开结果](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-UPDATE-V2.md)。
请A进行外部认证Status及原状态根核对，再决定D；C不代签外部健康或放行D。
D未动、采集未开、未发对象。旧8fed崩溃根因仍开放，真实中继未签收。
下节v1分支不支持／失败记录保留，已由本轮v2实际existing成功覆盖部署状态。

## 2026-11-11 C：cc9f核对取得旧8fed c0000374，原分支不合格即停止

系统管理员确认完成，C一次快照为活动节点0、目标UDP端口0；旧8fed任务Ready/
LocalService，退出3221226356。原exit.json的-1073740940是同一0xC0000374位模式，
Application1000事件546046/PID7816/05:40:46UTC亦记录旧8fed的c0000374。
进度公开字段完整且原guard可读；三项原恢复分支分别要求0、267014或c0000005，均拒绝。
因此C_875_EXISTING_RECOVERY_NOT_ELIGIBLE，不能签READY或放行D。没有调用更新器、
启动旧包／875、改原库密钥或开始采集。记录器负数UInt32转换错误与原快照原文保留；
离线解释位模式并核验原函数，没有重采owner或更改guard。完整任务／事件／原始公开结果
见[C本轮完成回传](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-RECOVERY-cc9f.md)。
下节UAC取消状态已被本轮完成核对覆盖；旧失败仍保留，当前缺的是该实际退出类型的受审恢复处理。

## 2026-11-11 C：cc9f恢复核对尚未执行，Windows UAC取消

C已同步cc9f874b，核对本机固定ZIP和原工具哈希，准备新的单次入口，按原函数判断
stopped/interrupted/crashed，只有一个分支完全合格才调用一次原更新器。没有修改原工具。
但2026-11-11T11:14:56Z Windows RunAs返回“操作已被用户取消”，控制入口未启动，
inspection/recovery均0次。因此没有当前进程／任务／退出／端口／进度的完整新观察，
不能签C_875_EXISTING_READY_FOR_A_QUERY，也不能据此诊断875或选择恢复开关。
原库／身份未动，未启动旧包、875或采集，D不操作。已请求用户准备好后完成新的系统确认，
不是重新申请停服或重复更新。上轮一次预检失败保持；本机ZIP无需E盘或重传。
[公开状态](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-cc9f-UAC-public.json)
及其引用的原始启动错误ZIP保留本轮事实，尚无节点恢复结果。

## 2026-11-11 C：875一次更新在原进程预检失败，未启动新节点

按main@5a145197、CD-UPDATE.md执行一次原更新器，管理员PS7，无恢复开关。
原工具退出1：`Original process identity not confirmed`；old_node_stopped=false、
existing_start_attempted=false，未生成before/after/result。未初始化、换钥、采集或重发，
失败保留且不自动重跑；不能返回C_875_EXISTING_READY_FOR_A_QUERY或据此放行D。
原始公开退出／错误／stdout/stderr见[C本轮回传](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-UPDATE.md)。
原ZIP现已完整留存在C本机D盘target固定目录，大小／SHA与A一致，后续不再依赖E盘。

## 2026-11-11 C：875d3307包已核验，未更新／未采集

C原接收目录已有用户指定ZIP，大小150,585,096与SHA256 e8e4e2a1…一致，
包内80文件及77项manifest／checksum只读核验通过，退出0。
没有运行包内程序、更新／启停节点、启动采集或重跑资格，数据库／身份不动。
完整摘要和原始公开结果见[C包接收回执](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/C-PACKAGE-READY.md)。
用户负责手动复制给D并由D Codex独立核验，D尚待回执；不能据此记C/D均已就绪。
本节仅通过Git通知A，维持用户“仅核验、不更新或采集”的本轮边界。

## 2026-11-11 A：875d3307两平台固定包及本地资格通过，Full与实机未签收

最新：两平台实际包各通过身份/NAT7、原owner20、原生恢复6、NUAS恢复1、启动1；
原包内arm/send/cancel三命令通过。归档成员及AOEM原清单已核验，原更新器离线
Windows12组/Linux13项通过。固定程序875d3307，文档后继10201a18不改变程序源码。
详见[固定候选及原始记录](../artifacts/audit/novonet-single-relay-20261111/candidate-875d3307/README.md)。
C/D当前只复制/hash新Windows包，不停节点或提前采集；Full成功后按A→C→D串行
existing更新再取三方中继证据，不重复直连。B本次不在此中继路径，不操作。
实际诊断承载为原HopStream/Iroh中继及NativeObjectLink，不冒充native_udp直连，
也不改变正常业务NovoRUDP优先。下方“包内待验”为保留的较早状态，以上为当前。

两平台本地原owner19项、原探针5项、中继边界3项及CLI1项曾通过，严格Clippy通过。
但随后Linux包内AOEM的原owner验收18/19，新增诊断在准备阶段报
`diagnostic time reply budget`，未发探针。因此63e9418e不部署，不能用早先通过覆盖。
定位中发现接收间隔直接判错的确定性边界：合法250ms发送经过抖动可能249ms到达。
原owner内一个有界待回复槽保持原250ms回复频率，期限不刷新、三样本不减少、
资源不增加；两平台原owner20/20与严格node Clippy通过，包含确定性反例和原三节点
交付。旧组合错误的每个谓词仍不能由日志独立分辨，不扩大为所有历史失败已归因。
当前四机均未因本轮换包，不发送新实网对象，C/D暂不更新或启动采集。
原Full42961080141对应程序63e9的纯文档后继88032d91，不取消，也不替新修正背书。
其中Cargo Audit失败已读原日志：openssl0.10.78命中三项公告（0474/0476/0477）。
根锁文件仅升级openssl0.10.80，不改ignore；本地根/runtime Audit和Deny已通过，
Linux依赖消费者回归已完成：原owner20、probe5、hop3、CLI1、rollout-policy1通过，
node/ctl/network严格Clippy通过；Windows原owner20和三个Clippy目标也通过。
875d3307两平台固定构建完成，包内资格尚未完成。不是中继或AOEM故障，不关闭安全门禁。
原Full另有NUAS恢复回执查询5秒超时，尚未归因；保留原失败，不放宽期限。
下一候选先完成两平台包内资格再触发Full；实测目标仍为A→C→D及三方同路径证据。
详见[原单中继记录](../artifacts/audit/novonet-single-relay-20261111/README.md)。

C的2cc97255交接已读取并原文合入；这是收件确认，不是Android运行库、UCA注册或
手机生产验收。当前先完成已授权单中继工作包，不把C的接入任务混入本轮节点变更。

## 2026-11-11 C → A：Android持钥增量与真实UCA流程

NOVOCHAT已推送`f804df51ef7cd4efcc250667e8cb9551c270e227`，受审SDK输入63e9418e。
本轮两次fetch时共享main仍为88032d91，未见A新增接收回执；本节是C新通知，
不表示A已签收。C继续手机消费者，A拥有共享协议、运行库和节点部署。

用户最新明确：原App是功能核心原型，现在要真正接入既有NOVOVM＋NOVONET，
UCA注册、恢复、设备授权和网络认证均须贯通；普通用户尽量合并为创建／恢复身份后
进入聊天。不能让用户手工选节点、复制公钥或反复连接，也不能把本地建钥、组件验签
或四机Status等同UCA注册完成。身份登录与钱包NOV账户、支出权限分开，收费关闭。
原手机身份、历史、附件和v108/v110保留，不把原型身份静默升级成已确认UCA。

C已完成本轮可独立推进的消费者工作：

- 通信专用Keystore AES-GCM封装及noBackup存储接缝，复用原预算、原子写；
  独立于资产alias，缺材料恢复失败，不重建替代钥。Kotlin12项通过。尚无生产native
  backend或启动接线，未装手机，不能当通信持钥完成。
- 隔离JNI工程同步当前ML-DSA-65授权验签长度及显式TransportVerifier，复用原
  authorize_authenticated_peer_v1；Android ARM64库实际构建通过。完整验证入口
  19项测试零忽略、格式和严格Clippy通过，不用默认feature省略运行库用例来报成功。
- 其中5项用实际AOEM及原from_mldsa，验证授权、撤销、攻击者自签根、错误设备、
  TCP加密帧和重连旧会话拒绝；仅单机组件，不是四机或手机NativeEndpoint验收。
- 本机Windows库SHA256为`1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50`，
  与原8fed交接一致，V3 preflight通过，缺状态仍拒绝；asset_authorized=false、
  transaction_submitted=false。覆盖旧Windows库能力阻断，不覆盖Android5b08失败。
- 主聊天库再检仍退出101、11项接口错误。未生成／安装新App，未签后台自动恢复。

请A接收并确认以下既有接口／部署输入，C接入而非重复实现：

1. 当前合格Android ARM64 AOEM及manifest／hash，实际通过MldsaRuntime和原统一
   语义OwnedMldsaVerifier。C不取消能力门槛、不以Windows结果替代手机资格。
2. ML-DSA信箱服务邀请／locator、声明／回执版本及实际部署状态；四机主节点认证
   不证明App的L3信箱协议已经迁接。
3. 真实UCA创建／恢复、设备授权及旧聊天身份到新通信凭据的原消费合同与可信状态
   来源。已核对product_identity仅提供网络能力授权，不代替聊天身份连续性；资产
   账户创建也不等于身份注册。未交付入口请逐项标明，C不另造注册协议。

源码、原始证据、剩余11项分类与简化流程约束见[C增量记录](https://github.com/XujueKing/NOVOCHAT/blob/f804df51ef7cd4efcc250667e8cb9551c270e227/docs/implementation/ANDROID_COMMUNICATION_CUSTODY_2026-11-11.md)。
不要求重跑四机认证或升级节点；本轮未操作运行中节点、手机、正式资产或收费。

## 2026-11-11 C → A：用户选择直接迁接新SDK，暂缓旧协议诊断APK

NOVOCHAT已推送`3271355`、`b5a3865`。SQLite消费已统一rusqlite0.41.0/
libsqlite3-sys0.39.1；仅对原tor-dirmgr0.47的兼容上界作本地清单补丁。
共享存储原12项测试、Tor原14项存储测试及37项手机诊断工具测试通过。
用户拒绝旧SDK诊断路线，已取消隔离编译并删除该构建入口，未安装任何新APK。

C已核对并消费到63e9418e输入：退役旧signed NAT CLI/适配器，App继续复用原
NativeEndpoint；中继目录接显式TransportVerifier/TransportIdentity，不恢复Ed回退。
Windows库编译错误由34项降到11项，尚未通过；Android完整编译、双机安装和自动恢复
修复均未完成。两机仍保留v110、原身份/历史，收费关闭，不作手机ML-DSA迁移声明。

请A确认下列共享输入，C继续承担消费端，不另造签名器/账号/网络：

1. Android ARM64 AOEM的当前合格制品、manifest/hash及原MldsaRuntime +
   OwnedMldsaVerifier语义批量资格。原5b08库此前缺少当前消费者所需能力，不能自动
   将其等同当前transport provider合格。C拟复用现有共享
   `novovm_relay::transport_identity::from_mldsa`及独立通信用途材料，资产签名权不外借。
2. 现用L3信箱服务是否已经部署ML-DSA服务声明、Finished及StoreReceipt版本；
   请交付可独立验证的新服务邀请/locator及明确协议版本。主链8fed Status通过
   本身不证明手机信箱服务已升级，不能拿旧32字节pin冒充1952字节ML-DSA pin。
3. 原本地聊天身份与新设备通信公钥的合法连续性应消费哪项既有授权/迁移证明。
   旧聊天历史读取不需要换钥；新传输身份不能仅凭目录返回公钥继承联系人信任，
   也不把聊天登录等同资产账户权限。请确认原共享合同/所有者，避免C另建身份授权协议。

实际日志及后续接线清单在NOVOCHAT
`docs/implementation/SDK_DIAGNOSTIC_BUILD_2026-11-11.md`和其evidence目录。
上述是新SDK接入缺口，不是要求重编译主节点或重复四机验收；本机未操作运行中验证节点。

## 2026-11-11 A：诊断专用单中继约束（实现验证中，尚未部署）

用户批准在原 owner 增加有界诊断路径约束，不改变正常 NovoRUDP 优先策略。
原 Driver 在原 Iroh scope/HopStream/NativeEndpoint/NativeObjectLink 内驱动
独立诊断会话，固定一个中继和一个接收者；不把它安装为业务 peer route。
本地受保护命令增加 arm/send-relay/cancel，仍只允许原固定非业务探针。
接收端复用原三次认证时间测量，保留原有效期、认证、共享连接与字节预算。
发送端 DONE、接收端完整性结果、中继实际转发及相同 route/session 分开核对。

Windows/Linux 的新增真实三节点组件用例已通过；Windows 三项边界测试通过，
node/ctl/network 严格 Clippy 通过。最终受影响回归、固定候选与实机证据仍在收口。
初始夹具未等到 locator、初始双向时间测量耗尽共享预算的失败分别保留；
没有增加预算、延长业务期限、再次阻断端口或重跑 A→D 已通过直连。
详情：[本轮原记录](../artifacts/audit/novonet-single-relay-20261111/README.md)。
当前各机仍为8fed，不据此要求 C/D 提前启动更新。未改 NOVOCHAT，手机消费无需
因这项节点诊断入口改签名或改业务网络；旧手机验收与本轮主节点验收保持独立。

## 2026-11-11 C → A：手机待机故障及消费者依赖阻断

NOVOCHAT 已推送 `2c717c7`：保留 v110 两机再次停滞证据，A 原进程有
authority_endpoint/255、E8；保存现场后重开 A 恢复路径就绪，原身份/存储保护摘要一致。
候选仅补原端点许可的首次失败类别，不新增重试权或修改共享协议，尚未打包安装。
本地2项原因保留测试与37项诊断工具回归通过；整仓集成测试未执行成功。

当前消费者 rusqlite=0.40.2 需要 libsqlite3-sys0.38.x，而共享
novovm-storage-budget 固定0.39.1，Cargo报告sqlite3 links冲突。
App build.rs仍固定SDK审阅基线06801146；C没有跳过审阅门槛或回改共享库。
后续由C在App侧对齐依赖并审阅新SDK消费兼容，A继续拥有共享网络/协议和运行库实现。
不要把当前主节点8fed部署等同手机主聊天包已升级。实际手机仍为v110，
自动恢复缺陷仍开放，本轮没有新双机发送或收费验收。
详情见NOVOCHAT `docs/implementation/V110_STANDBY_FAILURE_2026-11-11.md`。

## 2026-11-07 validator-C → A：NOVONET 通信能力复用交接

本节只交接公开源码、既有验收和制品摘要；不重跑手机/四机测试，不修改共享网络，
不读取或传递私钥、受保护身份、会话状态、联系人、数据库或私有连接配置。
**C负责手机端消费与验收；A负责共享网络审阅、NOV主节点统一选路接线及验证。**
不得把本节的调用链审查计为新部署、测试通过或所有通信消费者已经迁到ML-DSA。

### 1. 输入版本、未推送修改和实际制品

2026-11-07重新fetch后，SUPERVM main为`54b59f6edd69ee9531b660c342ac7e61184f506e`，
NOVOCHAT main为`45e3c917225ad2d4bded1a7560037b58a2ebd701`。两仓开始审查时工作树均干净、
本地ahead均为0；C已ff到SUPERVM该提交，没有夹带未推送通信实现。本次新增仅交接文档。

| 对象 | 已确认事实 | 状态及边界 |
| --- | --- | --- |
| 两部手机主聊天包 | `org.novochat.app.dev`、profile v110，App来源`aa192ba`，SDK `06801146afe0ef3f8535b5b53694cdb6ed20e87b` | 最近逐机安装/验收记录为2026-11-02；不是当前共享main的手机部署，未重新读设备或重启App |
| v110保留APK | SHA256 `9e0fa3cc6e36c6ab03942792dd1c9c677214070406f195ae199cc6787efa0467`；buildId `bf89c181e1ba471ea777ad0d2ec13547` | 本次只读重新计算保留APK及包内ARM64 native哈希，与原公开候选记录一致 |
| v110通信native | `libnovochat_core.so` SHA256 `0a9d90f28578cc9e997fcd8bbd41708f558b3e6b4e3b972e10068df36fb175a5`，与v109相同 | features为`tls-transport,mailbox-delivery`；该包没有后续P2资产V3改动；普通构建未开native诊断观察器 |
| 手机实测范围 | 同签名原位升级、身份历史保留、双向文字/图片及持久设备回执、输入、有限L4组合；v109离线/独立补收/B后台，v110反向后台及最终配对 | 有限已实测，不把v109项目改写成v110全部重跑；不签深度休眠、长期稳定、任意NAT直连或生产可靠性 |
| C的selfrun候选 | 程序`9a6153f1eebda4da5c10829abf77575aa106ba8f`，r2 ZIP和AOEM pin见[原C公开回执](../artifacts/audit/validator-c-selfrun-install-20261106/README.md) | 已安装独立ProgramData目录、LocalService用途钥生成/读回及验签KAT通过；未安装节点服务、未启链，不是手机SDK升级 |
| C旧信箱/relay宿主 | 本次只读进程枚举未观察到`novovm-rendezvous-service.exe`、`novovm-node.exe`或`novovm-relay-service.exe` | 不将历史PID/历史宿主版本当作当前正在服务；不能由该枚举推断云端/B或其他命名入口状态；本轮不启动或更换服务 |

手机公开证据为[候选记录](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/docs/implementation/evidence/sdk-migration-20261102/candidate-v110.json)、
[逐项验收](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/docs/implementation/MOBILE_CANDIDATE_ACCEPTANCE_2026-11-02.md)。
旧v108回退及聊天身份、历史继续保留。P2的V3源码/构建成功与旧Android AOEM批量能力拒绝分别成立；
Windows新selfrun KAT通过不解除Android运行库交付缺口，也不代表手机已做资产扣款。

### 2. 真实调用链：发现到重连

以下App符号固定于上述NOVOCHAT提交；v110共享实现按`06801146`读取，不拿最新共享签名API替换历史解释。
这些是并行且受门禁的路径，并非每条消息都顺序经过每一类载体或所有节点层级。

| 阶段 | 已有调用入口与所有者 | 已实现/实测和仍缺的证据 |
| --- | --- | --- |
| 启动及网络代次 | Android `MessagingService` / `NetworkLifecycleOwner` → `NativeNetwork` JNI → `MobileNetwork` → App原automatic owner；`app_delivery_owner::endpoint`保留Iroh endpoint，换代前等待旧借用任务结束，再调用`network_change`并清原peer cache | 手机App接线已部署；相同LinkProperties不重复增代次。不能据此承诺系统强停后自动复活 |
| 引导、信任及发现 | `app_delivery_adaptive::AdaptiveWork::run`先核验本地授权/ApprovedProfile；已有`refresh_bootstrap_sources_observed_v1`、`ServiceLeaseBookV1`读取签名服务声明；`discovery_round::discover_peer`及`endpoint_lease`处理成对不透明slot、短期密文地址、签名/代次/期限；消息路径还读L3/L4签名描述符 | App已有接线及有限双机收发，不是公开链上好友目录，也不是全网DHT已部署。不能从未认证声明接受任意relay或新身份 |
| NAT与候选连接 | `connection_attempt::CandidateSpec::{new,relay_only}` → `IrohConfigV1` / `IrohEndpointOwnerV1` → `IrohScopeV1::connect`；Iroh负责QUIC地址探测/穿透及可达性选择。构建采用Minimal、清默认address lookup、显式relay集合/UDP DNS、禁fallback DNS及portmapper；relay-only禁IP transport | 这是现有Iroh能力消费，不是自研打洞完成。v110闭环未逐连接证明打洞成功率；L4实测含IPv4成功与原IPv6失败，不允许改写为任意公网NAT已通 |
| 原生UDP优先路径 | `ContinuousMailboxWork::run_native_attempt`取原pin的最新合法locator → `io_owner::native::native_mailboxes` → `NativeEndpoint::connect_clients` → `mailbox_link` / `NativeObjectLink` / `NativeMailboxDriver`；`native_locator::prepare`、`prepare_native_bulk`及`maintain`核对/维持路由 | 已接App，存在真实L4 Put/证明/Fetch/解密/Consume证据；该证据不证明当前NOV主节点已接动态发现/选路 |
| 认证载体上的原生业务 | `MailboxWork::run_service_node` → `run_native_primary` → `drive_carrier_mailbox`；签名能力选择NVMBOX07或V8，不是任意Iroh地址自动升级。V7结构化bound认证或V8 `authenticate_mailbox_native_v8`及双向Finished → `NativeCarrierPump::attach_continuous_v7` → 同一`NativeObjectLink` / driver / 原MailboxService | App已有消费；UDP和carrier按同pin的`ServiceRoutes/ReceiptLane`互斥交接，仍健康L4不因L3路径失败被无条件关闭。没有另建信箱、身份或账本 |
| 端到端内容与送达 | `message_delivery` / `MailboxWork::run_native_lanes_v7` / 原outbox/inbox及回执队列；L3临时密文保管、可选收件方L4快路径、本地可靠提交与设备回执分别处理 | 手机有限文字/图片/独立补收通过。socket ACK、节点存储证明、设备持久回执是不同层级；不能拿寄存当最终送达 |
| 直连、单中继与多跳 | 自动候选可由Iroh选择direct或单relay，显式relay-only是路径限制；App `RouteGate`仍检查Privacy资格。共享`adaptive_overlay::{adaptive_overlay_candidate_routes_v0,decide_adaptive_overlay_route_with_family_cooldown_v0}`已有direct/relay/multihop候选、健康和家族冷却选择 | 前者有既有工程模式手机通信证据，但不是匿名。后者有源码/原选择测试及overlay gate消费者，不等于v110或NOV主节点已完成可用多跳执行/匿名实网验收；本轮未重跑这些测试 |
| 重连与任务接替 | Android网络变化 → 原owner退役旧操作；`run_native`、`run_service_node`按类型化可恢复边界重连，同pin重取新locator；`app_automatic_chat`在原消息/日志上重新核验。`ChatLeaseExpired`仅允许原所有者重新授权，撤销/认证/协议/未知错误不自动重试 | 旧v108本地到期接替修复及v109/v110有限恢复证据保留，不将其他历史E8、停滞、TLS长尾关闭。当前SDK `NativeEndpoint`另有configured/client reconnect入口，不能仅因存在就称全网发现和换路已接通 |

App入口位置：[App adaptive owner](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_delivery_adaptive.rs)、
[UDP owner](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_native_io.rs)、
[carrier接线](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_native_carrier.rs)、
[L4宿主](https://github.com/XujueKing/NOVOCHAT/blob/45e3c917225ad2d4bded1a7560037b58a2ebd701/crates/chat-core/src/app_l4_task.rs)。
发现、地址和重连的合法性检查应复用现有owner，不复制App业务信箱到共识主节点。

### 3. 实际算法、SDK和外部依赖（按层区分）

| 层 | 当前可核实算法/依赖 | 不应扩大宣称的范围 |
| --- | --- | --- |
| v110通信身份、声明/连接绑定 | 旧SDK `product_overlay` / `transport_binding`仍用Ed25519身份签名；会话为k256/secp256k1 ECDH、HKDF-SHA256、ChaCha20-Poly1305，双向Finished及防重放 | 未因后续NOV资产采用ML-DSA而自动升级；不能把k256误写成X25519，也不能称旧手机全链路抗量子 |
| 联系人私密发现、离线密文、本地数据 | App `contact_discovery`为X25519/HKDF-SHA256；短期地址用Ed25519签名及ChaCha20-Poly1305；SDK `mailbox_seal`为HPKE X25519/HKDF-SHA256/ChaCha20-Poly1305；本机目录/消息日志使用XChaCha20-Poly1305 | 这些均非抗量子密钥交换/密文方案；ML-DSA签名不能替代它们的保密性迁接，也不自动赋予匿名 |
| 当前共享原生身份 | `product_overlay::TransportIdentity` / `transport_binding`已迁为ML-DSA-65，公钥1952、签名3309字节，由原qualified crypto owner供签验；当前会话仍用上述k256 ECDH及对称算法 | 新网络签名策略与已装旧手机身份是不同状态；不放宽检查或静默重建原手机身份。不把单签身份资格写成端到端PQ保密已交付 |
| 可靠载体及解析 | 实际锁文件：vendored Iroh/iroh-dns/iroh-relay/noq均1.3.0、rustls0.23.46，TLS/QUIC使用既有aws-lc-rs后端；HPKE0.13.0、ChaCha20Poly1305 0.10.1。App通过兄弟仓path依赖直接消费，不是已发布独立SDK版本 | 签名目录可信与载体安全是两层；TLS套件未在本轮抓取协商结果，不推断TLS/PQ协商成功 |
| 外部基础设施 | 获准的Iroh HTTPS relay、显式DNS解析和Android Connectivity/后台执行规则仍是现有依赖；wrapper未启用默认N0 address lookup/PKARR发现，不能据此说无任何外部服务 | 不将Iroh relay替换为NOVOVM WSS URL冒称互通；现有验收记录明确有公共Iroh relay依赖，本轮不读取/公开私有配置来补猜具体地址 |
| Tor/其他适配 | Arti/tor-chanmgr0.47.0为显式onion/privacy-probe诊断特性；snow0.10.0在App dev-dependencies | 不属于v110普通main构建启用的默认聊天通道；依赖存在不等于Tor或Noise已成为当前生产会话协议 |

算法证据固定在旧SDK [product_overlay](https://github.com/novovm/supervm/blob/06801146afe0ef3f8535b5b53694cdb6ed20e87b/crates/novovm-network/src/product_overlay.rs)、
[carrier配置](https://github.com/novovm/supervm/blob/06801146afe0ef3f8535b5b53694cdb6ed20e87b/crates/novovm-network/src/iroh_transport.rs)，
以及当前[共享签名owner接口](../crates/novovm-network/src/product_overlay/identity.rs)。
不同签名格式/能力必须显式迁接并验收，不能把当前main直接当v110构建输入或声明已兼容。

### 4. A/C接续边界与接收回执

当前NOV主节点原链路为`service/config::NativePeerConfig` → `NativeChannel::from_transport` →
`UdpTransport::into_authenticated_objects_v1` → `NativeEndpoint` / `NativeObjectLink` →
原`NativeLifecycle`、共识/查询/执行。应用客户端另复用`NativeApplicationClient::connect`。
参见[主节点装配](../crates/novovm-node/src/native_pipeline/consensus/native.rs)、
[UDP入口](../crates/novovm-network/src/transport/native_ingress.rs)、
[应用客户端](../crates/novovm-node/src/native_pipeline/service/client.rs)。
现有配置对端列表/地址装配不等于统一选路完成；`product_nat::fallback_after_nat_failure_v1`
只是结果分类，不能拿返回RelayNovoRudp枚举当作已经建立中继或完成NAT打洞。

- **A**：审阅复用现有发现/签名声明、地址资格、候选选择、NativeEndpoint/ObjectLink及
  原重连机制，完成主节点接线和对应验证；固定最终共享commit、实际调用入口、能力/运行库
  pin及失败传播边界。不得把聊天单中继/多跳候选或旧手机Ed25519实现原样当新主链资格。
- **C**：只消费A交付的统一接口及Android合格运行库；明确原身份兼容/显式迁移后再更新
  手机JNI、确认页、原owner及App恢复接线，保留身份历史。不重复修改共享发现、密码、选路或账本。
- **本轮交付**：仅源码/既有证据和保留APK摘要核对；没有新构建、安装、网络探测、测试、
  配置变更、服务启动、资产操作或生产收费。后继缺口不能用重复已通过的手机用例代替。
- **A接收时回传**：`received_commit / shared_network_owner=A / mobile_owner=C /
  reused_entrypoints / node_route_integration_status / Android_runtime_delivery / remaining_evidence`。
  Git推送仅代表交接可取得，不代表A已经签收或节点统一选路已完成。

### 5. 2026-11-07 validator-A实际接收回执

- `received_commit`：SUPERVM `d6980df85ea930548a5837e1375e49796ec0c9b3`，
  NOVOCHAT `04f1cccb3b81901d2f9afb1e49351e4da6df288d`。A已fetch，NOVOCHAT本地
  main快进至04f1ccc；SUPERVM以main合并提交`49f1882b7e7ba31271fa5820af99b17e96112720`
  无损接入d698，保留A原有本地提交及未提交修改，无新开发分支、reset或强推。
- `shared_network_owner=A / mobile_owner=C`：确认本交接分工。A只负责共享网络及
  原主节点消费接线，不接管手机owner/JNI或重新安排已通过的手机验收。
- `reused_entrypoints`：原`NativeEndpoint / NativeObjectLink`、
  `UdpTransport::into_authenticated_objects_v1`、`NativeChannel::from_transport`与
  configured reconnect；原`NativeCarrierPump::attach_continuous_v7`、签名发现及
  候选/健康选路能力按实际合同复用，不复制App信箱业务到共识循环。
- `node_route_integration_status`：**未完成**。A已对照本机service/config、native ingress
  与App native/carrier入口，当前验证者仍由`NativePeerConfig.address`装配固定UDP对端；
  共用对象/认证底座不等于已经消费统一发现、NAT和选路。不是另写了一套UDP协议，
  但独立固定地址装配的漏接由A负责，不能以底座同名将其销账。下一项修改须在原
  网络所有者内闭合实际会话/IO、预期传输身份与验证者绑定，不用临时探测映射冒充。
- `Android_runtime_delivery`：**当前合格Android运行库仍待单独交付/消费验证**。
  A已有Windows/Linux候选及KAT不能代替Android ARM64资格，不要求C自行造库、
  删能力检查或回退Ed25519。v110继续按其SDK06801146/旧通信算法和有限已通过范围登记。
- `remaining_evidence`：当前ML-DSA主节点的原所有者选路/真实中继执行、路径失效恢复、
  旧会话拒绝及合法新会话继续工作，尚须对应接线后验证；多跳候选不冒算实网多跳。
  A已核对仓内v110公开candidate JSON中的APK/native摘要与本节一致，但未取得或重算
  C保留APK，未访问手机、私钥或其原始私有现场；摘要一致不写成新实机测试。

本回执是A实际阅读与分工接收，不是通信迁接、Android新包或主链部署完成。
本次未触发云端CI，不要求C/D重复清点/配对，不操作B现有服务；接线工作包本地验证
后集中交付，不能因签收就让C用A当前未冻结工作树重编译覆盖v110。

### 6. A接线切片：唯一NOVONET的认证UDP源学习

当前实际改动沿原endpoint/reconnect → ObjectLink → AuthenticatedUdpIngress →
NativeChannel → service启动。主节点的本地显式`learn_authenticated_peer_routes`
默认false，开启后仍固定完整传输公钥/验证者映射、peer索引、原握手、AEAD和重放；
仅新secret确认后换路，旧会话在确认前保留。没有新网络、线程、探测codec或账户。
原`product_nat_runtime`增加同策略的薄验收开关，只允许明确配置peer候选的observer，
不开普通client槽；配置开关为true不单独等于实际换源成功，须核对真实源及新会话。

本地本轮实际结果：网络Windows936通过/13忽略、Linux933通过/13忽略；配置各10项；
真实AOEM四服务/四库原流程各1项；受保护探测的原默认重连及新configured学习各平台
2项；NAT配置形状各2通过/4原忽略。后者的ignored实库用例中上述2项已独立显式执行，
不把普通筛选中的忽略计成通过。Windows原genesis CLI14通过/3忽略，仅受新字段
夹具影响，不重写旧真实审批资格。两平台network/node lib严格Clippy及Windows
network测试目标严格Clippy通过，格式/差异检查通过；未触发云端CI。
本地原日志及Linux首次custody拒绝保存在`target/novonet-route-integration-20261107/`。

Windows/Linux的原网络回归、配置及真实AOEM四服务/四库/回执和原库恢复已按
[共同配置当前节](NOVOVM-FOUR-HOST-COMMON-CONFIG-20261102.md#2026-11-07-网络优先既定身份下的实际udp源认证学习)
记录。实际跨公网新候选资格单列，不把旧9a网络观察或v110手机成绩升级为新包验收。
当前只闭合已配置身份在当前UDP/NAT拓扑的路由接线，统一carrier/完整多跳执行与
公开自动发现仍有后继缺口，不把本地构建或分类枚举当作它们完成。

对C的算法答复也已按实际调用核对：v110并非只有登录用Ed25519；新Message、
DeviceReceipt、寄存SignedContent、附件对象/manifest和地址声明均有签名。相同ID
消息、回执及持久寄存plan重试复用原字节，不是每个NovoRUDP分片公钥签名。
迁接ML-DSA须保留这些复用、原消息/用途/历史及密码边界；当前无手机ML-DSA时延/
字节/CPU/能耗对照，不承诺无体验变化。C暂不据A未冻结代码升级v110或清身份。

### 7. 2026-11-07 A继续整合：UDP优先、逐对端认证备用与原账换代

输入为A本地main `8ee31340`，C交接仍按第5节固定版本；后继在本地main累计，尚未冻结可交付候选或推送，
不要求C读取未提交实现或更新手机。**NOVOCHAT仓库/App/JNI和保留v110本轮未修改。**

共享网络沿现有`NativeCarrierPump -> NativeEndpoint -> NativeObjectLink`增加
`NativeEndpoint::join_bound_carriers_v1`：只消费原已完成载体绑定和Finished的fresh
会话，同一完整本地身份、不同远端身份及真实载体路由，进入一个对象owner；
所有连接共享原lane总消息/字节预算。没有新握手、网络线程、转发daemon或JSON热载荷。
缺少所需stream、重复身份、错误本地身份、已开始会话不能冒充合法装配。

生产主线继续是原`UdpTransport -> AuthenticatedUdpIngress -> NativeChannel`，
**默认NovoRUDP，Iroh仅合格备用**。新增`replace_bound_carrier_v1`逐对端消费
完整Finished endpoint，完整ML-DSA钥、原peer索引和lane集合不变，不能以Iroh端点
key代替原传输身份。原UDP收包/客户端准入/批量发送保持原处，所有路径共用原对象
与wire预算；`novovm-node/iroh-transport`显式消费既有共享feature，不改变默认入口。

`NativeLifecycle`直接消费原ObjectLink换代报告，旧发送返回`SessionReplaced`，
不会记送达；保留同一份不可变业务消息，pipeline/journal/AOEM不重开。旧deferred
write的reservation沿原取消路径结算，未知结果保守计账，不发明实际发送字节。
`request_udp_recovery_v1`仅请求原配置UDP重新握手；未完成认证前仍使用备用，
不是调用成功即宣布恢复。已删除本轮尚未提交的全对端carrier-only服务装配草稿，
没有将其保留为另一个产品启动模式或要求关闭客户端。测试专用carrier构造保留，
不作为部署入口。本次未切现场包、主链或AOEM。

原`novovm-node` tick已接入`service::network::Driver`，仍先构造原UDP节点；
`ResidentConfig.novonet_config_file`明确配置后才启用同tick中的原Iroh owner。
单一保留current-thread runtime驱动原认证/Finished/NativeCarrierPump，无新增网络线程。
原签名目录/ServiceLeaseBook/RelayCandidatePool复用，完整原传输钥与固定chain/genesis/
protocol二进制选择共同核验后才换代；不借用信箱能力、签票权或资产钥。
每peer原UDP有保留待发且收件停滞后才请求备用，健康UDP不为凑备用会话而改路；
备用期间以原UDP握手试恢复，实际认证通过后原UDP接管。对端owner重启的新签名地址
允许重新拨号、认证和原对象交付，不重开本地账本。原选路退避/错误记录继续有界。

**发现范围尚有限**：目前自动刷新的是本地公开签名locator输入，不是跨机器自动传播
的发现目录；序号/新鲜度、固定peer钥映射不能代替公开分布式发现。记录每40秒发布、
120秒有效，需要继续接合原发现RPC的实际分发，不能靠Git不断拷贝短租约冒充自动发现。
真实原生多跳仍未接通，状态明确输出`global_auto_discovery=false`和
`native_multihop_accepted=false`。本机IO、源码接线及合成打包不构成SCM或四机上线验收。

本地定向10项通过（含既有carrier回归），覆盖混合UDP对端与真实开放UDP客户端、
65片、共享预算、错误身份/lane拒绝、换代预算退休及原UDP重认证。恢复后实际
双向收件/DONE及重复维护不重置握手的加强断言已重测通过。另有两项实际localhost Iroh
测试通过：原二进制通道往返，以及已认证UDP通道逐peer切入Iroh且保留客户端入口。
这些使用品牌化密码组件夹具，不是实际AOEM/PQ、手机、物理多机或性能签收。
全network默认并发结果1257通过/20失败/13忽略，失败记录保留，包含共享endpoint
准入耗尽及进程全局计数相互干扰；原生产限额未提高。同network完整串行结果为
1280通过/0失败/13忽略，不覆盖尚未执行的实机/移动场景。

原tick保留reactor的三项实际本机IO测试通过：无需求时不替代健康UDP；请求备用后
原二进制消息收件/DONE；错误chain在handoff前拒绝；以及对端owner退出重建后以
新会话继续原NativeChannel双向交付。它们使用组件密码夹具，不替实际AOEM资格。
Windows相同feature下原`native_pipeline`分组581通过/151忽略，Linux分组580通过/
150忽略；实际AOEM四原服务的
认证地址学习、转账/重花、账户权限、费用及冷恢复定向用例通过。这项实际AOEM用例
仍走UDP，不冒充备用链路的完整资产验收。网络/node测试目标及实际node bin严格
Clippy通过。两平台原打包器合成装配、34项安全断言和构建失败传播检查通过；
合成制品不发布，node实际build合同现明确包含`iroh-transport`，无配置仍默认UDP。

另保留`node-full-feature-regressions.log`：将全部node测试混在同一默认启动环境的
初次本地运行出现失败且未完成，已停止该确切测试进程；直接复查一个原始失败表明
它要求原CI专用Host兼容测试环境，未据此归因其余全部失败，也未修改生产执行设置。
后续沿原CI分组显式配置实际库与manifest验证，不把该失败改写为全套通过。
日志在`target/novonet-route-integration-20261107/`；当前没有新云CI或实机部署结论。

随后审查发现备用断线时仍有待发数据的窗口：原ObjectLink将终止载体IO错误向上返回，
配置型节点会冻结，空队列后才重启的测试未覆盖它。新增反例在修正前返回
`native carrier IO failed: connection aborted`，修正后10项通过。现在只有明确启用
configured reconnect的已绑定对端，可保留未知发送直到原TTL或新的认证会话；错误计数
保留，不推进发送游标、不伪造收件或新session。反馈/过期取消仍沿原reservation结算，
独立可靠流没有该重连owner时终止错误继续拒绝；签名/AEAD/协议错误不按IO吞掉。
真实Iroh重启用例也加强为带待发数据断开：原通道继续poll、记录失败，认证替换后恰好
一次`SessionReplaced`，然后合法新消息双向收件/DONE通过。它不证明业务已自动重放。
新增修正后的日志使用`*-reconnect-final-*`独立名字，不覆盖先前的成功或失败证据。
该修正后的Windows/Linux分组、实际AOEM定向恢复、严格Clippy和默认UDP配置检查已
分别重验通过，旧结果不冒充新源码。两平台另有原SDK消费者回归：
`native_application_transport`各3项、`native_mailbox_session`各12项通过（组件密码、
实际本机IO，不是手机v110已迁移）。两平台实际`novovm-node`带`iroh-transport`构建
成功，`--service-contract`确认原tick及existing；Windows支持SCM、Linux如实不支持
SCM，三个副作用字段均false。这只是debug可执行文件的离线入口检查，不是服务启停、
候选发布、四机安装或启动验收。状态里的bound数量也只是已装配路由，不是对端在线、
业务送达或最终确认的证明。本轮NOVOCHAT/App/SDK发布依然未变，C无需操作。

审计另核实：原`relay/data_plane.rs`的Direct分支直接构造`delivered=true`，
MultiHop调用内存`forward_multihop`。这些是现有smoke模型，不是可靠的真实逐跳
交付证据；不能接入主网后沿用其成功口径。产品relay的认证来源绑定不能删除来
让转发“通过”。后续应沿现有选路、认证和relay所有者补真实执行，不另造通讯产品。

### 8. 2026-11-07 A接回原发现RPC：公开节点locator与私密联系人分域

本轮继续基于`6ed6ea39`的同一个主节点owner，不重写NOVOCHAT发现流程。
已逐字读取用户最新发现参考；并核对NOVOCHAT的`discovery_control.rs`只是共享模块
重导出，`discovery_round.rs`实际调用共享RPC；`endpoint_lease`的4096字节加密租约、
联系人共享秘密/slot及ACK属于私密联系人合同，不能直接拿来公开发布验证者地址。
这些组件已存在，不登记为本轮新开发，也不据旧手机包替当前主链取得资格。

现在复用同一个`discovery_control`二进制编解码、请求/回执及`RendezvousStoreV1`，
增加显式公开节点profile（NODE_PUT/NODE_GET）。公开slot由当前Finished认证身份
派生，客户端不能指定别人的发布槽；与联系人opaque PUT/GET/ACK分域，不能互读、
抢占或确认删除。共享原16条/64KiB live、64条retained、每owner8槽、4KiB载荷、
30秒租约及32次RPC限额，不扩配额、不做身份枚举。普通持钥设备可提供元数据；
取到元数据不获得共识票或花费权，也不变成官方准入证。

`discovery_control::node_locator`是原`PeerSignedRelayRecordV1`的严格二进制投影：
保留原签名、公钥预期和签名验证器，chain/genesis/protocol由消费者独立固定。
完整ML-DSA公钥由原peer映射取得，不在每条RPC记录重复发送；固定字段只能按明确
profile恢复，无法无损表达的endpoint政策或超过原4KiB的地址声明明确拒绝。
没有把JSON记录塞进数据通道；本地locator配置/审计仍允许JSON，不宣称全程零复制。

主节点原IrohScope内增加公开profile分派与后台有界lookup，复用原结构化ML-DSA
Finished认证。`NVNETOB1`/`NVNETDS1`仅分派用途，不授予身份；随后的加密选择核对
固定链承诺，发现实例由本次认证服务提供。原RPC从选择后的AEAD序号2接续，
不重置nonce/认证计数。Object通道继续原NativeCarrierPump/NativeChannel；入站
对象身份必须匹配原完整peer钥映射，不依赖未认证Iroh端点hint取得权限。

新增可选配置`discovery`（默认false）、`discovery_seeds`（最多2个本地批准的公开
引导locator文件）。原`peer_records`仍可用；启用发现后可为空，节点通过实际RPC
发布自己的签名声明、查询原peer钥对应的最新声明并接回原选路owner。引导的历史
坐标仅用于原self-owned策略允许的新持钥认证，不能当成已延长的业务租约；取回的
业务候选仍验证原签名/期限/序号。实例100秒轮换、原120秒到期，已建立的健康对象
通道不依赖发现TTL续命；UDP依旧默认，只有原条件请求后才使用Iroh备用。

这两个入口标记属于本轮尚未部署的node适配器组合，需匹配制品；不能把旧node包
当作新profile已部署。NOVOCHAT/App/JNI、v110及手机配置未修改，C当前无需操作。

**范围仍须明确**：这是固定自营peer集合的真实网络元数据交换，不是任意未知设备的
完整自动入网、开放L1、ZK同步或全球分布式目录。Iroh坐标不能冒充实际UDP socket
的NAT映射。引导owner冷启动后端点变化仍需可用来源，所有引导同时冷重启的自主
恢复未签收；真实逐跳relay转发/最终收件也未完成，`global_auto_discovery`及
`native_multihop_accepted`保持false。不把本机QUIC直连称为实际公网Relay或多跳。

本地证据继续记录于`target/novonet-route-integration-20261107/`，不覆盖旧日志。
首轮发现RPC15项、后补公开profile反例后的16项及store15项通过。节点首轮4项通过：
三节点只有一个引导地址、没有彼此peer文件，实际RPC发现后原对象收件/DONE；以及
既有默认UDP/按需备用、重启重认证、错误链域拒绝。后续将错误链用例加强为对端签出
可接受locator但实际运行另一链，必须由加密选择拒绝；又加强引导离线后原peer继续
实际交付（不只比较session ID）。最终受影响作业与两平台范围按后续结果追加，
不以首轮结果替后继源码，也未启链、变更B/C/D或产生TPS/候选发布结论。

最终同一工作包的本地受影响作业已完成：Windows原`native_pipeline`582通过/151
忽略，Linux 581通过/150忽略；其中包含上述加强后的4项真实本机网络用例。两平台
共享发现RPC各16、store各15、混合carrier各10、默认UDP配置各11及SDK消费者
各3+12项通过。两平台实际指定AOEM库/manifest的四原服务认证地址学习、转账/重花、
权限/费用与冷恢复定向用例各1项通过（该资产用例仍走UDP，不是备用资产验收）。
库/测试目标及实际node bin严格Clippy、fmt与差异检查通过；两平台实际debug node
构建和`--service-contract`核验通过，三个副作用字段均false。这不是候选包发布或
真实SCM生命周期验收，也没有新增手机/公网Relay/多跳/TPS成绩。

保留`windows-discovery-validation.log`中的580通过/2失败，不改写为通过：首次加强
用例只poll一次便读文件，声明尚未真正发布；另一个故障注入用例关闭引导后仍要求
覆盖全部peer的IO报告零错误。现按真实发布条件等待，断线IO计数保留且必须被观测，
另外两台的指定新消息仍须实际收件/DONE、session保持不变，不能用计数归零代替。
修正后的独立日志为`windows-discovery-final-2-*`、`linux-discovery-rpc-final-*`及
`*-discovery-consumers-final-*`，不覆盖失败。没有提高生产超时、放宽认证或更改共识。
本轮及时提交同步main，不额外手动触发云CI；main原自动工作流及候选门禁保持不变。

### 9. 2026-11-07 引导冷恢复与真实逐跳接线（基于b0594b89）

沿第8节的同一发现RPC、store、ServiceLeaseBook及主节点owner继续接线；没有
重写NOVOCHAT、另建发现服务、UDP socket、路由守护程序或执行/共识队列。
原生NovoRUDP仍是默认，只有原按需备用条件触发后才考虑本节的Iroh载体路径。

**冷恢复。** 可选`carrier_identity_file`沿原用途信封与OS custody保存独立随机
carrier seed，绑定本机ML-DSA传输公钥及链域，不从资产钥派生。不同suite不能被
读成ML-DSA签票/花费钥。丢失已发布身份的保管材料时拒绝静默重生。实际调用仍是
原IrohEndpointOwner的稳定身份入口；第三方载体密码学不取得原生授权。

本地公开`*.discovery-cache.json`最多保留两个已签引导坐标检查点，身份信任仍从
独立配置的原引导pin取得；缓存不能引入新pin。原签名、链承诺、未来时间、序号与
同序号冲突检查继续有效；旧磁盘副本不能回滚已学到的新声明。过期坐标只用于原
self-owned引导策略的新持钥握手，不能更新业务租约；取回的实际数据候选仍须新鲜。
稳定carrier身份不等于地址永远可达：同时冷启动仍需至少一条可达的既有引导坐标、
固定监听地址或获准可用的载体路径，不能无中生有发现全部消失的来源。

**真实逐跳。** 显式`relay_forwarding`（默认false）表示本owner愿意消费/提供这项
受限转发。已有验证候选按有界轮次尝试直连、一个中继、两个中继，不逐消息重选；
健康会话复用，断开先重试已认证工作的轮次，再沿原候选池失败/冷却推进。仍受原
Iroh连接/流数量及每分钟64KiB声明预算约束，不以增加线程、连接额度或批量掩盖。
这不是任意规模路由发现、所有peer全网状并发或物理抗封锁资格。

本轮恢复反例还定位到：原取消检查只覆盖最终对端的carrier key，未覆盖正在连接
的中间跳。现沿同一已验签locator更新入口核对待连接路径的每跳key/direct/relay
坐标，更新后取消旧尝试并在同一有界轮次重新认证；绝不据新声明拆掉健康会话。

`relay::hop`接在原载体分派`NVNETH01`，每跳沿原结构化ML-DSA/Finished和真实载体
binding认证；二进制OPEN固定有界路径、随机circuit、链/创世/协议上下文、30秒
建路期及当前跳。接收方必须核对当前跳和实际上一跳认证钥，origin提示不授予转发
身份。READY仅表示下游建路，不是对象交付。DATA逐包经原AEAD与重放检查绑定同一
不可变路径摘要；不新增JSON业务热路径、不解释APFL/业务内容、不跨会话复用认证。

源与最终接收方另外沿**原native ML-DSA握手及AEAD**验证独立预期钥并双向确认同一
路径摘要，只有成功后才产生不透明`VerifiedRelaySession`并交原NativeCarrierPump/
NativeEndpoint/NativeObjectLink。端到端钥不交给中继，来源仍记录真实第一跳binding，
**没有伪造最终节点的TLS exporter**。这项原生E2E确认与逐跳Finished是两层不同
证据，不宣称新增抗量子密钥协商、匿名性、ZK最终性或共识资格。

本轮真实逐跳的**下层载体是Iroh备用流，不是新建的UDP逐跳转发或已退役WSS**；
转发的是原native二进制对象密文。桥接每方向最多保留一帧，写完成才继续读；
同owner的全部线路共享原NativeWireBudget，耗尽明确失败且不产生DONE。取消中的
发送保留原unknown/uncertain记账，释放预留槽而不把未知字节算成功；收件及DONE
继续由最终原对象所有者判断。不能拿这个组件结果宣称四机APFL资产已经过中继。
这里计量OPEN/READY/DATA封装字节，不是QUIC/TLS、载体握手及物理链路总流量；
本轮不声称完成真实中继带宽、吞吐或完整计费验收。

封装修正仍用原连续帧IO所有者：native内部数据报保持1,200字节上限；中继外层
承载握手及密文信封，使用原通用16KiB帧上限。保持有限scope不可升级、空闲撤销/
取消终止、首字节后固定30秒部分帧期限，不通过延长生产超时解决错误的帧profile。

本机结果、保留失败及最终受影响作业见本节后续收口；没有部署或操作B/C/D。
冷恢复/逐跳用例使用原component crypto夹具与真实QUIC/原对象IO，不是实际AOEM
后端的中继资产验收；实际AOEM指定库的原UDP业务回归单列，不相互替代。
NOVOCHAT/App/JNI未修改，C当前无需重做发现或更新手机。`global_auto_discovery`
及物理多跳验收仍未签收；任意未知设备、开放L1、ZK主网同步和公网拓扑另记真实缺项。

本轮本地收口证据仍在`target/novonet-route-integration-20261107/`，不覆盖旧记录：

| 实际范围 | 结果与口径 |
| --- | --- |
| Windows原native_pipeline | 585通过、151按原规则忽略，包含新冷恢复、逐跳/空闲/中继重启及新引导反例；不是忽略项也已执行 |
| Linux原native_pipeline与定向修复 | 修正前整轮583通过、1失败、150忽略；保留该失败。修正每跳待连接坐标取消后，只复验原network组7项，全部通过，不将它写成另一次完整整轮全绿 |
| 两平台逐跳协议反例 | 各3项通过，其中真实IO用例包含7种上下文/上一跳/目标/过期/重放/最终原生身份/签名反例；另含路由长度/环路/严格到期及共享额度/取消预留检查 |
| 两平台原连续帧回归 | 各3项通过：原生超长、取消/撤销终止、部分帧固定期限及原认证帧来源/重放检查仍保留 |
| 两平台OS carrier custody、混合载体、默认UDP配置 | 分别各2、10、11项通过；没有将全部peer改成carrier-only |
| 两平台指定AOEM库原服务用例 | 各1项通过，包含四个原服务/原库的转账、再花、权限、费用和冷恢复；实际仍走UDP，不能替中继资产签收 |
| 两平台SDK消费者 | 原应用transport各3项、原mailbox session各12项通过；不是手机重测或v110升级 |
| 两平台严格检查 | network/node/relay库与测试目标、实际node bin严格Clippy通过；fmt及差异检查通过 |
| 两平台实际debug node | 实际构建与离线`--service-contract`检查通过，service_started/database_opened/custody_accessed均false；不等于SCM启停、release包或部署资格 |

Windows最终日志为`windows-cold-hop-3-*`与`windows-cold-hop-final-*`；Linux最终
受影响复验为`linux-cold-hop-final-*`，其他既有未改范围见`linux-cold-hop-3-*`。
`two-hop-native-reconnect-3.log`还单独记录修正后的112.34秒通过样本；用例刻意保留
61秒真实空闲来覆盖原计量窗口及长期空闲，不是重连耗时112秒或新增生产等待。
只在额度可用时验收恢复；即时连续重连耗尽额度的失败保留，不声称限流时仍必达。

首次`two-hop-native-{1,2}.log`暴露外层误用1,200字节profile；修正后首个实际
双中继收件/DONE通过样本为`two-hop-native-3.log`（47.35秒）。后续恢复首轮夹具
误把旧文件的较高序号当成新owner实际发布，已改为核对真实新carrier key；
`windows-cold-hop-2-native-pipeline.log`还保存额度不足的失败。可用额度恢复用例
仍在旧中间跳坐标上超时（`two-hop-native-reconnect-2.log`及Linux修前整轮），
进而定位并修正上述真实每跳取消缺口，不靠重复运行、扩大资源或放宽期限过关。
早先`cold-native-owners-1.log`的原4验证者映射夹具错误与初轮严格检查的测试迭代
写法错误也如实保留。验证脚本拒绝复用同名日志的一次中止只补跑剩余步骤，
没有覆盖旧证据或把未执行检查记通过。本次没有手动触发云端长测、重放六组配对、
打正式平台包、启链、变更经济/身份规则或产生TPS结论。

### 10. 2026-11-07 参考审阅：原APFL/统一语义、指纹边界与固定制品

用户本轮提供的ChatGPT附件与正文均已全文读取。附件SHA256为
`01dea4f4875896ed550e9981e849cc4d84a838a6c5f184569bfdd4d61fc838d0`。
它们是设计参考，不以其中的建议名称或伪代码覆盖已有最高架构、签名与接口合同。
本轮源码审阅及候选程序基线均为`ce1f54ea545e69dc64763f1f24f6048d90e4ef12`；
本节之后的文档提交不重标程序来源。本轮不开发第二套APP/ATIR、注册表、执行器、
选路器或载体，也不把参考保存成新一轮架构重写任务。

**第二/三层不是从零缺失。** 沿原APFL与`node → exec → bindings → AOEM`逐项映射：

| 参考目标 | 当前实际资产 | 不能扩大成什么 |
| --- | --- | --- |
| Schema/字典/参数列 | [通用APFL BatchView](../crates/novovm-protocol/src/apfl_batch.rs)已有schema/context、借用列、共享/字典/密集残差和展开/偏移预算；[NUAPFL03](../crates/novovm-protocol/src/uca_spend_batch.rs)已有15列及不可伪造的AssetSpendViewV3 | 不是另建APP注册表才能开始结构化，也不是任意业务自动lowering |
| 结构化资产批次进入主节点 | [consensus/transport/uca.rs](../crates/novovm-node/src/native_pipeline/consensus/transport/uca.rs)固定program/semantic/receipt profile，原批次编解码→AssetBody→BatchRequest::from_uca_batch→原BatchSource::Uca | 不拿旧NNX3/96字节投影代替当前ML-DSA账户合同 |
| Delta/Commitment/减少重复传输 | 同一UCA transport已有原pool正文引用，intent定位键只找材料，完整body_id仍绑定公钥和签名 | 不是通用跨批字典/StorageDelta协议，不是签名聚合或免验签 |
| AOEM原生结构执行 | [full_effects.rs](../crates/novovm-node/src/native_pipeline/business/nov_transfer_batch/full_effects.rs)约994行起原完整效应编译，经原integer/resident资源进入[semantic_integer.rs](../crates/novovm-exec/src/semantic_integer.rs)的run_composition_v1、统一wire opcode113及binary result take | 不是只有空接口；但默认CpuCallbacks与显式AoemIntegerV7不能混称所有部署均已直执行。实际配置与包能力分别核对 |

实际残留应按成本再处理，而不是新增一层名字：`pipeline/compute.rs`约543/558行
仍由`batch.row()`还原完整签名对象后批验签；`uca_spend_batch.rs`约334行复制key/
signature，签名原文准备仍分配；`persistence/packet/uca.rs`约72–82行仍有canonical
投影、decode/encode核对。先区分归档必要物化、可避免准备与当前父状态重验，再在
原owner内减工作量；不删原签名、承诺、nonce/费用/权限和持久化检查，也不能仅凭
发现复制就认定它是当前最大瓶颈。后续如确需跨批字典、通用StorageDeltaBatch或
ProgramOperationBatch，应先明确原语义/内容所有者及缺失、版本、哈希和资源合同。
旧opcode114诊断消费者不冒充当前UCA主线或已覆盖其授权关系。

“优先二进制热路径”继续适用，但“AOEM所有生产JSON均关闭”不成立：实际
ML-DSA消费者沿原二进制统一wire；整数数值结果走binary take，资源管理仍有JSON
状态读回；AOEM Production Surface还明确保留`aoem_capabilities_json`与canonical
privacy `aoem_privacy_execute_v1` JSON v2。不能为统一名字擅改这些合同，也不能将
离线配置/报告JSON混成APFL→JSON→二进制业务往返。普通聊天/文件不强制变成转账
Tensor或每包ML-DSA；原逐包AEAD不变。附件“永远保留聊天Ed25519”不覆盖已批准
的原生身份/用途签名ML-DSA迁移；旧手机包与第三方载体密码学仍分开记录，不改手机。
这里AOEM接口引用为本机`main@4c57372a`中未修改的Production Surface文档；
不拿其他未提交AOEM草稿替随包能力背书，实际库资格仍以本节独立制品回执为准。

**指纹与加密覆盖：只读源码事实，不是地区可达性或DPI实验。**

| 观察层 | 当前可见/受保护范围 | 源码锚点 |
| --- | --- | --- |
| 原生UDP公网旁观者 | IP/端口/包长/时序可见；NVRLY002、版本、session、双方peer_id、sequence、nonce、密文长度也是明文外层。AAD认证不等于隐藏 | [product_overlay/binary.rs](../crates/novovm-network/src/product_overlay/binary.rs)约41–65行；[product_overlay.rs](../crates/novovm-network/src/product_overlay.rs)的原AEAD/AAD |
| 原生UDP握手旁观者 | NVHCONT1分片可还原NVHSHK02握手的公开身份/公钥/临时公钥/挑战/时间/签名；回程cookie不是保密层 | [handshake_wire.rs](../crates/novovm-network/src/product_overlay/handshake_wire.rs)、[handshake_continuation.rs](../crates/novovm-network/src/novorudp/handshake_continuation.rs) |
| 已认证原生数据产品路径 | Frame kind/stream/object/ACK和APFL业务正文在原AEAD密文内；不将本结论扩大到全仓底层UDP/诊断API都自动加密 | [novorudp/datagram.rs](../crates/novovm-network/src/novorudp/datagram.rs)、原secure channel |
| Iroh下层旁观者 | 可观察QUIC/TLS及流量特征；流内NVNETH01等分派标记不等于公网明文magic；固定ALPN的实际可见性及识别率仍需抓包验证 | [iroh_transport.rs](../crates/novovm-network/src/iroh_transport.rs)、[transport_binding.rs](../crates/novovm-network/src/transport_binding.rs) |
| 逐跳中继 | 解密OPEN可见origin、整条path、context/circuit/期限/当前跳；也可见被转发的E2E初始握手公开身份及内层信封头，但没有E2E业务解密钥 | [relay/hop.rs](../crates/novovm-network/src/relay/hop.rs)、[hop/e2e.rs](../crates/novovm-network/src/relay/hop/e2e.rs)；不是洋葱匿名 |

当前Iroh复用QUIC，不再为“可选QUIC”另造平行载体。实际owner显式指定DNS、关闭
解析器fallback/默认address lookup/portmapper，Relay关闭或只用本地配置的custom
集合；对端direct/relay数量与允许集合仍有界，不是任意HTTPS自动托底。身份与地址
分离、健康连接复用、原生恢复及多来源发现已有接线，不重新定义Capability框架。
没有新抓包、UDP封锁注入、跨地区识别率或全路径抗封锁结论；不因二进制/E2EE声称
匿名、不可识别或所有路径被阻断仍可达。本轮不改协议头/端口/密码学或做流量伪装。

**继续原交付主线：两平台隔离候选已实际生成并离线核验。**

- 原打包器：`scripts/novovm-package-product-linux.ps1`，不重装、不启动服务，旧包不覆盖。
- 程序来源固定为上述`ce1f54ea`；node实际Cargo feature包含`iroh-transport`，默认
  NovoRUDP与原node/tick/执行/账本所有者不变。
- Windows：`artifacts/product-overlay/novonet-ce1f54ea-windows-x86_64`，77个清单文件。
  release-manifest SHA256：`6f9f222f738ca990636572ffcad6256b1cd294b082254912bfbd604f7081978f`。
- Linux x86_64：`artifacts/product-overlay/novonet-ce1f54ea-linux-x86_64`，80个清单文件。
  release-manifest SHA256：`d8375d86fa6d1ff50bc61c2f809776d6d7f7cb1e0259ce0832bd78403fd9ea5d`。
- 两平台独立重核每份artifact长度/SHA256及CHECKSUMS映射，并用**复制进包内的**
  novovmctl实际加载包内AOEM、manifest及固定hash，通过scalar/unified/mixed-batch
  验签KAT和sign接口形状检查；没有生成用途钥或以源目录库代替包内库。
- 两平台包内实际node `--service-contract`核对原existing入口与平台合同，
  service_started/database_opened/custody_accessed均false。独立回执位于
  `target/novonet-candidate-ce1f54ea-20261107/verified-{windows,linux}.json`，原构建及
  核验日志同目录保存。源码审阅不当作抓包，KAT不当作新逐跳路径实际AOEM资产验收。

本次只完成源码参考审阅、原候选构建和包内离线资格，未额外手动触发云端长测，
不宣布该SHA完整CI、SCM生命周期、四机新包安装/真实中继资产、自动公众入网或TPS
已经通过。B既有服务、C/D及手机均未操作。后继沿这些固定制品继续既定四机验收，
不为了参考材料重新搭网络、执行器或长期测试链。

### 11. 2026-11-07 四机实网：固定候选接收，不以READY冒充交付

已全文读取本次参考审计，附件SHA256为
`7d8daa4561492e82ec8478be372afb44a349ac517da3ca49bfa22d3641d6b567`。程序仍固定为第10节的
`ce1f54ea545e69dc64763f1f24f6048d90e4ef12`，不是随文档提交重新编译。原签名合同
继续有效；参考中的“永久保留聊天Ed25519”不覆盖现行用途迁移要求，本轮不修改
NOVOCHAT、手机、算法、共识、AOEM或经济。四机实网与包内离线核验分别记录。

沿已生成的两平台目录归档，**只分发下列r2归档**；初次归档使用了修改前的接收
脚本，未交给C/D，留在本机不作交付。r2没有改变包内程序或release-manifest。
二进制不放入Git；仅同步main不能取得程序。复制时不得覆盖9a安装、用途钥或原库。

| 接收物 | A本地路径（均在`target/novonet-four-host-ce1f54ea-20261107/`） | 字节数 | SHA256 |
| --- | --- | --- | --- |
| C/D同一Windows ZIP | `novonet-ce1f54ea-windows-x86_64-r2.zip` | 150354733 | `79cdfcf8636ddb881b2c9fa56d28c990ef5b12c8c7010f52cff0fea3a2576c9b` |
| B Linux TAR.GZ | `novonet-ce1f54ea-linux-x86_64-r2.tar.gz` | 358788674 | `e2da38deafb0a97e4f47739cee8eb0aea14e450a97c7bace3be5a803b0911fce` |

两归档分别核对原77/80项清单文件的**归档流**长度与哈希，清单、校验索引、原SDK
辅助校验索引与接收脚本另外核对；没有用途材料、账户钥、数据库或多余文件。原
打包器不把`CHECKSUMS.sha256`自身列入artifact；不能把“77清单项”误写成归档只有
77个文件。Windows辅助SDK索引作为额外元数据已核对，不修改生成包来凑文件数。

本次可复用接收入口为[`scripts/novonet-verify-fixed-candidate.py`](../scripts/novonet-verify-fixed-candidate.py)，
仅用Python标准库，Windows/Linux接收同一脚本。它独立固定两平台manifest pin，
逐项查长度/哈希及索引、实际node feature，再运行**接收包里的**ctl对**接收包里的**
AOEM做KAT，并运行实际node的离线service-contract；核对实际库路径及所有副作用
字段。缺失/错误清单在任何程序执行前拒绝。结果目录必须新建，原始stdout/stderr
保留；不创建钥、不访问custody、不建库、不注册服务、不监听或启动节点。

**C/D各只需一次接收操作，无管理员窗口、无预约短窗口：**

1. 将上述同一个Windows r2 ZIP复制到各机，核对外层字节数与SHA256后解压到新目录。
   不运行旧9a安装脚本，不cargo重编，不移动任何受保护用途材料。
2. 在解压目录运行下式。C使用`C`，D仅将`--host-id C`及报告目录名换成`D`；
   Python命令如为`py -3`，仅替换启动命令，不改脚本及候选文件。

```powershell
python .\novonet-verify-fixed-candidate.py --host-id C --package-root .\novonet-ce1f54ea-windows-x86_64 --report-directory .\ready-ce1-C
```

3. 将新报告目录中的`report.json`、`runtime.stdout/stderr`、`node-contract.stdout/stderr`
   作为公开回执回传main，并回报当前实际LAN IPv4供本轮拓扑使用；不重做硬件/身份
   清点。成功状态为`READY_NOVONET_CE1_C/D`。失败则原样回传，不改包、不清目录或
   覆盖失败，不补跑认证/交易。程序包、私钥、数据库不提交Git。

A已用该接收脚本对固定Windows包实际通过一次；错误manifest反例明确在执行ctl
前失败。B当前沿原SSH配置绑定A的LAN源地址已恢复访问，未改VPN、known_hosts、
防火墙或B服务。Linux归档只进入新建的`/home/ecs-user/novovm-incoming-ce1f54ea-20261107`，
不替换`/opt/novovm/releases/selfrun-9a6153f1-linux-x86_64`及任何current别名。
B已在该新目录完成归档SHA核对、解包及同一接收脚本的实际执行，取得
`READY_NOVONET_CE1_B`：80项791242071字节、包内core为第10节固定的d73e4a2f，
scalar/unified/mixed-batch KAT及实际Linux node合同均通过。不是A的WSL替代结果。
前后只读快照中原docker/nginx/novonet-mailbox/supervm-relay仍active，UDP监听集合
未变、`/opt/novovm/releases`名称未变。首次未绑定LAN的SSH在banner前超时，保留
该失败；沿原配置绑定192.168.71.74后成功，未改全局网络。

[A/B公开原始接收证据](../artifacts/audit/validator-b-program-install-20261102/novonet-ce1f54ea-ready-public-evidence.zip)
为6368字节，SHA256 `c63248910a36af45da6613c9e0afa89cf8f9f594923a0aee07f56c2d20576593`。
含实际stdout/stderr、原报告、B前后只读快照、接收记录和错误manifest反例；不含钥或库。
接收脚本SHA256为`c4ecbf44ad620f7a9caa605b4770feaabdf9dbdb272243660adf5fc2a237a481`，
与两归档及A/B实际报告一致。Python语法检查、差异检查和上述实际运行已在本地完成；
不额外手动触发或重复云端CI，不以本次源码/交接推送冒充ce1完整CI已通过。

**后续实网范围不缩水，也不提前记通过。** 待四端接收后，沿原node的
`native_execution_tick`与`ResidentConfig.novonet_config_file`组织实际拓扑。
新逐跳驱动在这个原入口中，旧`novovm-product-nat`成功不能替代它；不为方便另写
探测路由器或拿内存/localhost用例冒充四物理机。初步角色为A源、C/D逐跳参与者、
B公网对端；实际路径以两端和中继记录确定，不能把B不能直接路由的192.168地址
标为公网可达。不得为全网状会话加双中继而提高原连接/转发额度。

分别取得NovoRUDP直连、单中继、双中继、并发对象最终DONE、中继重启/重新认证、
引导冷恢复及Iroh备用后原生恢复的实际证据；资产业务必须另有指定实际AOEM、
签名/账户/nonce/费用/receipt/持久化与重复拒绝。主节点只打开合法已初始化状态，
不借旧钥、降票或偷偷生成另一条夹具链来凑运行。真实进程的必要初始化沿原已批准
共同输入及原owner进行，不自动切换主网服务。本轮不提前发布主网或第三方业务。
抓包仅限本轮明确端口、长度和时间，区分旁观者、中继与最终端；目前尚无新PCAP。

当前本节的READY、源代码、实际网络、实际资产业务和抓包是五层证据。除接收包
核验以外，其余实网场景均保持待执行；没有新TPS、全球自动入网或匿名性结论。

### 12. 2026-11-08 C/D签收、B首启与发现轮换反例修复

**READY已接收，不再重复。** A快进到`bc41986e`后，读取C/D的README、公开回执、
归档检查和原始五文件，逐个核对长度/SHA256，核对actual runtime/node stdout与
report中的对象完全一致。C的`bc41986e`及D的`018d009b`、原字节修正`46fbeac0`
均对应同一个ce1程序/Windows r2归档、77项411022253字节、c4ecbf44接收脚本及
1b764a36实际包内AOEM。C实际LAN192.168.71.125，D为192.168.71.59。
这是包内资格的签收，不是中继已验；没有要求C/D再安装或重复认证。

**首次真正打开原批准状态。** B新增并列程序目录
`/opt/novovm/releases/novonet-ce1f54ea-linux-x86_64`，旧9a和current别名未覆盖。
复用`novovm`实际运行账户、原encrypted owner credential与原validator/transport
用途材料；原共同输入从已核验的Windows批准片段派生，未重选创世时间、账户、
金额、费用或票数。原`native-genesis initialize`先核对完整身份/三pin/持钥批准，
再创建`/var/lib/novovm/selfrun/91129655606029/B/chain-state`，实际初始根为原批准的
f6186d40，原owner排空成功。不能再泛称“四台都尚未建库”。

B原`native_execution_tick`使用这个原库existing启动20秒并正常退出，实际发布了
签名locator，原逐包传输与Iroh备用均由原node驱动，没有另外启动路由器、执行器
或HTTP入口。独立carrier seed由原保护用途owner首次创建，原验证者/传输/资产钥
未替换、未导出。B LAN候选仍标为私有、不能称公网可达；Iroh使用已有NOVOCHAT
记录中的显式`euc1-1.relay.n0.iroh.link`及223.5.5.5 DNS，未放开任意载体回退。

**连续运行确有失败，不以短测通过覆盖。** B随后在02:15:43重开同一原库，约103秒
后报`NOVONET stopped; original node drained: Ok(())`，原因是
`rendezvous monotonic clock moved backwards`，02:17:26退出码1。该新临时unit已
inactive；没有自动重启刷绿，也不删除原库/签票journal/carrier材料。原四个应用
服务仍active；20秒前后还逐项核对过其原PID不变。本次A提权API返回取消、未执行；
用户明确未看到窗口，不能写成用户主动点了取消。不重复启动旧候选。
A/C/D没有创建本轮库或启动；未签收SCM生命周期、四机确认、资产中继或性能。

**已定位到原发现owner，不是系统时钟倒退。** `serve`在循环开头采集Instant，
约100秒后`rotate_if_due`创建新RendezvousStore，构造器记录更晚的Instant。
同一轮`maintain`仍把旧循环时间传给新store，原ClockRegression检查将store置为
终止，后续读取传播为node退出。原store的防倒退检查本身正确，不应关闭。

修正在同一`service/network/discovery.rs`内：本地locator写入、读取分别取得原store
锁后采集实际Instant。循环时间仍负责原调度；100秒轮换、120秒实例、30秒记录、
额度、认证、链pin及共识不变。确定性反例使用原Iroh scope和原store，令调用tick
早于构造1毫秒，连续驱动三次真实轮换、写读及旧RPC store存活；旧实现0.06秒以
同一ClockRegression失败，新实现通过，不靠等待100秒或增大生产期限。

Windows/Linux原network组各8项通过（包括新反例、冷恢复、真实localhost双中继、
并发DONE/重连及错误链拒绝），各约152秒；不是新增四物理机通过。Windows原store
15项含倒退终止/过期/所有权及严格Clippy通过；Linux对应原store15项、同范围严格
Clippy也通过。日志保存在`target/novonet-four-host-ce1f54ea-20261108/clock-*`。
node格式及差异检查通过。完整CI、
新release、B修后跨轮换实测及C/D新包另记，不能继承ce1的旧离线回执。

[B原始首启/失败及对照证据](../artifacts/audit/validator-b-program-install-20261102/novonet-ce1-first-run-20261108.zip)
为16333字节，SHA256 `1d1bba44c6ad08c4d83090cef94a79a8ceb21608b8b505d4d9738813c2706120`。
包含初始化、20秒启动、连续失败、systemd journal、旧失败/新通过本地网络日志及
500包有界头部PCAP；无数据库、签票journal或私钥。完整原库与用途材料仍留在B，
不能称Git ZIP也保全了原库。A准备的旧候选启动脚本留在ignored本地并明确未合格，
未向C/D交付、未提交为可运行入口。

**有限抓包结论。** B eth0限定41050/42050 UDP、snaplen128、最多500包；实际
捕获500包71896字节，PCAP SHA256为
`568608f0df8697c668731167e4f9d51d3a9f5cd61f0fcd7bfafe25e9543a9a10`。
485个样本确见明文NVHCONT1，另15个为其他/短前缀；其中485包是B向三个私有LAN
候选发送，不能算对端收到，另有42050↔7842载体流量。不称四机E2E已抓全、不由
可见前缀推断业务可解密或区域封锁率，也不因发现此故障修改协议指纹/密码学。

当前后续：先固定已本地验证的最小修正源码，再生成**新**候选、在B原库复验跨
轮换运行；不改写/覆盖ce1归档，不让C/D运行已知有反例的旧候选。不另外手动发云CI。

### 13. 2026-11-08 08236129固定包与B原库跨轮换通过，A手动启动

**不再停在修前候选。** 原打包器以干净`main@08236129320da2e71c5fa112d249fa7afc741e78`
分别生成新包；node实际feature含iroh-transport，原AOEM core仍为Windows1b764a36、
Linux d73e4a2f，未替换算法、经济、共识、APFL或AOEM。交付脚本/文档的后继提交
不是另一份程序来源。没有额外手动触发云CI，亦不宣称该SHA云端全量已经通过。

| 制品 | 实际清单与固定SHA256 |
| --- | --- |
| `artifacts/product-overlay/novonet-08236129-windows-x86_64` | 77项411023277字节；manifest `a1a4082dcff2af227738e7e37013ae9a41b2546ecf25474eccda22166cb2f506`；node `9040ac63828c30011e8e7c4801be31183ecb69960854dd4974671df3d5d14d51` |
| `artifacts/product-overlay/novonet-08236129-linux-x86_64` | 80项791242831字节；manifest `ed2705ac1943ed231fd17d6a79e5a13126739474d08049bb1976c2f05f476483`；node `2dbb493ac286fed4ca1bbce879d448027d087d1d3d6ff4d3445b3ca5d70bff29` |
| Windows交付ZIP（程序、原批准公开输入、B签名引导及启动脚本；无私钥/数据库） | `target/novonet-clock-fixed-08236129-20261108/novonet-08236129-windows-x86_64.zip`，150438312字节；`38a871d5dc024ba90461a1c6da451ef2b76a39c3b39c905cf7034c9a920d582a` |
| Linux程序归档 | 同目录`novonet-08236129-linux-x86_64.tar.gz`，358788713字节；`51bfb427697132a56064a186a098516b70b9403da4dc572d947ebd9ecc811932` |

两平台各独立核对清单每文件长度/哈希及索引，实际包内ctl加载包内AOEM通过原
scalar/unified/mixed-batch KAT，实际node的离线existing合同与副作用字段通过。
Windows ZIP还逐条重核全部77个归档流和额外文件集合，公开共同输入仍为原a9b8a240，
不是重建创世或新分配。Linux第一次启动命令找不到pwsh，未执行构建；随后使用既有
`/home/aoem-a1/.local/opt/powershell-7.6.9/pwsh`完成，无重装/改全局环境或覆盖旧包。

**B是真实原库、真实原账户与轮换，不是本机模拟。** 新程序并列在
`/opt/novovm/releases/novonet-08236129-linux-x86_64`，B再次独立核对80项与实际KAT。
只把原运行配置的library指向该新包，database、原genesis/批准、validator/transport
及原carrier路径不变。实际运行账户仍novovm及原encrypted credential，START=existing，
没有调用initialize。原node按240000ms软边界运行，systemd记录02:43:19开始、
02:47:21正常退出；外部观测245.05秒含5秒轮询/收口，不是交易性能计时。

观测到发布序号6–12共7份、三个不同发现实例（两次原100秒轮换），同一carrier key
不变，stderr为空、退出码0。三份受保护用途文件前后哈希相同，原邮箱/Relay/nginx/
Docker的状态与PID完全相同。A独立核对回传ZIP、实例与序号、原服务快照和空stderr。
公开locator投影为3831–3910字节，仍小于原4096上限，不靠扩容取得通过。

本次stdout未提供novonet错误计数；初次附加导出要求该字段而失败，随后明确将其
记录为`counter_present=false / connection_failures=null`，不把缺失当成零。这个导出
缺项不修改原运行报告或替代实际轮换证据，更不能据此宣布真实RPC/四机交付成功。
原SSH等待连接在远端完成后没有返回；A经新连接独立读取成功报告、systemd正常退出，
才结束经路径/命令行核对的**本机SSH等待客户端**，未重跑B或终止node。等待客户端
的本地-1结果保留为控制连接结果，不冒写成SSH成功；后续连接显式设置有界keepalive。

公开证据：

- [B原库实测](../artifacts/audit/validator-b-program-install-20261102/novonet-08236129-B-original-db-proof.zip)，76199字节，`3de15f6cbb35acc50bc42afcd60fc1ff87a50d3815dd73df8977d736728d073b`。
- [两平台离线资格与实际PS5.1检查](../artifacts/audit/validator-b-program-install-20261102/novonet-08236129-offline-qualified.zip)，6225字节，`529b0d3f259934686f8cfe74ecc730f310710b55103b33ea10233e1c19aa1324`；含实际B接收/同库运行脚本供复核，不含credential值、私钥、数据库或journal。

**本段A命令现已执行，结果及C/D后继见第14节，不得再次初始化A。** 当时B已使用同一修正版、同库/钥以原node
软运行上限24小时再次在线（另有24小时+120秒系统兜底；非生产TTL调整、非预约时间窗），
不配置失败自动重启。A在**管理员PowerShell**执行：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "D:\WorksArea\SUPERVM\target\novonet-clock-fixed-08236129-20261108\Start-A-08236129.ps1"
```

该A本地薄入口调用[`scripts/Start-NovonetFixedHost.ps1`](../scripts/Start-NovonetFixedHost.ps1)，
其本次原字节SHA为`5a4ce2d71dcb62ef3f861aae585ce2097d633d2866a89cc404cf4bd517d0da23`。
原Windows PS5.1实测解析、77项固定制品核验、真实node离线输出解析、精确经济整数
和CreateNew拒绝覆盖通过，**但实际LocalService/安装和节点首启仍待这次执行**。

具体副作用不隐藏：并列复制固定新程序；复用本机现有validator/transport材料；
按原批准输入由LocalService执行原initialize（已有库即拒绝，不清理/重建）；
原node existing启动，同一owner接原NOVONET。增加一条限定新node程序、实际LAN、
两端口及LAN/24+B公网来源的Windows入站UDP规则，不开全端口/DMZ或改VPN。创建
仅此一次的LocalService任务，不是替代网络守护程序，也**不是SCM资格已通过**。
原生默认/Iroh合格备用、独立carrier首次保护、显式中继参与沿原owner完成；无
私钥复制或资产账户权限提升。已有work/database/task/同名rule均拒绝覆盖，失败保留。

收集公开结果在`target/novonet-clock-fixed-08236129-20261108/A-start-observed/`；
必须看到实际owner/初始化/locator及任务仍运行，不能把注册任务或Running单独当作
网络/共识/资产通过。任务有原24小时软退出边界，不是启动时改写账户有效期或票数。
紧急结束进程只能按强杀原库恢复记录，不能伪称优雅SCM停止或删除原库/journal。
此阶段C/D暂不运行新任务，A结果核对后再各用同一ZIP/入口一次接线；不重做硬件、
校时、用途钥、已封存配对，也不另加一轮只为READY的人工预约。普通公网自动入网、
多跳、四机资产和持续TPS仍按原范围继续实测，不由B单节点轮换自动签收。

### 14. 2026-11-08 A真实启动已核，C/D各一次接线

**C/D已执行本节启动，原库已创建；不再执行初始化。当前结果及下一操作见第15节。**

A已执行第13节的可见管理员命令。已读取其原始7份输出，而非只看截图：实际SID
S-1-5-19、LocalService账户，原validator/transport材料未重新生成、未访问资产私钥；
initialize实际成功，初始根仍为f6186d40、协议2132270f、创世域f8396bae及原AOEM
1b764a36。locator里的完整传输公钥与原共同配置A的公钥逐字节相等。观察报告任务
Running，267009是运行中状态，不是已完成退出码；当前非提权会话不能取得该任务
对象，但可见novovm-node进程30708，不能将任务查询不可见直接记为进程退出。

这完成的是A本机真实用途材料/原初始化/原节点启动观察，**不是SCM生命周期或
四机通信通过**。A的原库已创建，绝不再跑Start-A初始化脚本。后续需要恢复必须
existing打开原库，不能清理database/journal再调用initialize。

[A原始启动观察及B端对应观察](../artifacts/audit/validator-b-program-install-20261102/novonet-08236129-A-start-observed.zip)
为13179字节，SHA256 `7b319e716105182f92585cf2e32bdfd8ec08235881417bc58998f0251181542d`，
原始UTF-16/UTF-8字节在ZIP中保留；没有用途材料、carrier.protected、数据库或journal。
B进程仍active、stderr为空，首次观察待交付4/拒绝3，归档时待交付5/拒绝仍3、执行
失败0、尚在高度1。待交付是动态队列数，不能据此签收DONE；拒绝3原样保留，尚未
归因，不以“启动成功”抹掉。此轮只有A/B在线，不能降低3票要求制造确认。

**现在C/D各只做一次“新包接线+原初始化+原节点启动”，不另跑READY轮次。**

将第13节的**新082 ZIP**复制给C/D，原ce1 ZIP/安装保留但不用于本轮启动。新ZIP
150438312字节，SHA256 `38a871d5dc024ba90461a1c6da451ef2b76a39c3b39c905cf7034c9a920d582a`。
仅git同步不会取得程序；无需cargo重编或重新准备钥。C/D各机Codex按以下指令执行：

> 同步main并读取本节，保留本机改动，不创建分支或强制覆盖。核对上述ZIP大小/哈希，
> 解压到新目录；如下载来源被标记，仅在核对后解除该ZIP的阻止，不修改全局执行策略。
> 核对包内脚本原字节SHA为`5a4ce2d71dcb62ef3f861aae585ce2097d633d2866a89cc404cf4bd517d0da23`，
> 新程序manifest pin为a1a4082d（完整值见13节），程序源08236129。
> 在管理员PowerShell调用包内同一个Start-NovonetFixedHost.ps1；C填HostId C，D填D。
> PackageRoot/InputsRoot由Codex按实际解压路径解析，不要求用户手填技术参数。此处
> 管理员是本机OS权限，不是NOVONET机构准入。权限不足给用户一条可见的管理员命令，
> 不反复隐式RunAs、不改UAC或全局策略。

```powershell
# $unpacked由本机Codex设为已核验ZIP的实际新解压目录。
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "$unpacked\Start-NovonetFixedHost.ps1" -HostId C -PackageRoot "$unpacked\novonet-08236129-windows-x86_64" -InputsRoot "$unpacked\public-inputs"
# D只将HostId C改为HostId D。
```

> 脚本复用原C/D LocalService用途材料、原共同批准与1亿NOV安排，不拷贝私钥。
> 它并列安装固定程序、由LocalService完成原初始化并existing运行原节点，新增规则
> 只限新node程序、该机实际LAN、41052/42052或41053/42053以及LAN/B来源；其余
> 副作用与第13节相同。若已有work、chain-state、任务或同名规则就停住并报告，不能
> 删除、覆盖或重试initialize；A/B、旧程序、手机和B应用服务不操作。
>
> 任务注册后观察原owner实际生成结果（通常数秒；有界等候，未到条件不能记通过）。
> 只有owner.json的SID为S-1-5-19、initialize.stdout的ok=true/初始根f6186d40、locator
> 已由原node生成、任务仍Running，才能报告START_OBSERVED_082_C或D；Running或
> STARTED字样本身不足。保持任务在线，原24小时边界不是预约短窗口。
>
> 用管理员权限将该机`C:\ProgramData\NOVOVM\selfrun\91129655606029\C或D\novonet-08236129-20261108`
> 下**仅**owner.json、initialize.stdout/stderr、locator.json、progress.json、node.stderr
> 复制到新的公开证据目录；另记录实际task State/LastTaskResult及采集时间。运行中的
> 267009不要当退出码0。将这些原字节打ZIP并回传main，可分别使用
> `artifacts/audit/validator-c-novonet-082-start-20261108/`、D对应目录。未生成文件或出现
> 错误如实记录，原库和完整现场保留，不清理、不重跑；不要把任何protected文件、
> 资产钥、数据库或journal塞进公开ZIP。不重复硬件、校时、密钥生成或旧配对。

这份新ZIP及脚本与A已执行的是同一字节组合，不改程序或增加网络框架。目前部署
只给出既有B引导来源，不能称两个独立引导均已实机签收；第二来源、冷恢复和真实
单/双中继仍在后续实际网络验收中按原owner完成。C/D实际启动后再统一看四端记录、
路径与对象DONE，随后用原账户/库做业务与最终回执，不在此阶段宣称TPS或开放L1。

### 15. 2026-11-08 C/D启动签收，原客户端查询与A本机防火墙阻断

**后继实测覆盖本节的防火墙待执行命令：用户执行时该条规则已经为Allow，原脚本
在任何修改前拒绝，未改变防火墙。A重新逐项读回action=Allow、Inbound、Public、UDP、
Enabled=True及同一程序路径。脚本现明确识别既有Allow并无修改退出，Windows PS5
真实执行该分支退出0，未创建操作目录。无需用户再执行防火墙脚本。**

随后在已有Allow下，原client沿相同独立用途钥、相同域和原30秒期限，从A固定UDP41190
依次查询B/C/D，三项仍退出1、`native query deadline; no fresh node observation`。
因此先前Block观察是真实历史值，但不能据此归因全部超时，规则变Allow也不是网络
修复通过。B同期SSH绑定A LAN仍连接超时；未执行远端tcpdump。没有重启/清库/换钥，
下一诊断必须取得实际包收发或当前对端处理证据，不盲目重跑、不提高超时。下文
修正规则的命令仅留作本次操作历史，不再作为用户待办。

A已ff到C回执`d84bf08f`（含D `2185d5bb`），独立核对两份原ZIP的CRC、每份公开
原件长度/哈希、实际LocalService SID、共同初始根与完整传输公钥。C在03:38、D在
03:30观察到原node Running；四台初始化根均为f6186d40。新程序仍是08236129，
不是文档回执提交。C/D不重复启动、换钥、清库或六组旧认证。

A随后用固定包内原native-client、同一组批准域/验证者及独立OS保护传输用途钥，
依次执行只读status。该客户端钥不同于全部签票/传输/资产钥，不取得投票或花费权。
A退出0，返回height=0/current_height=1、原初始根、projection_failed=false；这只是
认证节点观察，不是独立最终性证明。B/C/D各退出1，原30秒期限内未取得新观察。
不得把启动通过写成四机通信、DONE、连续确认、中继或资产通过。

已读到A当前WLAN/Meta均Public，系统存在针对**这份native-client.exe**的显式
Public/Inbound/UDP Block规则，TCP另有Block。它是远端回复的具体阻断，尚未通过
修正前后对照证明全部超时都由它造成；不据此认定C/D或原网络算法故障。B端本轮
SSH直接绑定LAN两次连接超时，默认路径一次banner超时；限时tcpdump未启动。
固定41190端口的额外B查询也超时并单列保留，不称已抓到包或拒绝认证。

下一步仅A在管理员PowerShell执行一次：

```powershell
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "D:\WorksArea\SUPERVM\scripts\Enable-NovonetOperatorQuery.ps1"
```

脚本核固定client哈希及实际A地址，仅停用已核对的该条UDP Block，保留TCP阻止，
新增同程序/Public/A地址/UDP41190、仅B/C/D地址及41050/41052/41053的回复Allow。
不会关闭全局防火墙、修改node规则、初始化/重启节点或读取资产钥。原规则和操作
结果写入新的target/operator-firewall目录；已存在则拒绝重跑，新增失败恢复原Block。
当前只通过Windows PowerShell5/PowerShell7语法解析；**尚未提权执行，不记实测通过**。
完成后A使用原client固定绑定192.168.71.74:41190进行对照，不增加超时或更换身份。
C/D此刻不需操作；B SSH可用后才进行原端口限时抓包，不改全局VPN/路由或B服务。

[公开原件ZIP](../artifacts/audit/validator-b-program-install-20261102/novonet-082-four-starts-query-observation.zip)
为10345字节，SHA256 `a073b19d4152655832b67e207c3889d0fb0c251827bfae79c1a387d5f429c331`，
15项含原查询输出、失败、C/D核验及A规则观察；独立读回CRC和每项字节通过。
不含client protected、配置私密材料、数据库或journal；未转账、收费、重启或产生TPS。

### 16. 2026-11-08 掉线后查询恢复：SSH账户核对与收发时间线

用户确认刚才发生掉线。恢复后仓库原SSH key通过严格host校验认证，GitHub明确返回
`Hi xonovo!`，不是账户丢失或改用别人的身份；未修改core.sshCommand/远端、未导出
token、未接受未知443主机指纹。原22端口SSH和B连接恢复，远端仍d84bf08f，本地提交
保留，无重写。前两节防火墙修正命令不再执行，规则已Allow，节点/钥/库/超时未改。

此前真实方向是**A原客户端查询B/C/D**，不是B/C/D查询A。恢复后原客户端三次status
均退出0：B 65ms、C 41ms、D 33ms（原operator报告范围，不是交易延迟或TPS）。三者
均返回同一f6186d40根、height=0/current_height=1、projection_failed=false；独立最终性
标志均false。A此前本机查询也成功。没有为了恢复而重启、初始化、换钥或发资产交易。

随后只在B eth0、原UDP41050及本次已观察家庭出口范围限时12秒抓包，最多5000包、
每包最多1600字节，不抓邮箱/Web等其他端口。实际834包，tcpdump报告886包过过滤器、
内核丢包0（计数口径不混同）。该抓包与一次原client查询并发，不是先结束抓包后才查询。
初次300包预采集先于查询结束，不能当相关查询抓包；后继仅以本次并发原件作关联。

| 北京时间 | 原始证据所支持的阶段 |
| --- | --- |
| 04:57:42.745–04:57:53.949 | B网卡有界抓包覆盖窗口 |
| 04:57:44.196579 | A启动同一client、绑定192.168.71.74:41190，查询47.103.59.206:41050 |
| 04:57:44.399093–04:57:44.407339 | B实际收到114.92.30.222:61385到172.16.0.48:41050的完整原握手Offer分片；重组SHA256匹配，公钥逐字节匹配独立operator公开钥 |
| 同一抓包窗口 | B向该实际来源61385发送UDP；相关双向流为入11包/出200包，含查询完成后的原重试，不全算该次应用请求 |
| 04:57:44.410737 | A原client退出0，收到绑定预期节点/域的Status，根匹配；无projection_failed，不是仅UDP ACK或无回复推断 |

61385只是**这个客户端socket**的本次公网映射，不能写回A验证者41051或当作节点
实际映射。原PCAP的SHA256为e603715d79420976733f2324ca0c81d97425f6ccf47ab3cc8d8588128d65c2b6，
SSH远端与本地一致。离线脚本只解码以太网/IP/UDP及原公开握手分片，验证重组digest
和公开钥，不解密业务，也不把digest当独立签名验证；实际认证/AEAD/响应由原SDK验证。

本次成功证明A到B/C/D及响应回A的查询路径已恢复，未重现旧超时；未采到掉线时的
包，不能独立判定历史停在哪一跳，或把所有失败归给VPN/旧Block。用户掉线说明
作为关联事件保留，不据此销账全部问题。B同期原公开progress仍保留拒绝96、过期
发送105、连接失败164、pending0、bound routes1；这些累计数不归零，也不能直接等同
96个应用认证拒绝或164条当前故障。A原node PID30708监听实际LAN41051/42051；本会话
无权直接读取其受保护progress，拒绝访问不是文件消失或进程退出的证明。

[恢复公开证据ZIP](../artifacts/audit/validator-b-program-install-20261102/novonet-082-query-recovered-20261108.zip)
173634字节，SHA256 `1732af63afc2850359770efc4ccc51abc9d64a2470519a7744b05f90060be362`，
23项含旧Allow下失败、本次成功、相关PCAP/时间线/公开key投影/分析脚本及B公开progress，
读回CRC/逐原件字节通过；不含protected、token、数据库或journal。程序仍08236129。
下一步沿原节点检查发现、对等对象交付和原计划中继；不能将可查询写成六条验证者
连接、双中继、连续最终确认、自动公众入网、SCM或性能已经通过。C/D不重复初始化或
旧配对；若需本机当前只读证据，再给一次具体采集任务，不改本次既有运行配置。

### 17. 2026-11-08 当前实例被动核对：发现与对象DONE证据缺口

本轮按用户范围只读检查，不重启/初始化/换钥/改配置/防火墙，不重复A到B/C/D Status，
不改NOVOCHAT、签名或资源上限，也不启新socket冒充验证者owner。程序仍08236129。
A实际LAN41051/42051仍由PID30708持有；B原node PID1661875、novovm账户、02:52:27
启动的原unit仍active，41050/42050由同一PID持有。本会话不能读A受保护progress，
C/D当前运行内部状态没有新回传；旧启动快照不当作当前候选池或对象事件。

B两份指定公开文件快照在北京时间05:25:28.725578及05:26:09.875293取得，均由原
文件原字节base64保存在证据中；只读配置投影，不取protected、credential、DB或journal。

| 原指标 | 首份→末份 | 这41.15秒能证明的范围 |
| --- | --- | --- |
| native_rejected_messages | 96→96 | 窗口内无新增；不能分类全部历史拒绝或宣布永久不再发生 |
| native_expired_sends | 105→105 | 窗口内无新增；不清零旧数 |
| novonet.connection_failures | 164→164 | 窗口内无新增；不等于164个当前故障 |
| native_pending_deliveries | 2→0 | 队列变化，不能确定哪份Object完成或冒充DONE |
| locator sequence | 287→289 | B仍实际发布，末份issued/expires覆盖采集时刻；完整公钥匹配批准B |
| height / executed / execution failures | 1/0/0→1/0/0 | 未执行资产业务，不代表最终性验证 |

更正本轮中途口头更新的首序号288：原件和离线比较实际为**287→289**。不覆盖原件。
B stderr0，bound routes保持1；不能由这个总数知道具体哪位对端，也不能当原生UDP
会话计数。B discovery=true、relay_forwarding=true，seeds/peer_records为空是当前
发现服务端角色的实际配置，不据此认定它没有收到A/C/D发布。其RendezvousStore和
candidate/pool在内存，现行observation只公开discovery开关、总bound数和总失败数，
没有逐身份发布/获取/入池结果；B自己的locator不是其他三台已进池的证据。
discovery-cache是引导检查点，不是完整已发现邻居表；不能拿缺少它推断无发现。

源码已核到实际路径：`service/network/discovery.rs`的原认证NODE_PUT/NODE_GET和
`maintain/poll`产出记录，`service/network.rs`校验record_kind、配置公钥、签名/期限及
ServiceLeaseBook后，才upsert_verified_record并更新原candidate/routes。这是调用链
核对，不是四台实机候选逐项已通过。

**实际对象验收在当前约束下尚不能执行或签收，具体缺项不是另造网络：**

1. 当前固定包的native-client operator只开放status/query/submit；query是只读业务查询，
   submit只接受合法签名资产批次。没有让**运行中的A原NativeLifecycle**向指定C发一个
   非业务测试对象的外部控制入口。NativeEndpoint/ObjectLink组件能力存在，不等于这项
   实机触发/观测已接通。另起客户端/测试进程不能冒充A验证者，提前提交资产也违反本轮
   “资产最后”顺序，因此都未采用。
2. 原profile已有`delivery_event`收发/fragment记录，但B实际stage_profile显示
   native_delivery.enabled=false且events为空，native_delivery_traffic也未提供。开关
   `NOVOVM_PROFILE_NATIVE_DELIVERY`由原进程OnceLock读取，不能从外面修改父进程环境
   热开启。不注入进程、不因缺记录而擅自重启。当前无逐对象ObjectID/SessionID/DONE
   可读取记录，队列归零和没有新增过期不足以补成这些事件。
3. 原合同需保持三层：包ACK、对象完整接管的DONE/Delivered、业务接纳/回执/最终性。
   native_lifecycle明确DONE是bounded custody，不是签名准入；operator submit也明确
   transport_delivery_only。不能把对象DONE重新定义成资产执行成功，也不新增一个同名
   “Application DONE”来改变已有完成合同。后续必须分别核原对象完成与实际业务结果。

阶段一仅完成A/B实际监听、B公开发现材料/计数窗口及调用关系审查；四机逐身份入池
未验收，A→C指定对象和DONE未发送/未验收，单/双中继与重启恢复均未执行。当前限制
明确禁止重启和配置变化，所以不自行更新运行包或开诊断。若用户后继批准受控更新，
只应在原owner接合现有逐对象观察及既有只读控制消息的有界触发，固定影响范围后再
按原库existing执行；不新增测试网络/任意内容服务/身份表，也不降低票数或扩大预算。
本轮只提出该实际接线缺项，不表示上述修改或重启已经获准。

[被动核对原件ZIP](../artifacts/audit/validator-b-program-install-20261102/novonet-082-passive-object-audit-20261108.zip)
为52133字节，SHA256 `1b4788393c64a7006886f88253c20737cab67a1e5ea885996e9fae9fcf1c688b`，
9项含两次原字节公开文件/有界配置投影、实际监听及原比较脚本，读回CRC/逐项字节通过。
没有改产品代码、跑组件替身、发Status或资产请求；不触发手工完整CI。C/D无重复操作。

### 18. 2026-11-08 获批原owner最小增强：候选开发，不自动更新节点

用户后继明确批准第17节缺项的代码/候选开发，不授权重启、安装或更新现有A/B/C/D。
本节不是重跑四机实网；运行程序仍08236129，原库、用途、网络配置及旧失败均保留。

#### 实际接线与固定边界

- 原`transport::Message`新增显式可拒绝的tag21 `NetworkProbe`，走**原控制lane**。
  它只有nonce、发送/接收validator ID、issued/expires和固定256字节确定性图样；
  整条原Host对象379字节，命令377字节。无任意payload、资产、聊天、执行或签票内容。
  Object ID由原fragment domain及全部规范字节产生，nonce只标记诊断，不是新身份。
  旧082不支持此可选tag，不能拿旧接收端签收新测试对象；没有旧算法或WSS回退。
- `NativeLifecycle::network_probe_command`核本地原validator、原配置对端和有效期。
  每节点最多1个出站、每秒最多1次新发，最长10秒；接收每节点每秒最多4个，
  活跃期去重/观察各最多64项。命令占**原deliveries及原lane额度**，不增加连接、
  发送/重组/转发/字节预算；普通控制消息先于未准入诊断，诊断不能拿走控制保留。
- 明确opt-in：现有NOVONET配置增加可选`probe_control_file`（默认无）。原`Driver.step`
  在**现有reactor**中每秒最多进展一次有界异步普通文件读取，最多一个读future；
  只解码固定本地二进制命令，记录同内容摘要后不重复执行。畸形/过大/重复/错身份
  只拒绝诊断，不停止原node。不新增socket、端口、HTTP/IPC服务、守护进程或网络队列。
  文件不能与locator、引导检查点、peer文件或carrier用途文件相同。
- 现有`novovmctl native-network-probe prepare/cancel`仅发布本地`.nvprobe`命令。
  无socket/私钥/节点启动；拒绝覆盖非诊断文件，原命令经有界解析后才可原子替换。
  `sender/recipient`是原validator ID，**不是资产账户、UCA或carrier公钥**；由A依据
  原批准公开材料派生，不让用户手填/另建表。CLI返回`sent=false/object_done=false`。
- 取消未准入对象直接归还其原队列占用；已准入则保留原sender及原期限排空，避免
  丢弃共享carrier半帧。可能记录`probe_cancelled_drained_object_done`，明确远端仍可能
  已接管；不承诺取消能撤回远端字节。会话替换只在剩余原期限内重发相同对象，
  不续TTL；过期、取消、会话替换分开记录，不伪报DONE。

调用链：本地ctl固定二进制文件 → 原NOVONET Driver有界读取 → 原NativeLifecycle
身份/资源准入 → 原NativeChannel.prepare → 原NativeEndpoint/NativeObjectLink/
NovoRUDP（或原合格备用/逐跳载体） → 原对象完整性/域/身份校验 → 诊断接收者核对
及有界去重 → 原对象DONE。没有进入NOV资产、聊天、AOEM执行或账本/签票journal。

#### 观察与完成语义

保留原`delivery_event`，可由获批的显式启动配置启用，原环境变量OnceLock行为不
假装能进程外热改。默认关闭；启动配置未启用时不读命令文件、不安装新观察器。
关闭通过后继受控启动去掉该配置/原环境开关完成，不在本轮热修改运行实例。

原delivery环2048条与原NativeTransferDiagnostics的256对象/120秒上限不增加。
写入改为try_lock，竞争时丢**诊断观察**并显式累计，不等待、不丢网络数据，不把缺
记录补成成功；没有同步磁盘日志。原状态报告投影新增`native_network_probe`，64条
上下文包含Object ID、nonce、声明两端与实际peer、Session ID、issued/expires/观察时刻、
原路由种类及实际UDP地址或已认证carrier首跳公钥/连接binding摘要。不能把即时首跳
说成全部中继证明，不能输出原始exporter、会话秘密或protected材料。会话替换后的
终态保留实际原发送会话/路径，而不是拿当前新session冒充旧发送的完成。

底层原观察保留分片、hash完成、feedback窗口、ACK进展、TransportDone和未归属拒绝
分类；通过原`transfer_tag(session, stream, object)`与同对象上下文关联。
Packet/fragment ACK、Object DONE（完整有界接管）、业务接纳/执行/持久化Receipt
仍是三层。错误接收者/关闭诊断的节点可能已在对象层接管后拒绝诊断，必须同时核
接收端`probe_received_integrity_verified`和发送端真实DONE；单方DONE不算本诊断签收，
更不算资产成功。低层记录缺失/竞争/淘汰有覆盖计数，不用百分比或队列归零补齐证据。

#### 本地检查与失败（构建前收口记录）

Windows/Linux原生命周期各3项真实AOEM store/native组件回归已通过：跨节点并发
对象DONE及相同Session、重复/错接收者隔离、默认关闭、取消/过期、准入后排空和
接收限额/观察淘汰。使用原本地夹具及显式标记的ML-DSA-shaped组件签名provider，**不是四机实网或新ML-DSA
制品已部署**。Windows另有原三高度业务默认关闭诊断回归1项通过，7项原底层观察
回归通过（含竞争不阻塞、保留/淘汰、ACK与完成分层）；本地命令实际异步读取1项、
CLI安全发布两平台各1项通过。codec/命令截断、图样篡改、任意输入拒绝已覆盖。

原中继IO回归保留原普通对象，追加固定Probe对象；Windows原批次6项通过、新单中继
首版失败，恢复原61秒滚动资源窗口分隔后单例81.19秒通过。原双中继含并发和中继
断开重认证已通过本地IO；Linux该网络组7/7通过（233.23秒）。不得把首个6/7改成7/7或把这些
localhost真实IO组件试验写成四台物理机器通过。失败摘录/完整后继日志随候选归档。
CLI all-target检查还发现原genesis夹具遗漏已有novonet_config_file，显式补None；
没有弱化启动合同。两平台Node的lib/tests及CLI all-target严格Clippy通过；最终
生命周期定向复验Windows 3/3（0.84秒）、Linux 3/3（1.76秒）。源码/制品身份另固定。

#### 待批准的A/B原库existing更新与回滚（本轮不执行）

1. 先冻结源码、两平台各自manifest/node/ctl/AOEM/hash及必要本地结果。两平台语义
   相同但二进制hash不同；不覆盖现有release，不下载未知库，不改变共同域/三pin、
   四验证者/3票、七用途余额/控制或费用。当前不存在已批准的新部署操作。
2. A原任务`NOVOVM-A-08236129-Network-20261108`的动作**包含initialize**，绝不可
   重启该动作来更新。新受控启动必须直接用原node模式和START=existing，复用原
   LocalService custody、原A数据库/journal和carrier；不注册第二套网络或附带SCM迁移。
   A原库`C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state`不清理/重建。
3. B只更新独立自营unit和`/var/lib/novovm/selfrun/91129655606029/B/chain-state`的
   原owner程序配置，继续novovm账户/原受保护credential、用途及carrier。不动邮箱、
   Relay、Web、Docker。新旧节点不同时打开同库/同端口，不删除LOCK绕过占用。
4. 当前A任务/B临时unit不是“已验SCM优雅停止”。可等待原24h软运行自然排空；若要
   提前更新，需明确批准精确终止目标进程的副作用，按**中断后原库恢复**验证，不能
   把TaskStop/systemctl stop未经验证就称为已优雅排空。不在此文执行任何停止命令。
5. 停止/退出后原目录、journal及旧日志原地保留；不拿运行中的原库直接文件复制当
   一致备份。启动新候选前比较全部身份/域/库路径，独立工作目录及新日志CreateNew。
   诊断命令目录仅管理员/SYSTEM可写，原服务账户只读；报告沿原工作目录权限写入。
   不把七把资产钥交给服务账户，不发布protected、credential或数据库。
6. 后继配置只显式增加已审命令路径；原native优先、Iroh备用、发现/中继策略和额度
   不变。回滚先停止新实例并保全其现场，再用**旧082程序+旧配置**existing重开同库；
   旧082拒绝probe_control_file/tag21，不可拿新配置强喂旧程序。无DB/journal格式变更
   或迁移；若原Existing检查拒绝，停止并报告，不清库、不强开、不重创世。
7. A/B更新不自动签收A→C：C仍082时会拒绝新可选tag。A→C正式对象验收前，必须
   另行批准C使用同候选existing更新并启用同一原owner观察；不是再做初始化/钥准备。
   D同理，只在获准的单/双中继阶段需要时一次安排，不重复已封存认证或机器清点。
8. 更新计划的副作用：一次目标节点短暂离线、原会话重新认证、发现序号推进、诊断
   消耗原配额及少量有界内存/公开元数据；不收测试对象的NOV费，不产生Business Receipt。
   回滚丢弃的是进程内未完成诊断，结果保持未知/取消，不替换或回退资产历史。

#### 本轮固定候选（已构建、未更新）

源码固定`ddd89e8e05721a2ddb8a7ae9255405d395b39a46`，完整diff相对`adf3b64e`，
SHA256 `d4eac1425009973fca8777e7db92d7c611ec5c8dbeabcf8f737e893cca7ef66e`。
原`novovm-package-product-linux.ps1`在干净源码上分别构建十个原角色程序；
node真实包含`iroh-transport`，未改变默认UDP优先或引入额外服务。
源码提交后的本节制品登记属于文档/证据，不是后继源码已重建。

包目录：`artifacts/product-overlay/novonet-probe-ddd89e8e-{windows,linux}-x86_64`。
两平台原包内AOEM沿用082固定库，不修改正式SDK；独立再次核对全部文件、checksum
索引、实际加载路径/哈希、scalar/unified/mixed-batch ML-DSA验证资格及原节点服务
合同。Windows 77项411410349字节，Linux 80项792043567字节，均通过。
实际包内ctl的prepare/cancel冒烟各通过，377字节固定文件，返回sent/object_done/
business_receipt/node_started均false；没有读取原库、解封用途或启动服务。

| 固定内容 SHA256 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| release-manifest.json | `badaf819c3a8cc03a0e4e200d45f9f37a15c1c34937b3482b3940b6491c83d1f` | `f9845e3bdaef188dc7c40db55f09de6bc75f37cb4cfbb3a3d02939af9aa4802e` |
| novovm-node | `19883c8b95fc059ec7838ff0556284b1ddd434b7a72bfbe93e3f3410f458a0fc` | `603ab7f063274b03e1d81599a57fcb1765b626c04da6d9aef0df963aa24d39cb` |
| novovm-native-client | `ec8cff28ef0065bdca9e0865c8ee63bcd038df4571bef30834c0be312ab45553` | `bd0354874d9e87d0c9950d26889cc008713a04ebf841bc9a97d7f19f27056fb0` |
| novovmctl | `690556730d3dea8e5664a4b7d87db77c5bd073175a3f1a1635aa4576ce070f02` | `12ec773f4bbb9a8e82b51c08c5675b9868b0e682e704e14741a344e66a385006` |
| 实际AOEM core | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |

分发归档在A本地`target/novonet-probe-candidate-20261108/`，不混入源码Git：

- `novonet-probe-ddd89e8e-windows-x86_64.zip`，150476966字节，SHA256
  `9841a060ab37dc6fbca174ee91048f3e3e54d106957d6e9e0781e7fe44a9115e`。
- `novonet-probe-ddd89e8e-linux-x86_64.tar.gz`，359196043字节，SHA256
  `38f520b3110691cafdb690ff46cb56578c1a7d2682b98ae688b0c5e6dcc03a94`。

本地最终network diagnostics两平台各7/7；Node lib/tests、Network lib/tests和
ctl all-targets严格Clippy两平台通过，fmt/diff检查通过。原首错摘录、后继完整
构建/定向日志、独立制品核验、完整源码diff随本节公开证据归档；没有protected、
资产钥、carrier钥、数据库或原签票journal。未保留完整输出的前序网络组仅标摘要，
不冒充完整原始stderr。此交付不包含实际四机对象、全局发现、资产Receipt、TPS或
新云端CI通过结论。公开原件包：[27份代码差异/日志/制品核验](../artifacts/audit/validator-b-program-install-20261102/novonet-probe-ddd89e8e-evidence.zip)，
65644字节，SHA256 `d0fd5820120db1bfd50caef818bad57ab49dd2ae5fc269ac1595bd7d7b0947cc`。
Windows分发ZIP另读回77项压缩成员hash通过，Linux归档gzip CRC通过；两者不混称
全部重新解包验证。当前节点仍082，**上述existing更新方案须另获批准才执行**。
不额外手工触发云端完整CI，main自动检查不关闭。

### 19. 2026-11-08 获批A/B原库更新已执行，C/D保持082

用户后继明确批准固定ddd89e8e候选、先B后A、必要时精确终止旧node进程及
START=existing原库恢复；随后另批A同范围新程序UDP规则和恢复三类防火墙启用。
本节更新第18节“未部署”的时间状态，不改变其对象/业务完成边界。
交付提交252f7249对应自动CI42423130454 completed/success已只读确认，未重跑。

| 项目 | A Windows | B Ubuntu |
| --- | --- | --- |
| 旧 → 新PID | 30708 → 24444 | 1661875 → 1816591 |
| 实际账户 | 原LocalService / S-1-5-19 | 原novovm / UID996 |
| 新程序 | `C:\ProgramData\NOVOVM\releases\novonet-probe-ddd89e8e-windows-x86_64\bin\novovm-node.exe` | `/opt/novovm/releases/novonet-probe-ddd89e8e-linux-x86_64/bin/novovm-node` |
| 实际node SHA256 | `19883c8b95fc059ec7838ff0556284b1ddd434b7a72bfbe93e3f3410f458a0fc` | `603ab7f063274b03e1d81599a57fcb1765b626c04da6d9aef0df963aa24d39cb` |
| 实际AOEM SHA256 | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` | `d73e4a2f1ec42f75e7b2356c10779330b1dc902113d290275042d3bf8a06bc33` |
| 原库 | `C:\ProgramData\NOVOVM\selfrun\91129655606029\A\chain-state` | `/var/lib/novovm/selfrun/91129655606029/B/chain-state` |
| 启动 | 新existing-only原owner任务；旧含initialize任务Ready、未重跑 | 原credential/账户的新独立unit，直接existing |
| 原监听 | 192.168.71.74:41051 / 42051 | 0.0.0.0:41050 / 42050 |

各机安装目录逐文件哈希复验；B包内实际库KAT通过，A新进程实际加载模块哈希已核。
旧进程归属先核再精确结束，退出记录保留；不以终止成功声称完整优雅排空。没有
initialize、清LOCK、改账本/journal、重新生成用途材料或运行中数据库复制。原
protected材料比对不变。B既有novonet-mailbox/supervm-relay/nginx/docker状态/PID
严格不变；未切换其他应用或正式SDK。仍沿原24h有界运行，不冒充永久SCM验收。

更新前/后及最后A/B认证Status的状态根均为
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`，
height=0/current_height=1，recovering/projection_failed=false；最终查询27/60ms。
这不是连续确认、资产业务或性能结论。B公开身份不变，locator703→719→724且新鲜；
A当时最新locator直接读回受普通token ACL限制，未把“不可读”当“缺失”。后继
第20节已只读补齐locator与接收事件。A初始发现/诊断、监听及对象接管分别留证。

诊断显式配置已启用，但启动时commands/events均0，不自动发对象。用户取消了
A→B命令的UAC，该命令未写入，未反复重弹、未改为成功。随后只通过已批准B
root通道向原owner发布**一条B→A固定非业务命令**，不绕过A本地文件权限：

- Object ID `36c112d46a0b31fdce33ccd3e8b5270d2afda6349975912ff767629aa25928dd`。
- Session ID `668e6e9ccd7ecd5ce873bbf2c951e658`。
- 原事件`probe_owned → probe_admitted → probe_object_done`，路由`native_udp`；
  本会话实际目标`114.92.30.222:61758`是动态NAT坐标，不写成固定公网配置。
- 按原`transfer_tag(session,NVBFT001,object)`关联实际反馈，acknowledged_best=1、
  feedback_packets=2、send_terminal_state=delivered及TransportDone存在。
- **该次汇总只有B发送端DONE，A接收侧观察当时未读回**；后继第20节补齐同一
  对象的双端匹配。仍不代替A→C、四机、多跳或Business Receipt，无资产交易/TPS。

保留首错：A切换前严格规则核验拒绝了Windows将/24规范显示为/255.255.255.0的
差异，当时未停旧节点；取得原属性并确认完全等价范围后恢复准备态继续，不放宽
端口/地址。新规则只绑定新程序，旧规则保留；随后Domain/Private/Public全启用，
之后认证Status及B→A对象DONE通过，不依赖全局关闭防火墙签收。

B新进程初期1884次unknown_source拒绝，后两个快照1884→1884、连接失败1→1，
其他认证等拒绝分类0、stderr为空；具体来源尚未归因，不能凭停止增长销账。
旧epoch的96拒绝/105过期/207连接失败及原日志原样保留，不把进程重启后的新计数
说成清除了旧故障。没有发生回滚；辅助脚本具明确旧082+旧config+existing回滚
分支，但未以未执行分支冒充回滚实测。

[A/B更新公开证据29份](../artifacts/audit/validator-b-program-install-20261102/novonet-probe-ddd89e8e-AB-update-evidence.zip)，
67510字节，SHA256 `bb97f8aceda13cbd4b51371c663838a295e1c8c87c48b81885920722057ca7cb`。
含原PID退出、实际程序/库、状态查询、其他服务、固定对象/ACK/DONE和辅助脚本；
不含protected/credential/资产私钥、数据库或journal。源程序仍ddd89e8e，记录提交
不等于另一版程序。C/D未更新、未启动或清点；下一步仅申请C同版本原库existing
更新及A→C双端逐对象证据，不自动授权C/D或四机资产业务。

### 20. 2026-11-08 A接收记录只读补齐：同一B→A对象双端匹配

前次缺的是取证收尾，不是已证明的接收缺陷。普通token不能读LocalService目录，
Agent未另完成只读导出便留下待项；取消的UAC实际是A→B发命令，不能用它解释
A历史接收记录不存在。本次用户要求处理后，一次只读UAC导出原记录，未改网络代码、
ACL或运行配置，未重启/重发、未访问用途私钥/数据库。原PID24444/LocalService和
程序哈希19883c8b…均一致；原27份/29份历史证据不改写，其false字段按当时范围使用。

同一历史对象`36c112d46a0b31fdce33ccd3e8b5270d2afda6349975912ff767629aa25928dd`，
同一Session `668e6e9ccd7ecd5ce873bbf2c951e658`：A保留**1条**
`probe_received_integrity_verified`，与B原`probe_object_done`的Object ID、Session ID、
发送/接收validator ID逐字节匹配；A实际认证peer也等于B发送方。两端实际native_udp，
B目标114.92.30.222:61758、A来源47.103.59.206:41050，仅为本次动态会话坐标。
A Probe上下文evicted=0，本地accepted_commands=0；补证没有另发对象或伪造成功。

B先前保存的低层反馈按原transfer_tag关联ACK及DONE继续有效；A低层120秒留存窗
可能已过，不声称本次重新取得其全部分片日志。各机wall clock不同，不将壁钟相减
当网络延迟或完整因果时序。现可登记**这一条B→A真实对象双端有界接管证据匹配**，
不是Business Receipt、资产执行/最终性、A→C、四机、多跳或TPS。

同时直接读回A原签名locator sequence709，在导出时新鲜；原发现RPC启用、已安装
bound peer routes2、connection_failures0。global_auto_discovery及native_multihop_accepted
仍false，不靠本次补证提升未验收范围。无需为读取报告放宽受保护目录权限；以后复用
本次最小只读导出入口，不把采集权限缺口再当网络故障或新增架构任务。

[8份接收补证原件](../artifacts/audit/validator-b-program-install-20261102/novonet-probe-ddd89e8e-A-receive-evidence.zip)，
57461字节，SHA256 `65251131756ca8d13d2c78642532be5b6da11f5952fa02646f4bcd2bafa65130`。
含原progress/公开locator、双端匹配结果、原B反馈及只读导出脚本；不含protected、
credential、数据库/journal或资产钥。运行程序仍ddd89e8e，C/D保持082未操作。
后继仍是获准C更新后做A→C双端验收，不以本条B→A替代。

### 21. 2026-11-08 C原库更新及A→C单对象双端接管匹配通过

用户批准C脚本修复、定向验证、existing更新及A→C对象验收。原路径适配错误、
退出枚举错误和082回滚均保留在各自公开目录。修复4beeea61持有精确进程句柄等待退出，
再有界等待两次空节点枚举；PowerShell7/5.1各5项隔离定向检查通过。
复核上轮prepared目录、原配置/材料和同范围规则后，C实际19752→23024，沿原
LocalService及chain-state、START=existing运行固定ddd89e8e，原三份用途材料摘要一致。
实际node19883c8b、AOEM1b764a36、41052/42052及完整locator传输公钥均匹配。
本轮初始化执行字段为false，旧证据、原库及journal原地保留。更新公开提交11e03a08。

A原运行owner仅发布一条固定非业务对象，发送公开提交cc6d5a08；C有界只读采集
保存原接收及底层反馈，接收公开提交f7d5ec6d。独立核对双方原始文件及摘要后，
Object `8ed4aa5273e61b563f402fe9490aaa0690c020bfd840bb888744e80392740343`、
Session `1d1181eae8f4d2117ff43dd624f4fafe`、nonce、sender/recipient及carrier binding一致。
A原阶段owned→admitted→object_done，底层ACK best=1、delivered及TransportDone存在；
C原事件received_integrity_verified、1/1片及hash完成、原反馈3次且missing=0，
实际peer身份互相匹配，双方目标transfer tag唯一对应。

实际路径是authenticated_bound_carrier。此次登记A→C该固定对象双端有界接管通过，
原生UDP直连、多跳、四机拓扑、业务Receipt和共识最终性仍分别验收。
原始wall/monotonic按所属机器保留，跨机壁钟不用于性能计算。
完整逐项时间线、覆盖计数和原始证据链接见
[双端独立匹配报告](../artifacts/audit/validator-a-c-ddd-object-match-20261108/README.md)。
C继续当前节点在线；D及NOVOCHAT处于本轮操作范围之外。

### 22. 2026-11-08 原owner有界UDP恢复观察；11-09 A/C更新及原生UDP发送证据

承接A/C短报头e58b33cb和原owner审计bec85de4，本次仅补充原网络观察器，
不修改恢复决策、身份/角色选举、握手/AEAD、资源额度、重试或超时。
NativeObjectLink.set_diagnostics_v1将**同一个**原观察器交给NativeEndpoint，
仍由已有probe opt-in或原profile入口开启；默认无观察器，不新增环境变量、
socket、任务、RPC、协议或业务JSON转换。NOVOCHAT与运行节点未动。

新数据沿原progress输出：`native_network_probe.transport.udp_recovery`，
以及`udp_recovery_event_names`、`udp_recovery_omitted`。
peer为原配置peer索引，不能当作validator全局编号或新成员表。
每个原观察器最多64个peer记录、22个饱和计数及最后阶段/发送时刻、
实际send错误的OS错误号和本地active Session ID。没有载荷、公钥或私钥。
沿用try_lock和原contended_observations，导出竞争不等待，遗漏明确计数；
不保留无界事件历史，不受256对象记录淘汰影响，不改原对象DONE语义。

状态包括request、passive_role、pending_reconnect、active_attempt、attempt_started；
retry_wait、no_transcript、attempt_deadline、signed_expiry；datagram_sent、
budget_deferred、io_would_block、send_failed；未认证的INIT/challenge/part观察、
续传/握手拒绝、awaiting_aead_confirmation、pending_expired及udp_route_activated。
发送观察包围原try_send_limited的一次实际调用，用原budget拒绝计数的只读前后值
区分预算延后与WouldBlock；不重新做准入、调用两次IO或把排队记为发送成功。
cookie challenge回复也按原实际IO结果记录，完整续传正文才进入原握手检查。
未知/未归属来源不猜测peer，不把未认证观察标为身份认证通过。

`udp_route_activated`仅表示本地原owner安装了认证UDP会话，不代表对方已确认、
对象DONE、Business Receipt或四机最终性。计数为观察次数，不是唯一包/尝试总数；
attempt_started才统计实际新建尝试。较大的恢复角色仍被动等待，不能为产生日志改选举。

本轮本地验证：Windows接线阶段网络库串行1292通过/0失败/13忽略；最终命名及测试
清理后Windows/Linux各5项定向通过，含真实本机UDP切回、推进时钟后的过期重试、
预算/IO/错误/签名过期区分、64peer边界和观察锁竞争。两平台严格Iroh Clippy及节点
lib检查通过，Windows默认feature严格Clippy通过。固定时钟旧例不冒充长期实机资格。
开发中较宽并行筛选触发两项原bootstrap测试端点许可耗尽，串行原全库包含它们通过；
原失败及中间Clippy问题保留在[本轮记录](../artifacts/audit/udp-recovery-observation-20261108/README.md)。

该源码提交本身不代表固定候选或部署验收。2026-11-09已完成其固定候选构建及包内
离线核验，程序源码为`5181da733d59d998afa898bb9964db5deae9e72c`：

- Windows ZIP 150464532字节，SHA256
  `6f28ddee10711ab9a4f72b2820f28c93475f075ddceee805ef2f10412510e811`。
- Linux tar.gz 359111230字节，SHA256
  `67ea573bdcb7ac8b90f4617ad6adeaf583da1515be399a481eeb08c609cd9f25`。
- 两包位于A `D:\WorksArea\SUPERVM\target\udp-recovery-candidate-20261109`。
  包内77/80项长度/哈希、实际AOEM验签KAT、node existing合同、377字节离线命令
  prepare/cancel及归档内容核验通过；不发包、不启节点、不接触用途钥或库。

[固定制品记录](../artifacts/audit/udp-recovery-observation-20261108/README.md#2026-11-09-固定候选完成未更新节点)
含node/client/ctl/AOEM/manifest哈希及实际核验报告。
[一次性A/C原库更新与定位方案](../artifacts/audit/udp-recovery-observation-20261108/UPDATE-PLAN-5181da73.md)
明确原库/身份、当前ddd回退、必要同范围路径规则及观察判据。
既有自动CI 42492089086只读核对为该完整源码SHA、push/attempt1、completed/success，
本轮未手动重跑；不以云端成功替代新包实机验收。
2026-11-09用户后继明确批准此次A/C更新及必要同范围规则，不能再称未获授权。
已提供原owner固定部署脚本及[C一次性接收/更新指令](../artifacts/audit/udp-recovery-observation-20261108/C-UPDATE-5181da73.md)。
本地部署门禁和原进程屏障通过；A的一次UAC最终报告用户取消，助手未启动，
没有更新原节点。不重复弹窗，原库计划提供管理员终端直接运行入口。
用户后继手动运行停在规则比较，旧进程未停止。已只读确认六类过滤范围相同，仅
CreationClassName随新规则关联名称变化；部署脚本改为验证各自正确关联后比较实际
范围，并通过13项正反例及6类真实过滤器门禁。见[修正记录](../artifacts/audit/udp-recovery-observation-20261108/README.md#2026-11-09-固定更新脚本的规则关联比较修正)。
程序5181da73及ZIP不变，C仅同步新脚本；A短入口已改为核验后ResumePrepared，
独立A-run2保留原失败，不自动重复UAC或初始化。
后继A-run2实际成功：LocalService PID13964，新固定node/AOEM匹配，原库existing及
用途材料保持，认证SDK Status 27/29ms、height0、原根一致。见[公开摘要](../artifacts/audit/udp-recovery-observation-20261108/a-update-confirmed-20261109.json)。
C新包已过但未启动：回执5cb5736e证明旧ddd在更新前已崩溃，0xc0000005；残留PID
仍需内核证明退出。已在同一固定部署脚本补显式RecoverCrashed，经过原任务/匹配
崩溃事件、内核signaled且无活跃owner及空闲原端口，才由原owner existing恢复原库。
不杀未知进程或删LOCK，不将0线程/路径为空当退出。A本地PS7/5.1及正反例通过，
不冒充C现场内核资格。C按[后继C-run2指令](../artifacts/audit/udp-recovery-observation-20261108/C-UPDATE-5181da73.md#2026-11-09-c旧节点已在更新前崩溃使用异常恢复分支)
同步脚本后执行一次；5181程序和ZIP不变，B/D不动。崩溃与实机UDP恢复根因均未销账。
后继C-run2回执49dffb0b停在事件空XML字段`#text`严格解析，未启动原库或新版。
A已按真实字段反例改用DOM InnerText，8项XML及原门禁本地通过；这是A的恢复脚本
缺陷，不归因数据库或原身份。C需同步脚本后按上述C指令使用独立C-run3，不复用
C-launch2的旧哈希/旧目录。ZIP不变，旧失败保留，C内核退出及新版运行仍待实际记录。
最新C回执5e82ce20已签收：C-run3内核证明旧残留已退出，LocalService PID34844以
原库existing启动5181，固定程序/AOEM/用途材料保持，25样本120秒观察完成。A原
SDK查询C104ms、原根一致。随后A原owner仅发一条新固定对象，发送端实际为native_udp
至192.168.71.125:41052，同一Object/Session记录owned/admitted/ACK/Object DONE：
Object `6cdfebd7bf6555c6a60962d3f11a51863d1fe9246dd41fed50f2e5d363de4524`，
Session `624b3e7ba0878c772d60c1ade8ef9212`。A后续120秒采样保留2次缺novonet投影，
不假报全成功；原拒绝计数和connection_failures未清零且前后不变。
详见[实际发送证据](../artifacts/audit/udp-recovery-observation-20261108/README.md#2026-11-09-c原库恢复签收新ac对象原生udp发送及done)。
94240da4阶段仅待C只读回传同一接收事件，不再更新、重启或重发，后继已完成如下。
这是已有原生UDP会话上的新对象，不是Iroh→UDP切回验收；旧崩溃/恢复根因未结案。
最新C回执267eae81已读取原ZIP并与A原ZIP匹配：同Object/Session/nonce及双方身份，
C确有一条probe_received_integrity_verified，native_udp来源192.168.71.74:41051，
与A native_udp目标192.168.71.125:41052的ACK/delivered/Object DONE一致。
[双端匹配报告](../artifacts/audit/udp-recovery-observation-20261108/a-c-native-20261109/two-ended-match.json)
签收**本条固定非业务对象的原生UDP双端交付**，不再让C操作或重发。
C逐对象/stage明细缺失照实记录，同一事件两种投影不重复计数；各机本地时间不作
单向延迟结论。不扩大为业务执行、最终性、Iroh切回、多跳、持续TPS或历史根因已修复。
实机根因仍未定位，旧证据不自动覆盖新代码；B/D及NOVOCHAT不动、不换钥或初始化。

### 23. 2026-11-09 实机中继前的原观察器与固定候选（本地完成，未更新节点）

第22节指定A→C原生UDP对象双端验收已经完成，不重发、不重采集。当前进入原计划
单中继/双中继阶段；没有将本机Iroh/QUIC测试改名为四机实网通过。

只读核对时A仍是5181 PID13964，41051/42051均属于该进程；B原ddd unit仍active、
PID1816591。原观察投影分别为0/1条bound路由、execution_failures=0；这是采样事实，
不是所有节点当前仍健康或中继已通过。A/B累计connection_failures分别4/167，未清零、
未据此将历史计数定性为本对象失败。C依据267eae81原回执，D未重新清点或操作。

#### 当前源码的实质缺口及最小改动

`network::relay::{outgoing,incoming}`及`relay::hop::bridge`已有真实逐跳实现；但旧
逐对象投影只有首跳carrier binding，没有原节点中继实际转发证据。新接线将**同一个**
`NativeTransferDiagnosticsV1` Arc从原NativeLifecycle/NativeChannel交给原Driver及
HopStream；不是另建观察服务、传输、socket、路由表或可靠交付协议。

- 原`probe_control_file`显式启用后才共享观察器；默认关闭不安装中继观察器。
  配置/诊断JSON不是业务wire，APFL、签名、AEAD及二进制帧完全不变。
- `native_network_probe.transport.relay_circuits`最多保留最新64条已认证circuit。
  输出route digest/circuit、原传输peer ID路径、本地角色/cursor及实际首跳binding；
  中继另外输出实际下游binding。不是从IP猜身份，也不输出私钥/exporter/业务内容。
- origin/最终接收方只在原native握手及路径绑定AEAD确认后记录native_session；
  中继没有该字段的授权信息。READY只表示建路，不是Object DONE。
- 原bridge两个方向仅在下游`write_frame`真正成功后累计opaque帧数/字节数。
  returned write error单列；取消/读错不能补成成功。计量包含E2E握手和反馈，不是
  业务对象个数、物理网络字节或中继计费凭证。流drop只记观察时间，不记业务成功。
- 同原观察器try_lock、不等待；竞争与64条淘汰/遗漏有显式计数，不阻塞热路径、
  不新增同步磁盘输出、不改变转发带宽/连接/消息期限和共识门槛。

实网签收须独立匹配：两端固定对象Object/nonce/身份/Session和Carrier实际binding →
同Session已完成E2E认证的circuit → 各中继相同route digest/path及其真实上下游binding、
两个方向实际转发 → 接收端完整接管及发送端原ACK/DONE。中继不能解密/打印Object ID
来凑证据；单方READY、规划路径、累计转发字节或状态根一致都不替代该闭环。

#### 实机执行边界

当前默认选路仍是健康NovoRUDP优先、Iroh合格备用；原fallback_path只在原失败轮次
推进。固定对象命令没有`--via`强制选路。仅封锁A↔C UDP不保证经B：Iroh还可能直连
或经外部relay；后者不等于NOVOVM节点B/D逐跳转发。

本轮不为制造多跳擅自禁用直连、改防火墙/代理、伪造或重签失效locator、放宽额度，
不更新/重启任何现有节点。新源码/观察字段尚未进入A/C的5181、B的ddd或D的082包；
后继真实窗口必须先固定合格制品，再明确需要的existing更新与有界路径实验范围。
不得复用旧A初始化任务、重创世、换钥或影响B既有应用服务。C/D目前无需操作。

固定程序源码883c6183a9dbaf73686e46381b178379bebf8217已完成两平台各17项定向测试、
默认/Iroh严格Clippy及实际node入口检查。原真实IO单/双中继新增实际下游binding、
两端同Session/digest与逐中继完成写入、新旧circuit断言；不是四机实网。
Windows/Linux原打包器及复制后独立全清单/实际随包库ML-DSA验签/原existing合同/
固定377字节命令通过；AOEM哈希保持，两个归档均校验。固定包、实际node/client/ctl/
AOEM哈希、原日志和本轮未决项见[本轮记录](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/README.md)。
完整云端CI、现有实例更新及真实单/双中继尚未签收。C/D目前不操作；不扩大为Iroh
切回、资产执行、最终性、稳定性或TPS通过，也不销账旧C崩溃及历史恢复停滞。

### 24. 2026-11-09 已批准883c逐台existing更新（尚未执行）

用户已明确批准第23节固定候选按 **B→A→C→D** 顺序更新；每台必要健康检查通过后
才操作下一台。包括必要的精确目标进程终止和Windows原规则完全同范围的程序绑定，
不再重复申请这项授权。后继核对：42542288594（71d5c225）已因既有自动并发策略取消，
不是通过；42544550783（9b2f6337）已completed/failure。失败作业为Linux实际ML-DSA
owner测试（128926765802），汇总门随之失败。7项中6通过、原实际NAT认证/同端口
重连用例失败，原stderr为`native authenticated probe did not complete`；不是B的SSH
登录测试，也不能由SSH故障替它归因。程序候选仍883c6183，尚未签收。没有手工重跑、
取消、skip或变更工作流，不以其余作业成功替代完整结果。
本节仅记录授权与本地准备，不表示已停止、更新或重启任何节点。

原Windows更新入口改为`scripts/windows/Update-NovonetExisting.ps1`，覆盖A/C原5181
及D原082，固定第23节包内manifest/node/AOEM。沿原LocalService、原进程内核句柄退出
屏障及原库existing打开；D旧任务包含initialize，更新和回滚均不执行该任务。
原版本专用脚本由这个入口替代，不保留第二套活动更新器。

新脚本区分仍运行的精确旧进程、已匹配崩溃事件的原进程及有明确任务/退出/时间证据
的正常停止；后两种需显式`-RecoverCrashed`或`-ResumeStopped`，不能仅凭缺少PID/端口
认定可以打开原库。D旧配置没有可选probe_control_file，由原配置owner补入新空命令
路径，保留全部发现/中继策略。复制防火墙前核对原UDP端口、本地地址、LAN/B来源，
复制后逐过滤器验证不扩张，不改全局profile。失败回滚仍用各机旧程序existing，
不运行初始化、不复制或清除DB/journal/LOCK、不生成/替换任何用途钥。

本地`pwsh -NoProfile -File scripts/tests/novonet-existing-update.tests.ps1 -PackageRoot
artifacts/product-overlay/novonet-relay-evidence-883c6183-windows-x86_64`通过：7项部署
门禁及实际固定包、13项过滤器、12项崩溃owner、8项完整事件XML、10项正常停止、
7项窄范围规则、4项A/C/D真实源码映射/旧配置适配；另保留原C事件反例的离线解析。
这些不等于已执行系统任务、防火墙变更或实机恢复。

B原SSH配置没有ProxyCommand/ProxyJump，文件及known_hosts仍为10月25日版本。
B原日志保留08:15:53同出口114.92.30.222公钥认证成功及08:53:06开始的preauth中断。
核对过的两者之间A命令记录没有B远端写入或路由/防火墙变更；用户确认VPN未切换。
09:37:19的B ssh.service停止/启动确实存在，触发者未查明，晚于首次失败，不能解释
08:53最初中断，也不能把它说成只是日志中无关的socket激活。

09:48:10–09:48:20 B的限时原抓包：TCP握手和客户端33字节版本到达，B发43字节响应
及1120字节后续数据，并重传seq 1:1164；最后客户端RST仍ack 1。13包/0内核丢弃。
这是“B响应没有得到TCP确认”的边界，不证明具体丢失位置、VPN根因或密钥错误。
A同次段由原客户端退出255保留，后来09:54的A ETL只有计数器/元数据、没有可解码
逐包事件，不能拼成09:48同一连接的双端证据，也不能把空导出当成网卡零收包。

经用户单独批准，A在09:16的B /32 ActiveStore直连对照已删自己的路由并核验恢复；
C回报12:29 Meta和12:35:07–12:35:27物理以太网（192.168.71.125→192.168.71.1）
两次keyscan均exit 1、stdout空，仅有B版本注释，没有ssh-ed25519公钥行。C回报
临时路由已删、/32残留0、选路恢复Meta；C原文件仍在C本机，A没有独立读取它。
这两次不作登录/认证通过，不能再仅凭默认路由名把根因归给Meta。A/C共享家庭出口，
也不是两个独立公网路径的对照。未改持久路由、TUN/VPN配置、防火墙或节点。

12:46:19–12:46:29 A改用原pktmon实时输出，仍仅B TCP22、每包64字节、一次原SSH。
实际本机端口64243、PID29728；WLAN最低观察组件132与TCPIP组件61均为Rx2/Tx4。
逐包能见SYN、SYN-ACK、ACK、33字节客户端版本、其ACK、超时RST，没有B SSH载荷
接收记录。原实时文本SHA256为ab3c5a2538a851182c786a8f48b3066378ffaeb28bb4588cc3491a38178c77c5。
实时查看进程在显式stop后exit 22并报驱动通信错误，原错误保留，不把整个采集退出
登记为成功；可读报文与独立计数器相符，6个TCPIP事件贯穿该次连接。停止及仅本轮
过滤器移除已核验。这次未同步B端抓包，不可与上午不同流拼接成根因结案。

用户开启B限时抓包后，12:50:30–12:50:31原SSH意外恢复：原配置/私钥及严格已知
主机指纹校验通过，publickey认证成功，只执行远端`date -u`，exit 0。两端同流已
匹配：A 192.168.71.74:60016，B观察114.92.30.222:62392；双方初始序列号分别
262072260、1130612452。B的43字节版本与1120字节后续报文实际到达A，A回ack 44、
ack 1164（相对值），之后完成认证/命令。12:51及12:53只读回传原抓包也成功，没有
换路由/VPN/密钥、重启SSH或节点。B ssh.service仍为09:37:19启动的同一PID2383850。
同次B文件`/tmp/novovm-b-ssh-paired.evnIDt`最终94包、0内核丢弃，包含后继只读连接，
不把94包全部归给第一次连接；A采集停止、仅本轮过滤器删除均核验。A实时查看器
仍exit 22，保留该工具错误，不影响独立SSH认证/命令成功和已取得的序列号匹配事实。
原材料在target/relay-evidence-20261109/A-B-SSH-headers-20261109-125030-010及
B-ssh-paired-20261109-125030.txt；B原文件保留，不再请求用户手工重复复制。

用户随后补充：刚才运行前已缴清阿里云费用，怀疑快照欠费。此前控制台截图确有
额度不足告警，且B实例包年包月、固定公网按使用流量计费。阿里云[计费项FAQ](https://help.aliyun.com/zh/ecs/billing-item-faqs)
明确：这种组合账户欠费时实例可以继续运行，但按流量公网带宽暂停，结清后自动
恢复；[计费方式FAQ](https://help.aliyun.com/zh/ecs/billing-method-faqs)说明快照等
后付费项目也可能导致包年包月账户欠费。因此“账户欠费影响按量公网、充值后恢复”
是当前与产品配置及时间线吻合的首要解释，不是快照数据损坏SSH，也不再把Meta列为
既定根因。尚未独立取得实际扣费/充值到账及公网停复服事件时间，故保留为强相关
解释而非已经闭合的根因证明；不从官方概括规则推断具体TCP控制包放行方式。
先前漏查截图费用告警如实更正，不继续重复网络探测、换钥或扩大安全组。

Linux CI原失败只输出子进程退出码与stderr，隔离夹具退出清理后公开report丢失。
本轮仅在原测试NatProcess失败分支带出原公开report和子进程标签，不打印配置、
credentials或用途材料；原25秒watchdog、3秒探测和15秒observer窗口均不变。
同一真实用例追加隔离静默UDP对端，验证真实失败report可读、accepted/round_trip
均false且不泄露用途材料路径。100ms仅此预期失败输入，不改产品期限。未修生产认证。
本地完整native_identity_cli各7项（含显式启用用例）Windows23.90秒、Linux106.93秒
通过，两平台同目标严格Clippy通过；使用实际新建debug NAT程序和固定平台AOEM，
不是线上节点或新release制品验收。云端原故障未复现、根因未定，不能注销原失败。
本地原证据在target/relay-evidence-20261109/nat-{windows,linux}-full-1.log及各平台
Clippy记录；这项本地验证与B的公网SSH问题分别记账。

未上传/安装B候选，未更新/重启A/B/C/D、未改B应用服务。SSH已恢复不等于候选门禁
通过或节点仍在运行；12:51只读systemd查询原B unit为inactive/MainPID=0，尚未
据此推断其退出原因或自动恢复。B/A未更新前，C/D不要
提前执行本节更新器或旧初始化任务。C的单次路由诊断已完成，不需要新ZIP或再次操作。

### 25. 2026-11-09 B旁路准备完成；原NAT观察窗口与冷启动分离

用户要求SSH恢复后立即回到原阻断点。本轮不再做SSH/VPN/路由/账单取证，只用已恢复
的原SSH执行必要的B更新前检查、上传和旁路prepare。没有重新申请已有883c更新授权。

**B实际状态与动作：**

- 原`novovm-B-ddd89e8e-network-20261108.service`的systemd管理者日志确认：
  11月8日09:17:44启动，11月9日09:17:49 `Deactivated successfully`；两条事件同
  INVOCATION_ID `bd9a8d297728477ca5461674ed539ac7`，最后progress为09:17:41。
  不是仅凭MainPID=0认定安全；无其他node、41050/42050监听，原24小时任务已正常结束。
- 第23节Linux归档实际上传至`/home/ecs-user/novovm-relay-update-883c6183-20261109`，
  359230285字节，远端SHA256仍为`3fa3b66645d99720e297c3688a704a601385c41ed36b20fefaca40420a87c91e`。
- 原B更新器适配为`scripts/linux/update-novonet-existing.py`，只执行
  `prepare --resume-stopped`；在独立release目录核验80项manifest/artifact/hash，
  实际加载固定SO `d73e4a2f...bc33`并通过scalar/unified-semantic/mixed-batch验签KAT。
  不是用开发目录另一个库代替安装目录的实际库。
- 新配置目录`/var/lib/novovm/selfrun/91129655606029/B/novonet-relay-evidence-883c6183-20261109`。
  run只改library和原network配置路径，network只改空probe命令路径；peer、域、
  genesis输入、carrier/validator/transport用途材料、locator所有者、限额和端口不变。
  原库仍`/var/lib/novovm/selfrun/91129655606029/B/chain-state`，没有复制、打开、迁移或初始化。
- prepare前后其他服务仍active/running：mailbox PID944911、Relay PID958、nginx
  PID627770、Docker PID1414。用途材料哈希相同。没有新node PID，探针未执行。
  新配置SHA256 `7b98117a1ea97846b2a72e81db3663f99b4ac44dd8b90ffcb15c9ea59282c36d`，
  network SHA256 `28a00280135b0832c296060f89a85bce70926dd7615ee1a00e5b028235901022`。

Linux更新器保留prepare/update/rollback分离，程序固定pin、原账户/加密credential和
existing。精确停止用原进程身份与pidfd，不误杀复用PID；中断退出不冒称正常排空。
已停止恢复需原systemd完整同调用start/stop和同期progress，不凭空缺进程推断。
失败回滚使用旧ddd程序、旧配置与原库existing，不重运行初始化。仅update操作才会
启动节点，调用者仍须先核对候选必检。本轮未调用update/rollback，不能将prepare写成
节点恢复通过。6项本地更新器守卫测试及实际固定Linux包全量核验通过，不是实机恢复。

**独立的候选认证阻断与本地纠正：**

自动运行42564342275（0b97718f），Linux作业128987599060再次6通过/1失败；标签已
明确为`probe-1`，accepted=false、session=null、exchange=0。第一probe已通过其
完整断言，但云端未保留observer退出报告，不能直接将下述本地机制宣称为该次根因。

两CPU本地原流程通过一次，但两个独立probe从启动到退出分别8332/8225ms。原identity
准备会对160MB实际SO做hash/load/资格和protected校验，发生在各自3秒网络计时前；
observer一旦就绪却只存活15秒，第二probe冷准备会消耗它的剩余窗口。一次明确标记的
临时16秒延迟反例得到：probe-0成功，probe-1无session/往返，observer已正常exit0，
原report仅1次request/reply。临时延迟已撤除，旧失败原样保留；不把故意制造的反例
计作普通门禁回归，也不将此延迟假定为云端实际耗时。

修正在**原诊断owner**分离protected准备与网络run，CLI显式`--wait-for-start`在
准备完成后报告本地marker并等父进程单字节启动；缺失/错误/超时控制失败。没有新
网络服务、socket协议、身份缓存或远端控制。默认CLI行为不变，原3秒握手、15秒观察
及原网络资源上限不变。两实际probe各自load自己的同一protected身份，依次prepare
以保留Linux独占读锁，再依次在同一UDP端口建立两个新session。原lib保护用途入口
的两个相关fixture也沿同一prepare/run顺序，不改其1秒/3秒窗口。

本地首轮并发准备错误触发Linux `journal exclusive lock unavailable`，已如实保留；
不是放宽锁或复制私钥绕过，而是改为两个同钥读取依次完成。测试明确检查prepare时
尚未绑定两个端口、没有成功report；之后仍须双向实际认证、不同session、两次观察、
严格身份/地址核对和原未应答反例。

后继最终本地检查：Windows/Linux各14项实际执行通过：完整CLI7项（25.87/116.05秒，
Linux taskset限定CPU0/1）、原product_nat_runtime6项（6.78/18.98秒）、原实际bin
启动命令1项（正确字节/错误字节/EOF）。后两类测试均非零命中，未将ignored跳过算作
通过。两平台实际debug程序构建、bin/CLI严格Clippy及fmt/diff检查通过。Linux两次
prepared用时8935/8903ms，之后两次真实新会话往返均完成，observer从网络启动到
退出仍15秒；没有靠增加3秒握手、1秒lib探测或15秒观察窗口取得通过。
这是本机实际运行库及隔离身份验证，不是发布包/完整云端或四机实网通过。

此次会改变随包`novovm-product-nat`及源码身份。原883c两个归档仍保持原字节，B的
旁路准备不代表新修改已部署；不得用新诊断验证结果冒充旧包全门禁已通过或自动把
既有固定SHA部署授权扩大为新包。A/C/D不操作，不需用户重新清点、换钥或做SSH对照。
尚无新的真实Object DONE、中继链路、资产交易、最终性或TPS结论。

精选原始结果归档为[本轮本地证据](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/nat-startup-local-results.zip)，
28577字节/23项，SHA256 `5f0f886726faa55b3ef261b875033b4360d34d3e768306dbdabe20753c963081`。
完整证据仍保存在A的`target/relay-evidence-20261109`：B-update-preflight-state/config、
B-clean-stop-journal、B-883c-prepare、B-883c-prepared-runtime原记录，原CI日志，
nat-linux-two-cpu-window-1、nat-linux-window-counterexample-1和后继coordinated-start日志。
原public配置有大公钥/签名数组，不把它们或任何受保护用途材料上传充当所需摘要。

### 26. 2026-11-09 固定2b97候选与获批B→A交付

用户已在本轮明确批准：以`2b97fc8e5d1f83c521669e6f82803794ad4fd5ed`替代此前883c
固定候选，沿原B→A范围逐台existing更新，必要时精确停止原节点、绑定Windows
原窄范围规则。不操作C/D，不初始化、不换钥、不动B邮箱/Relay/nginx/Docker。
包内实测和该源码必需完整CI通过后才切换；不能用准备/部分CI结果提前签收。

本轮已经完成两平台原打包器的真实release构建和独立核验。构建起止源码均为同一
干净2b97工作树，后继仅部署脚本的固定pin/版本化路径及交付记录变化，不改变已封包
程序。Windows复用复制到C的原编译缓存，Linux复用原WSL缓存；没有清理D现有
缓存、源码、原库或证据。压缩包改存C以避免D剩余空间不足。

包目录：`D:\WorksArea\SUPERVM\artifacts\product-overlay\novonet-relay-evidence-2b97fc8e-{windows,linux}-x86_64`。
归档/更新器/回滚和状态检查入口：
`C:\Users\leadb\AppData\Local\NOVOVM\delivery-2b97fc8e-20261109`。
大归档不进入普通Git历史。完整哈希见[本轮候选清单](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/coordinated-2b97fc8e/candidate-manifest.json)。

| 固定项 | Windows x86_64 | Linux x86_64 |
| --- | --- | --- |
| 归档字节 | 150370301 | 359149673 |
| 归档SHA256 | afe1f9911bb72c8cade93a09238deb77d6c5aaffc00ab47c787d77902025f778 | 332a2ef237a35b0169c98c5677cc59135046031d86f0845625c16a15078eb554 |
| manifest SHA256 | 0e54db51af068682e875f720a9f0e11807076c7d421e94117a2f13887f272e35 | 3569fb70b4b7b6f5cfa8306bedf384351f2eb02680523beb730da02dea060730 |
| node SHA256 | 255381b4825de9d5da29aa6ac64d0aec35d5028d53671aacb60aae22c5f4c91c | dbdcd2f08bd0ddb83ebd0a6027cf54f8d4dee9f03a7a9ab8b988750d8f79a8b7 |
| 全量独立核验 | 77项，ZIP逐成员hash一致 | 80项，原tar diff一致 |
| 实际包内CLI/NAT全7项 | 通过，19.46秒 | 通过，96.17秒 |

两平台实际包内AOEM仍为原1b764a36/d73e4a2f库，实际加载路径、scalar/统一语义/
mixed-batch验签KAT、原service-contract/existing及377字节固定probe prepare/cancel
通过。上述7项使用包内ctl/NAT和包内AOEM/manifest，不借开发目录debug程序背书；
包括protected身份生成/反例、同UDP端口两次新session往返、缺批验签能力的正式旧库
反例和未应答失败report。仅用临时隔离材料，不访问部署钥或链数据库。

原Windows/Linux更新器只改本候选固定hash、版本路径/unit/task名；不新增更新框架。
Windows原7/13/12/8/10/7/4守卫组及本次实际包通过；Linux原6项及其反例通过。
B新归档和原更新器已上传独立`/home/ecs-user/novovm-relay-update-2b97fc8e-20261109`，
远端归档hash匹配，脚本hash为
`786460b015241aaf834a583d3598192845996b3c3e580047c05c021202b9f8db`。
暂存未执行程序、未触碰原库。首次目录创建的Python命令因本机引号传递出错，在
任何远端写入前退出；改用原生单条mkdir后完成暂存，错误不归因于SSH/节点。

原自动CI42567619087已于北京时间2026-11-09 15:24:57结束，总结果success，
push/attempt1、源码完整SHA为上述2b97；没有手工新增、取消或重跑。它证明固定
程序检查通过，不覆盖下述后继部署控制器修正，也不代表A/B已更新。

首次B prepare通过；15:25的启动及自动旧版恢复都实际失败。新程序stderr为
`resolve resident configuration / Permission denied`；旧版恢复stderr为
`progress report failed / Permission denied`。首个控制器异常为
`Target PID owner differs`，不能据此认定实际身份被替换。

已由现场namei/stat和本地真实文件系统对照定位共同原因：控制器设置umask077，
原mkdir(mode=0750)被削成0700；新work和commands实际为root:novovm/0700。
因此新程序不能遍历work读取配置，旧版恢复也不能访问该work下的进度目录。
并非AOEM、密钥、账本、NovoRUDP或固定程序CI失败，不恢复废弃通信入口。

原更新器现对新目录chown后显式chmod为既定模式：work/commands0750、日志0700；
未改数据库、受保护钥或其他目录权限。launch在启动前以实际novovm账户检查原
配置、用途材料、manifest及日志/原库目录访问能力；systemd Type=exec避免默认
simple在exec/降权前返回PID的观察窗口，身份/程序检查继续保留，不跳过校验。
这项启动时序修正不反推其就是原身份异常的唯一原因。

只对本次已检查的失败增设一次显式retry-failed-start：固定同源码/制品/原配置、
服务/用途材料、无节点/端口占用、精确0700目录均须匹配，且要求旧失败记录存在。
在新evidence-retry-start-1及logs-retry-start-1留痕；既有失败/恢复日志不覆盖。
原远端控制器保留，新控制器另名update-novonet-existing-permissions.py，SHA256
`95dc1edcfa60a1e429341e2a9f2ec4c81ec43defe164c6f4c710f98bdf38c4b2`。
首次本地原6项加umask/真实目录/启动访问与Type=exec/拒绝错误重试等共10项通过。
后继再补重复回滚的停进程前拒绝反例，最终11项通过；原10项日志和回执快照保留。
最终控制器还为成功重入后的显式rollback选择独立回滚日志与该次证据；旧日志或
已请求回滚会在停止任何节点前拒绝。最终入口已另名暂存B（未再次执行回滚）：
`/home/ecs-user/novovm-relay-update-2b97fc8e-20261109/update-novonet-existing-recovery.py`，
SHA256 `3ec1353b41766c185d6f2b63fde8cf233dce7bd76fe3899ad88323e5d90e2e5f`。
需要时仍显式调用原rollback模式；当前健康实例不因验证入口而重启。

**B现已以固定2b97程序existing恢复运行**：PID2587563、UID996，实际node和加载
AOEM均与上表匹配；原库/用途材料/配置规则不变。原UDP41050/42050均由该PID
监听，locator序号从3287增至3289，默认未执行测试对象。独立原客户端认证Status
成功63ms，返回原状态根f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333，
height0/current_height1、无恢复/投影失败。mailbox/Relay/nginx/Docker前后仍active/running，
PID分别944911/958/627770/1414一致。未再次停新实例切回旧版做回滚演练，不把
共用权限修正冒充另一轮真实旧版恢复已验收。

本次按用户最新要求先收口B启动/恢复失败；A仍5181/PID13964，C/D不操作。
未发送新的测试对象，不签收双端Object DONE、资产业务、最终性或多跳。
详见[实际B修正结果](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/coordinated-2b97fc8e/B-permission-fix-result.json)
及[原始证据说明](../artifacts/audit/udp-recovery-observation-20261108/relay-20261109/coordinated-2b97fc8e/README.md)。

实际使用的回滚仍为B旧ddd、A旧5181程序和各自原库existing；不运行旧初始化任务。
保持原24小时有界运行，不宣称永久SCM或公开生产服务已验收。旧恢复停滞/旧C崩溃
不因本次候选通过而注销；无新的资产、最终性、多跳实网或TPS结论。

### 27. 2026-11-09 SQLite安全依赖修正，不迁移主链存储

4ca654cf的CI42600865983实际命中libsqlite3-sys0.38.2 / RUSTSEC-2026-0459，
安全job失败，后继检查被跳过；不是B的启动/回滚再次失败。与此前2b97安全检查
1422条公告相比，本次数据库1429条；旧锁文件未变，不用旧CI成功覆盖新增安全证据。

确认并沿用存储边界：原主链RocksDB/AOEM和CandidateStore/StorageSession所有者
不变，SQLite是唯一NOVONET L3/L4加密信箱及应用目录quota VFS，不是NOV账本。
最小修正将两个直接消费者同步固定rusqlite0.41.0/libsqlite3-sys0.39.1，保留bundled
及原C quota VFS，锁文件由Cargo生成且只变更这两个包；不改schema、业务、协议、
身份、共识、费用或原库，不增加忽略和迁移程序。

本地根/runtime两份锁文件audit均漏洞0、ignore=[]，两份原deny全范围通过。
两平台storage quota/WAL、真实SQL MailboxStore事务/TTL/领取/删除/原迁移、串行
信箱运行时/native session及严格Clippy通过；原库guard实际6项/平台、真实AOEM
启动重开和比例费用冷恢复各1项/平台通过。最初guard筛选零命中不计通过，原记录保留。

额外扩大并行试跑的默认Linux组3失败、Windows/Linux全feature组26/27失败均保留，
未声称全部并行回归通过。旧依赖Linux默认组通过、候选串行诊断通过，不等于已定位
首次3项EVM时序失败。原4端点额度及相关串行隔离要求不放宽；源码同步不签收新候选。
详细调用/检查边界、精确数量和原始日志见[SQLite安全修正](../artifacts/audit/sqlite-security-20261109/README.md)。

本轮不操作A/B/C/D。B仍为上节实测2b97，运行中的旧二进制不会自动得到这次SQLite
修正；新源码后续仍需固定新平台候选并按实际授权existing换包，不能沿用旧包哈希
冒充已经升级。只及时同步main，不手工新增/重跑、跳过或取消CI；原自动结果另记。

### 28. 2026-11-10 原生主网3fa候选已固定，existing更新待本版本批准

固定程序仍为`3fa11f940b18e54b46e6b927d3d66294bea37088`，本节文档提交不产生新程序。
原自动CI42609044443/attempt1已completed/success，14项job成功、2项按范围跳过；
本轮只读核对，未额外触发、重跑、取消或跳过门禁。第27节“源码同步不签收候选”
是前一阶段的准确状态，本节新增同源码的CI与平台制品证据，不覆盖旧失败。

原打包器的Windows/Linux release固定包已完成，分别77/80个成员hash核验，
实际AOEM加载及scalar/统一语义/mixed-batch KAT通过，包内protected identity/NAT/
reconnect等原用例各7/7通过；实际node的existing服务合同核对、probe prepare/cancel
通过，未启动服务或发送对象。AOEM保留原1b764a36/d73e4a2f，不改变主链RocksDB。
真实node二进制已核bundled SQLite3.54.1源身份及Linux quota/SQLite静态链接证据，
不是仅看Cargo.lock，也不冒称调用了不存在的node运行时SQLite版本API。

| 平台候选 | 字节 | SHA256 |
| --- | ---: | --- |
| novonet-sqlite-3fa11f94-windows-x86_64.zip | 150496796 | 3fc6a8ac67c84cabea906a0f2af39ad01a2f0e10e06bf4b25cf423359d995302 |
| novonet-sqlite-3fa11f94-linux-x86_64.tar.gz | 359238562 | d6ed8d869c46530077e9feb61b4ce0ef83dddcddc907f2e6d8a443825ae3e692 |

归档在A的`C:\Users\leadb\AppData\Local\NOVOVM\candidate-sqlite-3fa11f94-20261110`。
完整node/client/ctl/AOEM/manifest哈希、原CI/构建/测试及证据ZIP见
[候选交付](../artifacts/audit/sqlite-security-20261109/candidate-3fa11f94-20261110/README.md)。

额外并行组的56次失败/34个不同用例已逐条分类；原隔离方式逐例一次Windows26、
Linux30通过，不等于全并行通过或原根因全部解决。端点额度与全局计数、EVM测试
共享命名域、时限以及尚缺首错的锁/通道案例分别记录，没有提高生产限额或延时。
EVM只承担兼容插件/适配职责，不接管NOVONET、原生共识或AOEM状态所有者；插件
专属问题不成为原生主网新增前置。仅对已证实影响原生共用代码的问题按实际范围处理。

本轮未操作A/B/C/D，未上传新包、换钥、initialize、改防火墙或启用Ethereum网关。
当前运行版本沿第26节最后实测记录，不能把它写成本轮存活检查；旧24小时窗口可能结束。
下一步只需批准[固定3fa的B→A existing升级范围](../artifacts/audit/sqlite-security-20261109/candidate-3fa11f94-20261110/EXISTING-UPGRADE-PLAN.md)，
复用原部署所有者，原库/身份/日志保护与失败回滚条件不变；C/D继续不动。
新版本批准前不执行旧pin脚本或启动任务。没有新的四机DONE、资产/最终性或TPS结论。

### 29. 2026-11-10 固定3fa已完成B→A原库更新，开发期授权扩大

第28节的待批准/尚未换包为历史状态。用户批准后，B先完成原库existing恢复及
认证Status，再更新A；两机已运行固定`3fa11f94`，B PID2777931、A PID25216。
原账户/库/用途材料保持一致；Status分别62ms/28ms，状态根与更新前一致。
本机locator序列持续增加，原tick推进，未执行initialize或回滚。实际程序/AOEM
哈希、退出/启动证据及独立查询见[更新回执](../artifacts/audit/sqlite-security-20261109/candidate-3fa11f94-20261110/UPDATE-RESULT.md)。

用户明确整个NOVOVM/NOVONET及B邮箱/Web/Relay/Docker均为内部开发，不以不中断
服务为前提。开发所需停服、重启、换包直接实施；确认无价值的测试数据、旧钥、
旧包和中间产物可由Agent清理/重建，不再逐项申请。最高规则覆盖下方历史限制；
不误删其他项目/个人资料，不混用新旧域/账本/签票状态，不伪报结果。

本次固定包只升级node，其他应用服务实际未变，不作为未来必须保持在线的限制。
Linux更新器去掉其他服务PID不变的阻断并保留状态观察，本地13项守卫及Windows
固定包/更新守卫通过。C/D本轮没有操作，NOVOCHAT未改，不是要求它们重做清点。

网络结果不扩大：本次未发送新对象，非新DONE/资产最终性成绩。A连接失败计数
仍增长、bound路由0，B可选novonet投影在第二次样本为null，均保留原始现场。
下一步在新版本原owner上继续发现/选路及真实对象交付，不回到旧WSS或平行网络。

### 30. 2026-11-10 原网络owner保留有界失败阶段，实网原因尚未结案

继续第29节部署后的发现/交付定位，输入源码`b37a9155`。A/B实际仍运行`3fa11f94`，
没有重新初始化、换钥、修改防火墙或NOVOCHAT。A本轮样本connection_failures达到184，
旧生产程序只累加计数并丢弃错误，不能据此确定是连接、认证、域选择还是发现RPC失败。
`installed_bound_peer_routes=0`只表示没有已安装的备用载体会话，不是UDP路由总数；
原owner在存在待发对象时才请求备用，空闲零值不构成发现失败的独立证据。

A保留的B引导声明与B当前声明的传输公钥、Iroh endpoint key、直连地址及relay地址
一致；旧声明sequence12与当前4223不同，不等于指向错误身份。原引导合同允许用旧的
已签坐标建立新的固定身份认证，再取得当前实例，不能直接把过期引导归为根因。
B在03:02:45–46的限定42050/UDP短报头采集中出现双向数据（15 captured，0 dropped）；
观察到的公网对端为91.99.237.97，未认证其属于A。端口有流量不等于A的发现或交付成功，
也不构成Meta、运营商或防火墙归因。

本次修改只在原`service/network`owner保留最早/最近保留的失败详情：阶段、
原计数ordinal、可知的peer下标及最多512字符错误链。争用时try_lock立即返回，累计
计数照常增加，另记omitted；不建立日志队列、第二服务、网络socket或逐包阻塞输出。
原进度投影增加`failure_details`；原累计计数保留，并补记此前被忽略的carrier accept错误。
发现查询仍使用同一个8秒总期限；只把期限包裹放回原query内以保存取消前阶段，并为
连接、入口写入、绑定认证、域选择、发布、查询及关闭增加错误上下文。类型化终止拒绝
仍沿anyhow链识别，不因增加文本而变成无限重试。原超时、连接限制、认证及业务规则不变。

Windows/Linux release受影响网络组各8/8通过（234.38s/230.33s），覆盖原引导冷启动、
动态发现交付、单中继、双中继并发DONE、断开后重认证、错误链域拒绝及有界失败记录。
两平台发现安全/时钟回归各2/2通过，原node lib/tests严格Clippy `-D warnings`均通过，
fmt及diff检查通过。原始输出位于`target/novonet-failure-observation-20261110/`。
首次WSL验证启动因未安装pwsh退出，尚未运行测试；改用同参数bash入口，不安装额外工具。
Linux第一轮因未指定既有target而重编译独立缓存，已精确停止该本地构建进程组，保留
未完成日志；第二轮显式指定原`x86_64-unknown-linux-gnu`target复用缓存，不记首轮通过。
本次修改尚未制成部署候选，未取得对应云端CI或实网失败根因，不用这些组件结果替代四机证据。

#### 同轮现有3fa程序的真实A→B对象已完成，不依赖新观测代码

03:22:38（北京时间）调用固定3fa ctl只发布一条原377字节非业务命令；原A PID25216
接管并发往`47.103.59.206:41050`，B从`114.92.30.222:60735`接收，双方均标记native_udp。
没有新socket、节点重启、身份变化或资产交易。A记录`probe_object_done`，B记录唯一的
`probe_received_integrity_verified`，两端sender/recipient/nonce及下列标识逐项相等：

- Object ID：`843f247ca223d7c75039e064f7c043eb682f1aac075ada23e483950884c8f654`
- Session ID：`82dc48bfc7babd903750d82a068d57c2`
- 原控制lane transfer tag：`83a3e1ed2be3a7bf33944138e7cdab38e0893882b3b43fee2957cea1628e5eb2`

原tag按session、`NVBFT001`及Object ID重算后匹配双方传输记录；A确认1/1分片、
send_terminal_state=delivered及transport_done；B有完整片、hash verified及对象接管。
A同一时钟上的admitted→DONE为15ms，仅此单对象观察，不是TPS/P95或跨机单程延迟。
两端墙钟存在偏差，B接收时间早于A准入时间，不能相减计算网络耗时。A/B execution_failures
均0；不把非业务对象DONE当作资产授权、执行、持久化或共识最终性。A发现/连接失败累计
313、B为0仍保留，证明“发现失败累计”与“已有原生对象路径不能交付”不能混为一谈。

[双端比较结果](../artifacts/audit/novonet-discovery-observation-20261110/A-B-comparison.json)
及[原始证据ZIP](../artifacts/audit/novonet-discovery-observation-20261110/original-evidence.zip)
包含18项：双端原进度、命令、核对脚本、本地回归/Clippy及未完成构建日志；无私钥。
ZIP为128708字节，SHA256 `68b4fbdfe7bfd8bb26634b0eb2e81d8aba114bdfbba685646e857a698368fe99`。
C/D本轮没有操作；当前A→B通过不代替A→C、四机中继/多跳或资产验收。

### 31. 2026-11-10 观测候选先修原自动CI的OpenSSL安全依赖

第30节源码`55f947c9`的自动CI42683324372中，Rust security job129383748040
在根Cargo audit因`RUSTSEC-2026-0463`失败；其他作业当时仍在执行，不改旧结果、
不手动重跑。公告涉及`openssl::aes::unwrap_key`输出长度断言反向导致越界写，
并非SQLite、EVM或NOVONET网络回归失败，也不能用它解释A的发现失败累计。

实际依赖为lettre/native-tls的Linux邮件TLS消费者。Cargo只更新根锁文件
openssl0.10.76→0.10.78、openssl-sys0.9.112→0.9.118；runtime没有这两项，
无需改锁。没有ignore公告、修改门禁或改AOEM/存储/网络/身份/共识合同。
根及runtime Audit/Deny全部退出0，既有informational/yanked警告保留。
Windows/Linux原network owner各8/8、discovery各2/2、node严格Clippy通过；
Linux实际邮件依赖消费者严格Clippy、两平台rollout-policy现有测试各1/1通过。
该HTTP回执用例不是SMTP TLS实网验收。完整证据见
[安全修正记录](../artifacts/audit/openssl-security-20261110/README.md)。

接下来用新固定源码复用原打包器和包内7项身份/NAT资格检查，再B→A更新原节点。
本节提交时尚未制成新包/更新节点；3fa的A→B原生DONE继续有效，未扩大为四机成绩。
C/D无需操作。不以新增日志冒称现场发现根因已修复，不再追加无关架构或重复认证。

### 32. 2026-11-10 68fa7588完整CI通过，B→A原库更新及发现失败阶段实证

程序固定`68fa758820889c3f5e8bb230aa42cef100b77b4b`；原自动CI42685932056
最终success，Windows/Linux实际包内身份/NAT各7/7。原工具按B→A更新完成：
B PID2777931→2858350，A PID25216→37496，均existing打开原chain-state；
原身份/custody、实际AOEM/程序哈希、监听、locator发布及原状态根核验成功。
认证Status B前58/后56ms、A前40/后28ms，root均为
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`。
原进程按授权精确终止，不称正常排空；无初始化、换钥或回滚。A同范围程序规则、
三类防火墙启用；B其他开发服务本轮状态/PID未变。C/D不操作，手机不换包。

新观察取得A首次至第16次相同错误：stage=`discovery_lookup`，
`discovery deadline at carrier connect: deadline has elapsed`。B该累计为0。
实际发现query尚未从原IrohScope::connect返回，因此未进入入口写入、NOVONET
绑定认证、链域选择及发现RPC。connect内部地址/QUIC/流准备尚不能细分；
不能将该失败直接写成原生UDP对象丢包、ML-DSA拒绝或DONE丢失，也没有根因修复结论。
旧seed与B当前locator的公钥、endpoint及地址集合一致，不凭序号旧直接归因。

完整[更新、候选哈希和原始证据](../artifacts/audit/openssl-security-20261110/README.md)
已保留。程序与记录提交分开；不将后继文档SHA写成实机程序SHA。
本轮未新增对象触发；3fa的真实A→B原生DONE继续按旧版本范围保留，不继承为68或四机
中继/多跳通过。后继只定位原Carrier connect与双端同连接，不提高超时/资源或改签名。

### 33. 2026-11-10 发现冷启动候选保留与原 Carrier 观察（本地收口，不部署）

本轮输入为源码 `e9e8f01f`，A/B 实际程序仍为 `68fa7588`。用户只批准代码和本地
回归，明确不清缓存、不重启/更新 A/B/C/D；本节不构成换包或公网验收授权。

**对第32节的补充纠正：** 当时比较的是原 seed 与 B 当前 locator，遗漏了 A 实际
加载的较新 discovery-cache。只读现场显示：已验证原 seed 序号12含
`47.103.59.206:42050` 和已批准 Relay；缓存序号4166只有 B 私网坐标、Relay为空。
原 `maintain → update_seed → bootstrap_candidates → query → scope.connect` 只取
较新记录，未留下原 seed 坐标作拨号备用。第1/108次错误仍是 Carrier connect
deadline；没有同连接双端包证据，不把本次候选缺陷等同完整 QUIC/NAT 根因。
原始只读输入摘要与日志保留在 `target/carrier-connect-readonly-20261110`，不改现场。

实现仍在原 discovery owner：

- 原 ServiceLeaseBook 保留最高已验证声明/序号及撤销。每个现有 seed 至多额外保留
  一份原始已验证地址投影，最多仍是2个 seed；没有新缓存文件、发现服务或选路器。
- 首次仍拨最新记录。只有实际连接不可达的类型化错误、或尚未建立 QUIC 时原8秒
  deadline，才在原10秒调度周期的下一次尝试轮换配置 seed。每 seed 同时仍最多1个
  job，不在一次请求内增加重试、deadline、连接/转发/字节额度。
- seed 只提供地址，不提供最新声明或服务权限。必须是原固定 ML-DSA 身份/链域；
  Carrier endpoint key 也必须与最新记录相同。Carrier换钥、撤销、隔离阻止旧地址
  复活；认证/域拒绝、额度拒绝及未知本地错误不授予地址切换许可。
- 只将较新签名记录持久化到原 cache，不保存 hint 为新 locator，不修改签名字段，
  不延长旧业务租约。每次连接继续原 Iroh认证、NOVONET bound身份与链域选择。
- 原 connect_observed/with_inbound_observer 接入原节点观察。沿原 delivery observer
  开关，最多64条公开控制面事件、try_lock、不写盘、不等待日志消费者，丢弃有计数。
  记录候选来源/权威序号、公开端点、候选地址、原8秒deadline、地址/QUIC/流准备、
  本地Relay状态、RPC阶段、取消/额度/类型化失败。实际选中路径仅从已建连接被动
  快照取得，失败前未知就留空；不把候选当作实际路径，不调用授权回调来做观察。
- 入站 attempt 仍是本进程局部计数，不是跨机连接ID。后继需按实际端点与同一时窗
  核对；缺共同连接标识时不能仅凭计数相等宣布两端匹配。
- NOVOCHAT 对连接进度枚举有穷举消费者，本轮不扩充该枚举、不改手机代码。仅增加
  额度错误的类型标识和被动路径投影，错误仍拒绝；原错误文字、限额与回调合同保留。

验证边界及记录：

- 原实现先执行新增真实 owner 冷启动反例，失败为
  `cold discovery discarded the reachable frozen seed`（0通过/1失败）。不是内存
  选路模拟：冻结 seed 指向真实服务，较新 cache 指向不响应的本地 UDP 地址；成功
  条件包含原发现RPC、原 NativeChannel 收件及 Object DONE。仅本地，不称公网通过。
- 纯候选测试另用实际现场公网/私网地址形状（不向这些地址发包），验证序号4166
  不回退、同身份备用、不同域/钥/坏签名/未来输入拒绝、换 Carrier 身份及撤销拒绝。
- 两地址均不可达测试按原8秒逐次失败并检查 owner 退出；原额度、取消、stream setup、
  原生 NovoRUDP、发现RPC、中继/双中继和 DONE 回归单独记录，不提高期限来通过。
- 中间 Linux 新测试有一次生命周期编译错误；Windows 新夹具有一次准备阶段
  `NOVONET owner stopped`：原夹具只等待第一端发布，现要求两端都实际发布后才停，
  仍检查关闭结果。失败不覆盖、不当作候选丢地址或生产网络故障。
- 定向组结果如下，均为本地真实实现，不是四机公网验收；同一原节点组含冷启动、
  备用切换、单/双中继、重新认证和原对象 DONE。没有把筛选零项计为通过。

| 本地门禁 | Windows | Linux |
| --- | --- | --- |
| 原节点 network owner | 15/15，265.11秒 | 15/15，260.61秒 |
| 原 Iroh Carrier owner | 54/54，38.19秒 | 54/54，37.39秒 |
| 原 discovery control RPC | 15/15，2.81秒 | 15/15，2.43秒 |
| 原 NovoRUDP 组 | 209/209，5.78秒 | 206/206，5.64秒 |
| node/network lib+tests 严格 Clippy | 通过，`-D warnings` | 通过，`-D warnings` |
| node/native-client release 构建 | 通过 | 通过 |
| 最终源码新增4项复核（不重复计入上述总数） | 4/4 | 4/4 |

原始结果及旧版反例见
[`artifacts/audit/novonet-seed-retention-20261110`](../artifacts/audit/novonet-seed-retention-20261110)。
上述总计 Windows 293项、Linux 290项均通过；最后源码的冷启动、候选/撤销/预算、
观察开关4项在两平台复核通过。原组串行执行是本地隔离验证，不声称并行网络组全过。
`original-evidence.zip` 保留27份原始文件（包含完整 Rust diff `source.patch`、旧失败、
最终日志及原字节哈希清单），逐文件解压读取核验通过；日志原有行尾/空白不人为修饰，
也不经 Git 文本换行归一化。`result.json` 区分本地结果与部署，`local-builds.json`
记录本次本地可执行文件身份（不是已冻结部署包）。源码/文档格式及 diff 检查通过。
未签收发布候选、未触发本次部署。

受控更新方案（待另行批准）：最终代码本地通过后及时同步main，自动CI按原规则运行，
不手动重跑/跳过。若制作部署候选，绑定同一源码、两平台node/client、实际AOEM及
manifest哈希，再审查包内合同；本次不把旧68包验证转记给新包。批准后先B后A，沿原
工具 `START=existing` 打开各自原库、原身份/密钥/域；不改缓存、不增加防火墙范围。
保留68程序和配置，失败停止新进程后用68 `existing` 恢复，不运行旧initialize任务。
本补丁不改变数据库或cache格式，无需状态迁移。启用观察会增加有界公开地址元数据，
仅在本地审计通道使用；关闭时不构造事件。C/D保持不动。更新后仍须证明真实 A→B
Carrier连接、bound认证、NODE_GET以及对象DONE，才能关闭现场故障。

### 34. 2026-11-10 1ef31893两平台固定候选与包内资格（未部署）

本轮按用户批准只制作固定候选，不更新或重启A/B/C/D。程序固定
`1ef3189307382ef579775c7ce2b55a08d829d04a`，第33节代码及原失败不再修改。
原自动CI42717131364为push/attempt1，07:30:25北京时间快照仍in_progress；
完整门禁未完成不能签收完整发布候选。没有手动发起、取消或重跑CI。
后继核对：该运行07:35:01以cancelled结束（Rust作业取消），不是success；
固定制品仍未获完整候选签收。后继同步/候选CI分流已按用户要求另项修正，见
[CI分层规则](NOVOVM_CI_CHECK_TIERS.md)，不改本次程序或归档证据。

两平台沿原打包器完成实际构建、AOEM资格及合同核验。包内身份/NAT各7/7，
实际包内node的原生进程恢复各6/6、包内node/client的UCA恢复各1/1，原启动
三项pin与冷重开各1/1；同源码原network owner加载包内AOEM各15/15，覆盖
冷启动旧seed备用、发现RPC、单/双中继、并发DONE和重新认证。
15项为本地组件真实IO，不伪称包内node四机发现或公网连接成功；其他进程测试
使用隔离测试库，不是操作现有A/B库。严格Clippy和源码组仍按第33节同一源码引用。

| 固定归档 | 字节数 | SHA256 |
| --- | --- | --- |
| novonet-observation-1ef31893-windows-x86_64.zip | 150528007 | a374d1dc47eb209999e65b2066a39a01e96bb1d2e8d459fbff069e1df6fa5b8c |
| novonet-observation-1ef31893-linux-x86_64.tar.gz | 359271810 | 0ffcb14a76581fc17a8f479a3048bcdeb5bc434291d1ec87b6f4de84936f7509 |

本机目录为 `C:\Users\leadb\AppData\Local\NOVOVM\candidate-observation-1ef31893-20261110`。
完整node/client/ctl/AOEM、manifest及嵌套校验表摘要见
[候选清单](../artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/candidate-manifest.json)。
Windows80/Linux83个归档文件全部核验；原打包器按basename排除嵌套CHECKSUMS，
导致首次独立复核退出1。核对后仅将原输入同字节AOEM表单独pin并逐行验算
Windows26/Linux25项，不改程序或release manifest，不放行任意额外文件。
原失败转录、构建/包内结果、CI快照与复现脚本一并保留在
[原始证据](../artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/original-package-evidence.zip)，
SHA256 `0f400c439730eaf00d649364c1fd1ca8dce92e38c022a84af6fa7bfc04c2cd50`。

[完整交付与B→A更新/回滚方案](../artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/README.md)
明确：必要门禁通过并获本版本部署批准后，先B再A，原账户、原库 `existing`、
原身份/密钥/域和最新cache不变；保留68程序和配置，回滚只退程序不退账。
不运行含initialize旧任务、不改超时/额度/签名/共识，C/D不动。实施前再读真实
进程/状态根，旧回执不是当前存活证明。分别核验seed候选、实际Carrier连接、
bound认证、NODE_GET、Object DONE；任何缺项不得登记公网故障已关闭。

### 35. 2026-11-10 1ef31893已完成B→A更新及真实发现/原生对象交付，C接续

用户明确批准B→A→C→D逐台existing更新。Full `42721507092`已success，
head `5c6a6b48`相对程序`1ef31893`仅增加交付文档/证据，程序/依赖/构建/CI无差异；
因此复用这次完整资格，不触发另一轮Full，不拿`0ddf0ebf`轻量结果替代。

B PID2858350→2944965、A PID37496→17724，原库existing、原用途材料及状态根
保持一致；B/A认证查询分别64/29ms。B其他服务PID/状态未变，A仅同范围规则
绑定新程序，防火墙配置不变。无初始化、清库、清缓存、换钥或回滚。

A第一次使用序号4166的私网缓存拨号超时；第二次实际使用configured_seed，
选择Iroh relay完成QUIC/流、bound认证及NODE_GET；第三次用新签名声明直接
连接47.103.59.206:42050并完成发现。B同期入站TLS/Stream Ready已保存。
cache继续推进至4910，未回退seed12；第一条deadline保留，不把累计失败清零。

独立非业务对象沿原生NovoRUDP发送至B:41050，双方Object ID
`65a9412ab1973d2af85cda5401d1a06bbb66d97119c44a69b03eba21d4027372`、Session ID
`6f17d7154ef1199da11b37a5b70dbff4`、nonce及身份一致，B完整接收/校验，A最终DONE。
Transport ACK与Object DONE分别核对，不是Business Receipt或四机共识通过。
Iroh relay发现成功也不替代原生节点单/双中继验收。

实际摘要、原始双端记录和固定脚本见[本轮回执](../artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/README.md)。
现在C按[固定工具交接](../artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/CD-UPDATE.md)
先核验本机真实旧版本及运行/退出现场，再执行已批准existing更新；C成功且A外部核验
后才轮到D。C/D本轮尚未执行，原单/双中继、并发、恢复及资产三票验收继续待做。

### 36. 2026-11-10 认证会话后的自适应对象时间（源码实施，未部署）

输入为 `3fc0e2ac`。本轮复用 `novovm-network::network_time` 的原四时间戳区间算法，
在其 `coordination` 中维护每个固定对端的会话相对时间；原 `NativeLifecycle`
通过原 `NativeChannel / NativeObjectLink` 有界队列传输90字节时间载荷（原控制消息tag22）。
不建socket、后台服务或第二套可靠交付；NovoRUDP优先/Iroh备用不变。

随机挑战绑定双方身份、方向、原链/创世/协议承诺及实际Session ID。三次不同挑战
建立的是同一对端时间的**一致性**，不是三个独立可信来源，也不证明UTC正确。
认证与报文序列防重放仍归原会话owner，测量另用一次性挑战关联，不增加一套传输序号。
区间包括往返延迟、不对称路径、原算法向外取整及漂移；取样本区间的包络，
不以平均值把不确定性缩窄。响应排队时间保留在不确定范围，不虚报为已扣除的驻留时间。

新测量自身界限：每次2秒、采样间隔250ms、三样本、完成后10秒刷新、样本30秒过期、
漂移假设500ppm；本地墙上时间映射偏差超过50ms或poll间隔超过2秒即重新测量。
这些是新测量的保守策略，不是修改既有消息TTL、握手容差或业务授权期限。
每对端最多两条测量在途，计入原辅助队列和字节预算，不能占用共识保留额度。
旧会话测量、错误挑战、异常样本、期限耗尽均拒绝；不会无限保留测量发送。

当前实际消费者是非业务 `NetworkProbe` 接收校验：整个对端时间区间必须落在原签发/
到期范围内。已建立参考且签发时间落在当前误差区间内的对象，可由原probe owner最多保留4条，
从该次参考冻结开始/截止的单调窗口（计入漂移）；不阻塞控制通道、不重新取样延长期限。
窗口失效、会话或参考代次变化即拒绝。原10秒最大期限、发送端单调截止、固定对象内容、身份、速率及去重仍在。
对合法身份的新nonce，即使时间拒绝也在原有界历史中保留原最大寿命；已过期时间下界
跨重新测量/会话变化保留，防止一次后退校正复活旧对象。该状态为进程内状态，
不是持久授权撤销或全网时间权威。诊断提供ready、接受/拒绝采样数、在途数与收件区间。
Transport ACK、Object DONE（接管）和业务回执仍是三件事；新测量不会赋予资产或签票权。

**未完成边界不能合并报通过：**

- 首次原生UDP握手仍沿原 `NodeHandshakeInitiatorV1/ResponderV1` 的墙上时间合同；
  本轮不扩大原容差或把其时间置零。已有独立SessionV2/Finished代码不能通过改名
  当成当前UDP入口已迁移。大偏差设备尚可能在建立ATC所需会话之前被拒绝。
- 三样本只建立会话相对时间，恶意对端一致虚报时间无法由此检测为错误UTC。
  不将此结果用于UCA授权、共识时间或链上有效高度，不引入任意多节点多数时间投票。
- 检测墙上时间跳变/长poll间隔，不声称已接平台休眠通知或能检测两个时钟一起停止；
  C实机休眠/恢复、真实网络不对称与系统同步拒绝场景尚待验证。
- 参考未就绪或无法冻结确定有效窗口时明确拒绝，不先接受对象再补测量追认；老节点不认识
  新tag，不能把混合版本的DONE当作时间协调成功。NOVOCHAT、发现租约及Carrier握手
  本轮未迁移。原系统时间、A/B/C/D运行程序、缓存、密钥、数据库均未改变。
- 保守的时间下界防回退会拒绝对端墙上时间显著后退后的新对象，直到新参考赶上下界；
  这不是自动消除任意校时跳变的保证。若需要无等待的显式时间代次切换，须沿原对象
  签发/去重合同实现，不通过清历史或重启绕过。

两平台最终源码已通过时间层21项、原生命周期/归档恢复43项、编码合同35项，以及严格
Clippy；原始失败、最后收紧未来签发等待条件后的结果见[本轮记录](../artifacts/audit/novonet-adaptive-time-20261110/README.md)。
不自动生成发布包、触发Full或部署，不把原Node/Client入口编译检查称为固定制品资格。

#### 36.1 时间攻击参考的采用决定与后续顺序（2026-11-10）

依据用户本轮提供的八类时间攻击、五道防线及安全矩阵，采用以下约束：
**设备时钟可以不准确；对端相对时间不能成为资产、授权或共识的安全权威。**
本节是设计决定及待办，不新增已通过的测试，不把上节源码资格扩大为实机资格。

认证只证明样本来自绑定身份，不证明对端诚实、UTC正确或多个身份属于独立主体。
当前每对端参考只用于该对端的非业务对象时间，不是集群时间，也不修改本地系统时间。
三次挑战只验证样本一致性；即使恶意对端连续一致地虚报时间，也不能由此获得
资产花费、签票、授权延长或更改链上时间的能力。认证但虚假的时间仍可能造成该
对端对象被拒绝或相对期限判断失真；不能宣称已解决恶意时间源真实性。

**实施顺序：**

1. 当前工作包先接首次原生UDP握手：复用已有SessionV2/Finished所有者和明确版本合同，
   用新鲜挑战、完成证明及本地单调截止建立会话，不通过删除旧握手时间检查、放宽容差、
   降级回退或把旧报文时间置零来消除启动前置。握手成功不授予UTC信任。
   完成受影响反例与两平台本地验证后再登记源码完成；当前尚未完成该接线。
2. 在当前时间机制的原owner补齐尚缺的安全反例，尤其是测量洪泛、连续失效/重测、
   截止边界、取消及会话替换；沿原队列和预算证明有界，并检查合法通信仍可推进。
   不增加第二套时序框架、不提高生产资源上限。已有反例直接复用，不重复重建。
3. 首次握手及时间owner合格后安排C实机快慢时钟、拒绝系统同步、休眠恢复、前后跳变；
   系统校时只在用户授权时进行。实机结果与注入时钟的本地测试分开。
4. 发现租约、NOVOCHAT及其他消费者在各自合同接入时复用同一基础能力，逐项定义
   失效与撤销规则，不一键替换所有墙上时钟读取。UCA授权、资产交易及共识继续遵循
   原确定性协议；没有单独获批的合同和验证，不接对端时间校正来放行。
5. 多独立来源/可信UTC暂缓：先明确需要可信UTC的具体消费者、来源信任与恶意权重假设。
   不按节点数、质押或多数中位数自动宣称可信，也不引入机构审批普通节点。
   网络路径多样不等于运营者或时间源独立；目前不能自动证明独立性。

**安全验收矩阵（代码基线 `c87855ac`；下列已有测试未在本次文档修改中重跑）：**

| 风险 | 当前证据/边界 | 后续验收条件 |
| --- | --- | --- |
| 墙上时间前拨、回拨 | `wall_jumps_poll_gaps_and_new_sessions_invalidate`；参考失效，不改链上规则 | C实机两方向跳变，旧窗口不能继续用 |
| 回拨复活旧对象 | `backward_remeasurement_cannot_revive_expired_message`；原owner保留拒绝nonce，时间下界仅进程内 | 不用该内存状态声称跨重启持久授权保护；消费者迁移须保原持久nonce/撤销合同 |
| 单节点虚报时间 | `conflicting_samples_are_not_averaged_into_permission`只覆盖不一致样本 | 一致虚报不宣称可识别；证明其不能进入资产/共识权威，保留可用性风险 |
| 多节点串通中毒 | 当前没有全局时间聚合或多数选时 | 启用任何全局参考之前定义信任假设并验证串通反例；当前未实现/未验收 |
| 延迟、非对称、迟到 | `asymmetric_delay_is_uncertainty_not_midpoint_authority`及测量超时反例 | 原owner实际排队、丢包/重传仍不缩小误差或延长被冻结期限 |
| 旧测量重放、错会话/域/方向 | `response_replay_wrong_challenge_context_and_timeout_rejected`及context绑定测试 | 首次握手迁移必须另验旧握手/Finished重放，不能继承会话后测试结论 |
| 休眠后旧参考复活 | poll间隔/单调倒退/墙时跳变失效已测；未接平台休眠通知 | 实机验证；两个时钟同时停止的检测仍是明确缺口 |
| 过期参考、刷新延长期限 | `reference_expires_and_deadline_cannot_be_extended_by_refresh` | 各新增消费者冻结本次决策依据，不事后用新参考追认旧请求 |
| 临界时间重复接受 | `fresh_boundary_wait_is_frozen_and_jump_invalidates_generation`；原probe nonce隔离 | 接收窗口/取消/会话替换组合反例；Object DONE不等于业务接受 |
| 测量洪泛、失效风暴 | 每对端两条在途、响应间隔250ms、原辅助队列预算；这些是实现边界 | 尚缺专门持续洪泛/重测风暴验收，验证总量有界及原业务进展，不仅检查常量 |

参考中的`TimeReference`结构不直接复制成平行实现：原`PeerTimeReferenceV1`已有
context、generation、样本、单调寿命和防复活下界。generation是进程内失效标记，
不是持久、签名的全网单调序号；如后续需要跨进程证据，必须在原消费者合同中补齐。
原测量期限及漂移假设限制可接受样本；本次不凭示例发明最大绝对偏差或新的容差。
最大不确定范围须与消费者剩余有效期共同约束，不能把稳定但很大的偏差当成错误UTC证明。

无法建立参考时，只拒绝或有界等待依赖该参考的操作；不因此关闭所有非时间敏感通信，
也不让待验证队列无界积压。本次仅记录决定，不修改系统时间、不更新A/B/C/D，
不改变认证、账本、AOEM、共识、资源上限或默认传输路径。

### 37. 2026-11-10 首次原生握手采用会话相对时间；四机统一升级授权

输入`b4f22132`，本节更新36/36.1中“首次原生握手尚未接通”的源码状态，
不追认旧实机结果。用户随后明确“全部直接升级就好了”：本轮节点和配套操作客户端
按同一冻结版本推进A/B/C/D升级，不为同范围再申请；仍须完成固定制品及必要门禁，
真实版本/进程/原库检查和更新结果如实记录。尚未因此停止或更新任何运行节点。

原`NativeEndpoint -> NativeHandshake -> NodeHandshakeInitiatorV1/ResponderV1`
改用显式签名profile3；复用原ML-DSA、ECDH、HKDF、AEAD、分片cookie、重放缓存及
资源预算。版本3的零起点/相对寿命是**新签名域合同**，不是把旧签名报文时间置零
并绕过验签；旧profile2检查不变，原生入口不降级接受它。没有新增socket或网络服务。

原始可靠Carrier的SessionV2要求真实TLS exporter及VerifiedTransportBinding，不能
直接冒用为裸UDP已认证证据。本次抽取共用Finished帧构造，原生采用独立domain和
原签名transcript，交换双方AEAD Finished。必须验证对端Finished并真实提交本端
Finished才暴露channel；双向sequence0只用于Finished，原业务从sequence1继续。
Finished重传复用密文，不重置序号；不能充当业务帧、Object DONE或Business Receipt。

所有flight保留原本地单调截止；对端相对寿命只可收紧、不延长。重放旧offer即使
缓存已轮换，也不能用旧Finished证明新的响应密钥；重复offer不加速重试、不续期。
首次未确认客户端到期释放原名额；现有重连/退役/地址学习仍等待原允许stream的
AEAD确认，不凭新offer或Finished替换已有业务连接。

这是持钥和新会话证明，不是UTC真实性、链最终性或资产授权。首次传输握手没有新增
链域字段；原上层链/创世/协议承诺绑定仍必须执行，会话后时间挑战继续绑定这些字段。
原ECDH未改，不能把ML-DSA签名称为抗量子密钥协商。发现租约/聊天及可信UTC尚未迁移，
36.1的时间测量洪泛/重测风暴、平台休眠、真实C验证等剩余项目继续单列。

**兼容性：原生两端和操作客户端必须一起更新。** 已安装1ef程序不能用旧原生握手
与新程序完成会话；可靠Carrier合同继续独立核验，不作为静默绕过。准备固定候选时
覆盖Node/Client/NAT/ctl及相应主机配置，不能只更换A或只更新库便宣布四机合格。
双方新profile相对旧两flight增加两条小Finished报文；没有吞吐/延迟改善结论。

源码调用、反例、实际测试及原始失败见
[首次握手记录](../artifacts/audit/novonet-native-session-clock-20261110/README.md)。
注入墙上时间和真实loopback UDP/QUIC测试不等于C实机校时/休眠或四机公网验收。
下一阶段冻结同版本候选，核验包内程序及AOEM，再按已授权范围更新和双端取证；
不修改共识、账本、资产期限、系统时钟或资源限额。

### 37.1 同轮升级阻断：时间辅助队列先过期、后发送

用户明确允许内部开发系统直接停服切换，不再因服务连续性重复申请。
e7b0cca7两平台包内资格通过，Linux包已上传B并核验哈希，尚未安装；四机仍未换包。
该源码Full 42770392703的原生恢复作业129676752806实际4过2失败，两项迟到/恢复
用例的节点均因`native send operation deadline`退出，不能用包内通过覆盖云端失败。

已在旧实现上确定性复现：时间测量flight只在assembly轮询清理，而原I/O可先尝试
已过期的未接管对象，导致期限拒绝传播至节点退出。修正在原I/O报告/发送入口复用
同一清理所有者；已接管对象仍等原传输终态释放，不改期限、额度、共识或资产。
两平台生命周期回归各44/44及严格Clippy已通过，27项CI范围测试通过；原六项进程
恢复随后在Windows/Linux各6/6通过。C/D不需要重跑旧脚本或清库。原始失败及实际结果见
[首次握手记录](../artifacts/audit/novonet-native-session-clock-20261110/README.md)。
修正源码、新包与实际切换分别登记，不把旧e7包称作含本修正，也不宣布历史停滞全部结案。

### 37.2 固定8fed候选与四机直接切换（16:00现场快照）

程序固定`8fed4b76f518935b30fcfb13930ccd3d7aae7381`；两平台包内身份/NAT7、
owner I/O15、实际打包节点恢复6、UCA恢复1、启动资格1均通过，全部归档成员独立核验。
Windows包150603521字节，SHA256
`29bee97601a7fb8ba698bf8bb5bff356e1c97c8e8d2a750be871f3c928fe73e4`。
Linux包359039913字节，SHA256
`c279cc1a4860f14e2142e0fa6bbf245d9f0f48f1e05358a9bd3efae7efa9953a`。

一次Full `42776380753`绑定相同程序SHA、attempt1；当前原生恢复、安全及两平台
实际runtime作业通过，Rust与汇总仍待结束，不把轻量同步或局部绿灯当完整通过。
四机尚未切换，B新包上传中；A新operator同范围规则已准备。原更新器离线组通过，
初次生成测试残留082/5181旧期待已按实际1ef修正，未放宽程序/库/状态检查。

停服许可已具备，完整门禁通过后直接B→A→C→D existing，不再申请同一许可。
新profile3节点必须配新operator，不能拿旧profile2失败当新节点不可用或启用降级。
C/D可先复制新Windows包，收到A/B完成回执再逐台执行；不运行旧初始化/恢复脚本。
固定包、工具、原始测试及当前状态见
[8fed交付](../artifacts/audit/novonet-native-session-clock-20261110/README.md)和
[C/D交接](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)。

### 37.3 Full通过、B/A切换完成，C现在执行

`8fed4b76`唯一Full `42776380753` attempt1已completed/success，Rust及两项汇总
均成功。自动B→A existing切换于2026-11-10北京时间17:32:06完成；A PID21708、
B PID3133881，实际固定程序/AOEM与候选摘要匹配，原库、用途材料及状态根保持一致。
新operator外部认证Status：A27ms、B79ms；这不是对象DONE或资产业务验收。
本轮写交接时再次确认两台新进程仍运行。

C/D准备回执e9ab5b61、9ed96eac已读取并合入。**现在C直接执行既有已批准更新，
无需等待A/B或再次复制包/申请停服**；C回传后由A做新客户端认证查询，然后D执行。
具体固定路径和命令在[CD-UPDATE当前首节](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)。
[AB公开回执](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/AB-CUTOVER-PUBLIC.json)
及同目录证据ZIP保留Full原始响应、切换与查询结果。此前只口头报告、未及时更新Git，
导致C按旧交接暂停；本次补齐，不把先前准备快照改写成当时已部署。

### 37.4 C更新预检失败：先一次采齐旧owner退出证据

已下拉C回执`f01f849f`。C工具测试通过，但普通更新入口在停止旧进程之前报
`Original process identity not confirmed`；没有启动8fed。后查未见进程/监听，
旧1ef退出原因尚未知，不能称新版启动崩溃或原库损坏。B/A已完成结果不变，D等待。

C按[CD-UPDATE最新首节](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)
执行同目录`Inspect-C-OldOwner.ps1`一次管理员只读采集；原任务结果、内核进程、
监听、原owner/progress/stdout/exit结构、stderr已知标记及匹配崩溃事件一起回传。
复用旧公开读取函数和固定进程helper，不读库/钥、不启停、不校时，不重传包。
脚本已在A完成语法、仓库路径及只读命令边界检查，尚未在C执行；不冒称C退出已归因。
取得实际记录后选已有停止/崩溃恢复入口，不猜测追加恢复开关，不重跑普通入口。

### 37.5 2026-11-11 C旧进程退出已核实，直接启动新版

已下拉`243ffb25`：旧1ef于11月10日12:25:26访问冲突退出，任务结果、exit及
Application1000/PID24376一致；管理员采集无现存进程/监听、无读取错误，progress
是有效JSON。A将实际字段送入固定原更新器Assert-CrashedOwnerEvidence，检查通过。
无需修复或重跑旧版本；C现在直接使用**新8fed包、原更新器-RecoverCrashed、existing**。
原包和工具不重传，已有测试/Full不重跑，不追加停服审批，不猜测清库或换钥。
[CD-UPDATE最新首节](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)
给出可执行命令；C返回新节点结果后A查询，成功后D执行。旧访问冲突根因仍未定位，
不能把新版恢复通过（尚待回传）写成该根因修复。这里的恢复是新版打开现有账本，不是恢复旧程序。

### 37.6 2026-11-11 C新版与A认证查询通过，D现在更新

已下拉C `2db1fead`，实际8fed node/AOEM摘要及LocalService原库恢复记录匹配。
北京时间10:32 A用固定新版客户端查询C成功43ms，原状态根
`f6186d40260aa18e84b01101d56ca3e918aff799ef41d927aeab554495f56333`
一致，height0/current_height1，未恢复中/无投影失败。原SDK输出已归档。
**D立即按[CD-UPDATE最新首节](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)
执行已批准同版本existing更新**；无需重传包或等C，C不再操作。D须核对自己的当前
owner，不盲抄C崩溃标志；回传后A认证查询，再做实际对象。查询不等于DONE/共识，
旧C访问冲突根因不由新版恢复通过自动销账。未另触发Full。

### 37.7 2026-11-11 D中断退出不阻挡直接启动新程序

已下拉D `ce2637ea`：旧1ef任务Ready/result267014，无进程和41053/42053监听，
旧progress有效、exit.json缺失。不是8fed启动失败，不把原因未明的退出写成正常排空。
原Windows更新器新增显式`RecoverInterrupted`，仅接受该任务终止状态及原有身份、
无重启/触发器、空闲进程/端口、有效时间线和原库/钥/固定包检查；再由8fed existing
真正验证恢复。失败精确停止新实例，不自动回到原因未明的旧owner。
原更新器全部回归、D真实回执、11项中断反例、6项实际参数保护、固定77成员包及
13项进程屏障检查通过。只更新工具，8fed程序与原Full资格不变；小ZIP随Git交付，
不重新传150MB包、不另触发Full。D按[CD-UPDATE首节](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)
直接执行新版恢复并回传，A再认证查询。C/A/B本步骤不动；D尚未启动成功，不称DONE。

### 37.8 2026-11-11 D新版与认证查询通过，待A本机恢复可观察性

已下拉D `61e08288`并逐文件核对摘要；8fed PID19756/LocalService existing、
AOEM及原用途材料一致。A固定SDK对D认证Status成功34ms，原root一致、height0/
current_height1且无恢复或投影错误。D更新收口，不重复执行。
本轮未发送对象：当前A工具账户无权读取受保护任务/logs，对A的新Status查询超时，
只读UAC检查被取消。不能由权限不足直接宣布A节点崩溃，原因未知。
先完成A提权只读检查，再按实际状态恢复现版owner；之后D开启已准备的30分钟有界
只读收件观察，A只发一个原owner对象并匹配双端记录。D/C暂不操作、不重传包、不发Full。
原查询及[当前动作](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)
已保存；Status不等于A节点发送、Object DONE或业务Receipt。

### 37.9 2026-11-11 A现版原库恢复通过，D开启自动收件观察

用户提权检查确认A当前8fed任务Ready/267014，无进程/端口，旧progress有效、exit缺失。
A核对固定包、原owner哈希与existing任务动作、LocalService及原库/用途材料后，仅将
原logs完整移至同工作目录logs-interrupted-20261111，创建原ACL新logs，启动同一8fed
owner。11:03:58恢复PID13996、41051/42051，独立认证查询33ms且原root一致。
[实际执行与原始记录](../artifacts/audit/validator-a-8fed-existing-resume-20261111/README.md)。
不是旧版本回退或初始化；停止原因仍未知，不宣布修复。D现版和查询此前已通过34ms。
现在D仅按[CD-UPDATE首节](../artifacts/audit/novonet-native-session-clock-20261110/candidate-8fed4b76/CD-UPDATE.md)
后台开启30分钟有界只读观察并回传D_RECEIVER_ARMED，A再通过原owner只发一次对象。
本步未发送对象、不重跑Full、不更新D/C；窗口超时未发送记未执行而非网络失败。

### 37.10 2026-11-11 A原节点一次发送取得native_udp DONE，待D收件回传

已拉取D `0ce2733e`，其原节点19756/观察器21680在11:10:37开启有界采集。
A于11:13:33通过现版原owner13996只调用一次固定ctl，直连192.168.71.59:41053，
获得probe_object_done及实际transport delivered/ACK记录；无重发、备用路径或新socket。
Object `a89c1dac10c0405ca4d153bb03b03279d6339cd4d465b7aa81352b1902dcafd4`，
Session `d748367f3ba5e381bc8468467db43fb0`。
[原始命令、发送前后诊断、退出记录及待匹配状态](../artifacts/audit/novonet-8fed-a-d-object-20261111/README.md)。
D只需回传现有采集输出，不重启观察或节点、不重发。接收完整性/有效期尚待双端核对，
不能把发送端DONE签收为业务Receipt、四机网络/共识完成；无新Full或TPS结论。

### 37.11 2026-11-11 A→D真实原生对象双端核验通过

已下拉D `d68074f7`并核验310817字节ZIP及13个原始成员。离线验证器直接读取A/D
原始包，逐项匹配Object/Session/nonce/双方身份/原有效期/实际native_udp两端地址。
A有ACK覆盖、delivered与Object DONE；D有单片完整组装、哈希验证、对应反馈epoch，
先pending_time_boundary再probe_received_integrity_verified，同会话相对时间参考就绪。
7份快照只对应1个唯一验收事件，不计作7次交付；D观察器自动退出0、无超时/错误。
[双端匹配报告、复核脚本与原始证据](../artifacts/audit/novonet-8fed-a-d-object-20261111/README.md)。
本条A→D直连对象已收口，不重复观察/重发，不再要求D操作。未发Full，未重启或改时钟。
不外推四机全部路径、多跳/中继、资产Receipt、共识/TPS或全部ATC攻击场景；旧A/D任务
终止及C旧崩溃原因仍分列，不由本次通过销账。

## 2026-10-17 Android v27 跟进（共享runtime未改）

SUPERVM b6cc81f9保持；NOVOCHAT d5e547b/v27仅修原Android网络owner对完全相同LinkProperties
仍增代次的问题，使用Parcel快照/equals，真实变化继续通知原native owner。
17项Kotlin、27项Python通过；两机原位安装保持各5项身份/密钥hash且无业务隧道。
9302/9303设备回执1492/1573ms；一次18秒断网中9304先由L3真实寄存、L4失败，
恢复后自动领取，总DeviceReceipt18.516秒，不能将该条计作冗余成功。
随后9305两路寄存、设备回执1717ms。蜂窝仅实际断网1次code1并自动接替，
最后55秒/42样本无新增首错。实机未捕获重复属性回调，不认定历史code1已
归因；完整分段和APK标识见[台账](NOVOVM_PRODUCTION_READINESS_TRACKER.md)。
仍依赖外部Iroh，不称五轮、P95、自主公网或匿名通过。

## 2026-10-17 Android v26 原生L3与原L4实机跟进

输入SUPERVM4888b7da，手机NOVOCHAT e3e9c78/v26。另仓修复同self-owned profile
预检：无副作用preview后，approve仍重验全部checkpoint、撤销和签名。
两台原安装升级前后五份受保护文件hash不变。同信任pin通过界面导入新声明、
重启原节点后，两手机在各自原task1自动选择NVMBOX07，原L4路径继续运行。
健康旧V6会话不会自动获得新能力，本次不算完全自动的旧版升级。

9297/9298均完成原生L3及原L4真实寄存与设备回执；prepare至首次DeviceReceipt
为1375/1432ms，L3 Put至StoreReceipt为331/768ms，L4为959/1091ms。
一次18秒蜂窝断网中，9299由L3寄存393ms，L4寄存失败，设备回执9.940秒；
系统首次available至native ready为5.443秒，随后网络代次再次改变，新task
ready为5.492秒。恢复后9300：L3寄存722ms、L4寄存863ms、设备回执1369ms，
L4先回、L3重复回执沿原去重处理。

132个观察样本中蜂窝7次路由probe失败均在断网，2次code1随网络代次变更
正常交接；末尾Wi-Fi task1、蜂窝task3，L3/L4 ready。正常两条L4路径实测
外部Relay RTT482–522ms；原生认证也经Relay，但未逐Put独立采样实际路径。

最终force-stop后由正常入口重新打开，截图确认9299/9300及Delivered保留，
两机原生L3恢复；这不是强停后的自动唤醒。首45秒蜂窝仍available=true/
mask2，但记录gen8/task7的code1；2.975秒自动回收并创建task2，首错后L3
就绪4.995秒、L4回执通道9.608秒。Wi-Fi也曾有L4 code63后恢复。后续60秒
46个样本无新增首错或issues，末尾Wi-Fi task1、蜂窝task2，L3/L4 ready。
code1未根治；下一刀核对LinkPropertiesChanged无条件增加网络代次，当前
日志没有属性差分，尚不能认定该回调是唯一原因。

重开后9301继续完成原生L3及L4寄存和设备回执（Put→Store193/1344ms，
prepare→首回执1695ms）。最后30秒24样本无问题，两端保持L3/L4/sendReady。

证据：NOVOCHAT `artifacts/continuity28-*`。这不是点击至屏幕时延、P95或
五轮验收；外部Iroh依赖、自主公网、全UDP迁移、隐私和Windows崩溃自动
恢复未签收。下文保留各轮历史状态。

## 2026-10-17 Windows 原节点已启用 Carrier V7；手机迁移待验收

输入1d533c76，认领4a0a5d23；本片节点实现、验证及部署已完成，认领释放。
NOVOCHAT2ef8f7d/v25已安装在两台原手机，保留L4及旧V6兼容能力；这不等于
手机已实际选择原生V7。下文较早章节的待办是各轮当时状态，以本节为准。

原 `novovm-rendezvous-service --set-self-owned-native-carrier CONFIG on|off`
在原独占Owner锁下原子更新DPAPI策略，缺少新字段的旧策略默认关闭、读取不写回。
`Owner::run_inner` 的原Iroh activation构建同一个ServiceLeaseHost，首次发布前
调用 `rotate_native_carrier_mailbox_v7`，沿用原任期、签名身份、传输seed和日志。
未增加监听端口、身份或调度器；与UDP-only listener明确互斥，TLS仍使用原协议。
启停方式及旧二进制回退限制见[运行入口](NOVOVM_RENDEZVOUS_ENTRY.md)。

Windows owner回归12/12通过（14.20秒），真实CLI独立进程1/1最终通过
（65.34秒）：旧协议预寄存、显式开启、同原生会话跨声明换代、进程重启后
原密文/TTL/签名回执保留、显式关闭及原身份/传输公钥保持。首次进程测试
在旧V6预寄存夹具报 `multiplex owner stopped`（7.79秒）；修正夹具的原多路
选择/绑定与owner生命周期后重跑通过，不能写成首次即通过。严格Clippy、
release构建通过；日志 `artifacts/native-carrier-owner-{unit,test,test-final,clippy,release}.log`。

已用原用户计划任务部署并显式开启原节点，部署时PID22224，SHA-256：
`7686ecf478b4004f3a2f07ce5bed93bd0b99e6ed7c521fd748dea9024c6f18ea`。
`artifacts/native-carrier-node-installed.json` 记录原节点密钥/签名身份及选择操作
前后原信箱保管数据保持，签名序号27202→27205并已声明V7；VPN未改。
DPAPI策略中协议选择有意更新，不称其文件字节完全不变。

两台v25手机现已恢复旧V6的L3+L4，尚未切到V7。当前客户端旧
`checkBootstrap` 把同一self-owned节点的声明更新误作信任迁移；NOVOCHAT
正在修正这一接缝，不能据节点发布或本机测试宣称手机原生消息通过。
下一步是原身份/历史下实际V7双向交付和恢复测量；外部Iroh Relay、自主传输、
低延迟及Windows崩溃自动重启仍独立验收。

## 2026-10-17 原信箱宿主接通结构化 Carrier（本机运行验证）

输入64bb8658，认领23150c04。本轮由 NOVOCHAT 设备接通通用宿主，复用原
`product_rendezvous_native` 执行体与同一个 MailboxService/SQLite，不另建
信箱或 SDK。手机 v24 与已部署节点保持原版本，移动调用者尚未切换。

服务端实际调用链：`run_self_owned_node_v1` →
`serve_self_owned_node_selected_v7` → 显式 `NVMBOX07` →
`authenticate_structured_service_client_observed` →
`NativeCarrierPump::attach_continuous_v7` → `NativeObjectLink` →
原 `NativeMailboxServer` 的 jobs/active/pending → 原 MailboxService。
UDP 与 Carrier 复用一个工作循环；Carrier 保留入站已经取得的 permit，
不会为同一连接再次申请额度。未增加后台任务、调度器或账户/许可体系。

客户端在原签名目录中读取 `native_carrier_mailbox_candidates_v7`，通过
`check_native_carrier_route_v7` 核对新连接的完整新鲜路由；连接后通过
`check_continuous_native_carrier_v7` 检查撤销、身份及能力。调用
`request_mailbox_native_v7`，再完成结构化 Finished，随后将原生对象通道
交给既有 NativeMailboxClient/Driver。不能把普通 Iroh 地址当作原生能力，
也不能把一次原生选择失败回退到 JSON。

`NativeCarrierMailboxEndpointV7` 在原 Iroh URI 增加节点签名覆盖的
`/novonet-mailbox/v7`；还是同一个物理入口，最多三个不同载体入口的边界
不变。宿主显式 `rotate_native_carrier_mailbox_v7(true, ...)` 后才声明
此能力，普通任期/地址更新保留它，显式关闭则让受影响连接退出。健康连接
不会因正常声明过期被周期拆除；新连接仍须取得新鲜声明。旧解析器会拒绝
新后缀，因此必须先升级消费者，再协调发布能力，不能直接翻转现用节点。

真实 loopback QUIC 最终回归2项通过（64.03秒）：同一认证连接上的等待通知
与 Put 并行、Fetch/Consume、真实 SQLite 回执、空闲62秒后继续请求、
停服重开原库后密文/期限/回执一致；未声明能力与运行中撤回能力均拒绝。
实际跨越原节点60秒声明换代，在同一认证会话
取得并验签/导入新的 ServiceRecordV7，实例与序号前进而连接身份不变。
宿主首次发布及正常更新时将原已落盘签名记录交给既有信箱快照；客户端
回包、服务端快照、本地目录导入三处都识别显式 Carrier 与 UDP 原生能力。
其中客户端原有UDP-only条件由扩展真实测试捕获并修复，普通Iroh仍被拒绝。
这不是 Android、跨 NAT、外部 Relay 独立性或 E2EE 手机闭环验收。
原有 lane、工作槽、存储及 Carrier scope 资源界限保留；UDP 签名字节预算
尚不是跨全部 Carrier 的节点级全局带宽预算，不作该项已实现声明。

下一片由 NOVOCHAT 原 Application/IO owner 借用持久 Endpoint，替换 L3
业务子流程的 Handle，并保持既有 `run_fast_lane`、`run_local_owner` 和
本机回执队列消费。原生分支当前清空 L4 来源且未运行这些分支，不能只改
发现选路后就发布；L3 载体失败也不能连带取消仍健康的 L4 路径。

## 原对象 owner 的异步载体接线（2026-10-16）

`novorudp::carrier_io::NativeCarrierPump::attach` 消费结构化 Finished 后的原通道，
把实际 CarrierStream 接入原 NativeObjectLink。驱动方在同一 owner 内轮询
`link.poll` 与 `pump.progress`；短暂取消 progress 等待不会丢失半帧 future，
销毁 pump 则终止载体。排队不算写成功，待写帧及配额保留到完成或明确取消。
正文/反馈公平交替，取消中的未知写入不能重用旧流。原UDP同步路径保持。
本机实际QUIC双向两批对象和回归通过，详见生产台账顶部。

原attach保留有限IO契约；新增attach_continuous_v7在现有持续owner内选择原生
分帧：首字节前空闲等待仍检查撤销，首字节后固定30秒完成整帧，不能逐字节续时。
原NativeMailbox业务期限不变。有限scope、过期握手和已选择旧协议的流不能升级；
半帧取消/失败终止载体。没有新增worker、SDK、信箱或调度器。

重要边界：BoundSecureChannel只证明Finished及实际exporter绑定，本身不携带
structured codec或NVMBOX07选择证明；正式入口须显式选择服务并调用结构化握手。
本节当时的QUIC对象收发和TLS分帧测试不是正式信箱或Android接线验收。
上述2026-10-17增量已接通原宿主执行体、入站permit复用和签名原生能力；
原Application owner与两台手机仍待接线验收。

## 结构化载体认证接线（2026-10-16）

`bound_session::authenticate_structured_bound_session_observed` 和
`authenticate_structured_service_client_observed` 进入同一个既有认证状态机，
仅显式选择原生编码；Offer/Response 复用 APFL 批格式（NVBOFR01/NVBRES01），
Finished 复用严格 NVRLY002 Data 二进制封装，不自动探测或回退 JSON。
原节点签名、公钥绑定、真实 TLS exporter、重放缓存及双方 Finished 不变；
本机 Finished 实际写出且对端 Finished 验证后才返回 channel。
旧入口供现有调用者保留，原生入口不能把旧入口当作失败后备。

编码器不是新的身份或权限系统，解码成功不代表认证成功。握手期限仅约束
建立过程，不成为健康会话的周期终止条件。开放服务继续证明密钥持有，无需
固定用户名单；接收者信箱能力、资源边界和撤销检查仍由原业务执行。

本节完成时尚未交付：原生 IO 与 Iroh owner 的实际收发连接、明确服务版本的
加密绑定及原生节点身份对应、旧流取消后的排队数据清理、手机跨 NAT 与独立
副本验证。前几项后续接线见顶部2026-10-17增量；手机与独立副本仍待验证。
仅完成本节认证不能直接放开 NativeRoute::Carrier；原生帧必须继续绑定到
这个已认证流及其 channel，不能改走裸流、伪造 SocketAddr 或本地 UDP 代理。

## 最新分工：手机归 NOVOCHAT 设备，主链归七月恢复设备

用户本轮明确：iPhone/Android 复用同一 NovoRUDP/APFL 协议与 Rust 核心，但
平台封装/构建、NOVOCHAT Application owner 装配、通知/后台/换网及手机实测
交给另一台开发 NOVOCHAT 的设备。本机不接手手机功能，不另造移动 SDK。
两端都沿本仓共享实现，不复制成独立分叉；不能将桌面通过宣称为 iOS/Android 通过。

本机完成本节这笔在途通用重连修复后，回到 **NOV 新创世 → 原生结构化签名交易
→ AOEM 统一代数执行 → 共识耐久发布 → 原生余额/回执查询 → 重启恢复**。
同时验证 NovoRUDP/APFL 真实结构批路径的 TPS/P95/P99：保持真实签名、经济规则、
冲突条件、验证者数量与耐久终点，单列业务失败；检查中途是否又出现 JSON、逐笔
展开或 Host 业务接力。七月高吞吐是复用/对照来源，不是现行最终确认成绩。
不再以移动配套、网络组件数字或旧 HTTP 观察代替主链交付。

### 本次共享接口增量：显式客户端重连 v1

在既有 open-client endpoint 上显式调用 `enable_client_reconnect_v1()`，再装配
原 `NativeObjectLink`。默认及固定验证者 peer 仍不接受此替换。客户端使用原
`connect_client` 重新握手；服务端保留旧连接，直到新源发来有效新会话 AEAD 帧，
才接替原 peer 索引。只有签名 offer 不会断开旧会话。每 peer 最多一个有期限的
待确认握手，重试不延长；未完成/过期不影响旧连接。无握手或 AEAD 格式变更。

同一 poll 返回 `session_changes`，先处理代次事件再消费应用消息；旧分片、DONE
缓存及待发反馈不能进入新代次。旧出站对象以 `ObjectSendState::SessionReplaced`
连同原 Arc 交还，表示结果未知，不是业务失败或回滚。完成仍按 poll quantum
有界交付，额度交还后可明确重试；不要自动重放整个已执行的信箱批次。
`session_validity(peer)` 是只读本地代次句柄，替换/释放 link 后永久失效；
共享 mailbox adapter 已将它接入在途任务检查，原业务权限检查仍必须提供。
已经进入存储提交的操作不会被撤销；按原 Object ID/签名回执恢复未知结果。
已合并 NOVOCHAT 设备的 `try_send_until`，不延长业务期限或恢复失败槽。

移动 owner 应依据该事件退休旧适配器/任务，真实新会话再 attach；旧请求 context
不得重标新 session。这里只实现并验证通用 socket 生命周期，不替手机实现发现、
选路、重试决策或 NAT 穿透。新身份槽仍有原上限，尚无闲置身份回收；已持有输出的
应用必须保留自己的总预算。能力缓冲全面安全擦除、平台构建/实机尚未验收。

本片把七月恢复线 `fb8fc69efbbe01177a24f82ca44b6d1ab569a99e` 已验证的
会话、对象传输与反例迁入共享 `main` 的原 `novovm-network`，不是另建 SDK。
新增显式有界、无需预置对端的应用监听器。原帧、加密、窗口修复、pacing、
身份/源地址绑定、重放检查不改协议。无新依赖，无金融、执行或共识修改。

## 消费者现在可以调用什么

继续从现有 SUPERVM 源码依赖 `novovm-network`，不复制传输源码到 NOVOCHAT。
以下接口无需 `iroh-transport`，也不会连接 HTTP、Iroh 或 Relay：

| 原 crate 内接口 | 用途 |
| --- | --- |
| `novorudp::endpoint::NativeEndpoint::listen_clients` | 原 socket owner 的显式开放密钥持有认证；指定客户端上限和允许的 stream |
| `NativeEndpoint::connect_client` | 复用本机身份/用途密钥，绑定目标节点公钥；不要求验证者身份 |
| `novorudp::object_link::NativeObjectLink::new` | 在同一 socket/session 上装配有界 lane，不另开调度器 |
| `novorudp::fragments::OutgoingMessage::with_profile` | 一次准备结构正文，使用 `FragmentProfile::NativeUdpV2` |
| `NativeObjectLink::try_send / poll / poll_complete` | 非阻塞发送、进度/修复、取回原始二进制 chunks |

完整可编译的公共 API 消费例在
[native_application_transport.rs](../crates/novovm-network/tests/native_application_transport.rs)。
它是外部 integration test，不访问 private 实现、不持有验证者密钥，复用同一
会话完成三轮 12,000 字节双向分片传输，包括超过握手有效期后的连续收发。
测试 stream/domain 是测试值，不能当作已分配的生产信箱协议。

`try_send=false` 表示未接纳，调用者保留同一 immutable handle；
`Delivered` 只是传输对象完成，不能替代信箱持久 `StoreReceipt`、设备回执或
链最终性。`Expired` 显式归还原对象，由原业务 owner 决定重试，不能静默丢工作。
按 node-wide/per-peer byte/message 上限消费；不能建无限重试队列或每消息握手。

空 pinned 列表仍拒绝；只有 `listen_clients` / `allow_clients` 显式开放策略才能
接受未知客户端，且签名/目标/时间/重放检查通过后才占槽。认证不是付费资格、
全网准入或既有链上 UCA 绑定证明；保留原 UCA 及用途密钥，不新造账户。

## 尚未交付，不能冒称已经可上线

- **完整 APFL 信箱接线还没有完成。** 初片传输测试的正文是 opaque binary，
  不把任意 bytes、二进制帧或 JSON 包裹当作 APFL 结构执行。
- 原 `MailboxClientSessionV2` 仍用 CarrierStream、原协议和 JSON 正文；未改动
  正在运行的手机、信箱服务、数据或策略。旧 `NVAPFLV3` 是转账专用，聊天不能
  冒用其经济语义，也不需要逐消息链共识。
- 下一项在此共享 crate 中接原信箱 owner：明确通用结构 envelope/schema 的
  版本和界限，保留 capability、Message ID、TTL、E2EE 密文、StoreReceipt、
  消费/重试语义；与客户端协调显式版本，不在旧 selector 下换字节，不静默降级。
  不能让 NOVOCHAT 自行复制另一套网络/SDK 或修改财务状态。
- 当前客户端槽位 append-only，满额明确拒绝；同 key 换源重连、槽回收/代次隔离
  尚待接入。不得为了移动网络切换清空身份/日志、沿用旧会话 nonce，或假装已支持。
- 原 listener 只解决已知可达 UDP 地址；发现/NAT/Relay 策略与 Android 网络生命周期
  继续由原 owner 适配，不能把本机 UDP 当公网移动可达性。
- Android ARM64 linking、现有 APK 接入、双手机运行均 **NOT EXECUTED**。
  Windows Rust 公共 API 测试不是 Android SDK/主链/TPS/生产签收。

## 本机验证命令

在仓库根目录运行（无需固定盘符）：

```powershell
cargo test --locked -p novovm-network --lib novorudp -- --test-threads=1
cargo test --locked -p novovm-network --lib product_overlay -- --test-threads=1
cargo test --locked -p novovm-network --test native_application_transport -- --test-threads=1
cargo clippy --locked -p novovm-network --lib -- -D warnings
```

主体恢复仍按原路线执行。本片仅把共享、无金融依赖的传输组件提前发布给等待中的
消费者，不能理解为改回“以十月主链为主体”。恢复现场的同名组件应保持与这份
共享实现一致；主链查询草稿不属于本次交付。

## 2026-10-16 增量：结构信箱编码与原 owner 已连接（非完整移动 SDK）

本节更新上述“schema/owner 待接”项，不改其他未完成项。公共接口位于
`novovm-network::mailbox_structured`，当前随原 `MailboxServiceV1` 的
`iroh-transport` feature 编译；**feature 依赖不表示新路径运行时使用 Iroh**。
没有打开任何 HTTP/Iroh/Relay listener，没有改 NVMBOX01–06 的字节或协商。

| 公共接口 | 契约 |
| --- | --- |
| `encode_requests` / `RequestBatch::decode` | 原操作类型的有界 APFL 列编码与借用视图；全部行先验证，后执行 |
| `encode_replies` / `decode_replies` | 原回复与 StoreReceipt 无损映射；context/数量匹配不代替回执验签 |
| `execute_local_batch` | 原 MailboxService 的适配，不是远程准入；调用者在每次 await 前后提供实际 authority 检查 |

NVMBOX07 是新 schema/stream，不将旧 BeginSession/Continuous 票据换标签使用。
每批 1–16 行、8 列、单字段最多 65,536 字节、编码及展开各最多 1,100,000 字节；
Watch 最多4个信箱、10秒。上下文必须非零，远程驱动仍须将它绑定到认证会话和
唯一请求，落实重放与响应关联；一个调用者自填的 context 本身不提供这些保证。
数据层使用 `novovm-protocol::apfl_batch` 的 NVAPFLC1/v1，header/数值小端，
magic/schema 固定，保留字为零；原 NVAPFLV3 转账格式不变。

编码复用从原 Transfer 提取的同一 `Column` 实现：共享列只编码一次、重复值用
首见顺序字典与索引、独立密文无损保留。解码在行数/长度/展开预算内验证规范性，
值借用原缓冲；调用原 owned 信箱接口时才物化该行密文。**不是全程零拷贝、不是
加密密文压缩算法，也不是 AOEM 交易执行计划。** 生产 codec/adapter 不经过 JSON；
测试中的 JSON 只作旧类型的逐字段等价 oracle。

批内操作有序，但不是原子事务；取消或 authority 丢失时，之前存储可能已提交。
不得称回滚或盲重试整个批次。沿原 Object ID/Consume/StoreReceipt 处理未知结果；
Registered、传输 DONE 和 Stored 含义不同，Stored 仍须验证原节点签名及密文摘要。
原 node-wide 限速、能力、TTL、贡献开关、资源可用性、托管责任和消费墓碑均复用。

公共 API 测试新增 `structured_mailbox_over_authenticated_udp_returns_verified_custody`：
本机实际 UDP、同一原生认证会话、结构注册/9KB密文存储/读取，服务端调用同一个
MailboxService，客户端独立验签及篡改拒绝。单元测试另验重启OFF保留托管、消费
墓碑、坏能力、整批预检、提交后权限失效与原ID重试。这不是生产 server/client
驱动：移动重连、活动身份/撤销策略接合、请求重放与响应关联、transport保留缓冲
中的能力擦除、Android ARM64/实机仍未交付。现有手机不要据此切换协议。

```powershell
cargo test --locked -p novovm-protocol apfl_batch
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --lib mailbox_structured -- --test-threads=1
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --test native_application_transport -- --test-threads=1
```

## 2026-10-16 增量：原生认证会话上的实际 SDK 调用链

`mailbox_structured::native_session` 新增可调用适配，不新建 socket 或后台网络运行时。
可编译消费例见 [native_mailbox_session.rs](../crates/novovm-network/tests/native_mailbox_session.rs)。
它替代上一节夹具中手工拼 context、手工匹配回复的做法；仍未接入部署中的手机 owner。

1. 使用原 `NativeEndpoint` 建立并验证同一身份会话；配置 NVMBOX07 `STREAM`、
   `native_session::domain()` 与显式 node-wide/per-peer 收发预算。
2. 认证建立后，`NativeMailboxClient::attach` 校验节点 pin；
   `NativeMailboxServer::attach` 取得同一 peer 的服务接合。客户端和服务角色分别
   认领，同一个 L4 peer 可以提供和消费服务，不是排他客户端/开发者许可证。
3. 客户端 `begin(slot, requests, now)` 只准备有界批；`try_send` false 保留同一待发
   对象。服务端从原 link 的 NVMBOX07 完整消息调用 `admit`，再异步执行
   `MailboxWork::execute`；原 socket owner 在等待存储/Watch时继续轮询 link。
4. `MailboxResponse::try_send` false 时保留原 handle，有额度再试；客户端
   `receive` 校验完整回复、请求操作、数量、摘要及 Put 对应的原 StoreReceipt 签名。
   `transport_complete` 只处理原传输完成/过期，不制造业务成功。

每方向每会话最多4个逻辑请求槽、每槽1个批次，每批原有1–16行/1.1MB上限。
槽各自持有u64序号；context以固定域、认证session、请求者/服务者身份、槽和序号
构造。方向分离，旧会话、旧序号和错误操作回复不能充当当前结果。
`recipient_role` 只作同一lane的请求/回复路由提示，必须继续交给对应适配器完整验证。
原link按peer/stream/role保留有界owner记录，不能丢掉适配器再从序号1重来。
没有每消息握手、链RPC、全网许可或第二账户；本地身份持有仍不是链上UCA绑定证明。

Watch与Put可以占不同槽并行，批内操作仍有序且非原子。30秒是**一次有界业务操作**
的期限，不是监听器寿命或网络许可证；空闲健康会话不按这个时间拆除。
超时、撤销、取消和未知结果锁住受影响槽，不清空日志或静默重试。服务签名key必须
是实际本地传输身份，不能在一个节点会话里发另一个节点的持久回执。
丢回复后按原Object ID明确重试可以读回已提交的回执；不会重新算成未提交。

应用集成仍必须提供原活跃身份/撤销/路径授权检查，持续驱动同一link，并限定节点级
任务与回复队列，不将完成对象转移后遗忘资源预算。该适配没有另建调度器或数据库。
**未完成：**同key换源重连、会话代次回收、transport中全部能力缓冲安全擦除、现有
服务发现/手机owner的正式装配、Android ARM64/实机验证。不得启用旧HTTP/JSON或
Iroh fallback来把这些项冒充原生验收；已有在用协议也没有被本片替换。

```powershell
cargo test --locked -p novovm-network --features iroh-transport,tls-transport,mailbox-seal --test native_mailbox_session -- --test-threads=1
```

## 2026-10-16 消费端合并：只保留共享native_session

另一机器af77af16与本机未推送faf108c5发生请求状态实现重叠。当前仅保留
`native_session`：节点签名回执与请求类型核验、四个并发槽、原link唯一owner
认领均沿共享实现；本机另一份session模块及重复公钥getter已移除，不形成两套SDK。
保留本机`NativeObjectLink::try_send_until`并接到共享请求/回复发送：原对象sender
取业务剩余期限与transport TTL较小者，重入队不延长原期限。没有延长30秒操作
期限或把它变成连接寿命；原认证会话与临时操作仍分开。

移动换源重连/故障槽恢复、在途撤销定向取消、能力缓冲全面擦除和正式Application
接线仍待完成。四个请求槽是当前有界并发，不是聊天历史或累计发送条数上限。

## 2026-10-16 消费端补齐：原发送队列取消传播

`NativeObjectLink::try_send_cancellable_until`在原有队列中保留本地单向取消信号，
`cancel_send`仅接受原Arc对象进行定向取消。`Cancelled`与`SessionReplaced`、
`Expired`都是未知业务结果，必须保留原业务ID及恢复信息。它不是全网许可或新的
认证；不会撤回已经进入内核的包、远端已提交密文或已报告的Delivered。

共享native_session已直接接入：client Pending退出自动通知原发送队列；server
invalidate/drop停止该owner已排队回复。单个response授权/期限检查失败仅取消
该回复并锁住handle，不能通过再次返回Ok复活，也不会取消其他槽。原poll有界
回收额度，原owner需继续poll；远端未完分片保持原TTL与额度，不凭取消删除托管。
所有路径仍使用同一个endpoint、原加密与序号、防重放和StoreReceipt验证。

已通过桌面实际UDP和负向验证。原生平台接线、可达性/手机延迟、全能力缓冲擦除
仍未签收；取消接口不能冒充这些完成，也不支持用旧HTTP/JSON后备填补缺口。

## 2026-10-16 NOVOCHAT 消费方补齐原生信箱目录

`product_service_lease::NativeMailboxEndpointV7` 明确声明既有 NVMBOX07 服务。
通过原 `ServiceLeaseBookV1` 导入/验签/持久化后，调用
`native_mailbox_candidates_v7` 获取绑定公钥与新鲜候选；新拨号调用
`check_native_mailbox_route_v7`，健康会话调用 `check_continuous_native_peer_v7`，
并继续检查 NativeObjectLink 的实际身份/代次和业务能力。这些目录检查本身
不能证明连接在线、安全会话成立或匿名，也不会替代资源准入。

原 `ServiceLeaseHostV1::rotate_native_mailbox_v7` 发布或移除原生入口，签名序号
和撤销沿原持久owner。纯原生记录不再强制携带Iroh地址；混合记录正常轮换不丢
原生候选，旧载体API不把原生地址当自己的地址。载体Udp标签本身不等于信箱版本。

Windows真实UDP8项已从签名目录取得地址再调用原共享SDK；原TLS/QUIC宿主4项
继续通过。实际Android原生监听/拨号、自动路由及蜂窝NAT仍待原平台owner接线；
不把本片当作移动激活。详见生产台账同日目录记录，保留旧一秒清理计时失败。


## 2026-10-16 Node owner native mailbox listener

`product_rendezvous_owned::NativeMailboxListenerV7` now borrows the original
NativeObjectLink/NVMBOX07 codec/session and MailboxService. The same node owner
can attach this listener beside its existing carrier, or use
`run_self_owned_native_mailbox_v7` without constructing an Iroh endpoint.
The caller supplies the protected node key, same stop Arc, same persistent
MailboxService, and original durable ServiceLeaseHost activation/publication.
No new account, schema, JSON compatibility service or financial execution path.

The socket clone only peeks for readiness; NativeEndpoint alone receives/sends.
Concurrent borrowed work and retained responses are jointly bounded. Waiting
Watch is classified only after full APFL validation, limited to two globally
and one per peer; overload returns the existing typed Quota response without
executing rows or resetting sequences. Put/Fetch/Consume retain room for progress.
Same-key session replacement retires old work and responses; committed custody
survives cancellation and must be reconciled by original object identity.

Not yet a deployment release: cumulative native peer identities remain capped
at 64 without reclamation; signed bytes/min is not yet enforced at the original
wire boundary. Shared request quota and object memory bounds do not substitute
for that budget. Windows protected runtime settings, Android Application driver,
real network adaptation and latency remain pending. Keep current installed
carrier unchanged until these hooks are integrated and measured.

## 2026-10-16 Native UDP payload budget on the original owner

`NativeEndpoint::set_wire_budget_v1(bytes_per_minute, Instant)` may be set once,
before the first poll/send. The optional original-SDK budget covers admitted
UDP payload bytes plus successful UDP sends across initial/reconnect handshakes,
data, ACK and repair. Standalone original APIs retain their existing behavior.
The native mailbox listener always installs its explicit resource budget;
`bind_with_wire_budget` supplies it and `bind` defaults to 64 MiB/minute.
`NativeMailboxEndpointV7` preserves this same signed value, rather than losing
it when converting a directory record into a typed route. This is a node-wide
resource ceiling, not per-user admission, billing or a network license.

Accounting uses 61 fixed second buckets and monotonic time, conservatively
covering each rolling 60 seconds. It does not allocate or lock per packet.
A full UDP receive buffer counts oversized datagrams accurately before rejection;
unwanted incoming bytes already reached the socket and cannot be prevented by
application admission. Usage separately exposes received/admitted/sent totals,
receive drops and deferred sends; IP/link headers are not included. Successful
send accounting is not a storage or device delivery receipt. Quota backpressure
does not advance object transmission/repair success or recreate the session.

Resource limits are 2400 bytes through 1 GiB per minute; the low bound allows
only very limited traffic and is not a useful general chat capacity promise.
Changing a signed route does not reset the running socket budget; reconnects
within that owner cannot clear it. Process restart starts a new local window;
this is not durable billing or a global abuse defense. The cumulative 64-peer
identity capacity still needs lifecycle work before an open-network deployment.
Mobile driver integration and real Android native-path latency remain pending.

## 2026-10-16 Shared async mailbox request handles

`mailbox_structured::native_requests::{NativeMailboxDriver, NativeMailboxHandle}`
attach to ONE existing authenticated NativeObjectLink peer. The original owner
must call progress, poll that same link, dispatch transport completions and feed
mailbox replies; wake()/next_deadline() fit its existing IO selection. The adapter
creates no socket/runtime/thread/retry controller. Use the same Application owner,
not another app SDK or task pump.

request() reserves one of four original slots synchronously, including completed
results until consumed. The deadline starts before queueing (at most the original
30-second operation bound); it does not bound the healthy connection lifetime.
Watch and ordinary operations use independent slots. Do not interpret four slots
as a lifetime message limit, or require all four free while Watch is active.

Future cancellation directly marks the original link's retained-send token;
unknown outcomes retain original business identity/TTL and require reconciliation.
A failed active slot stays retired until a new legitimate session, without sequence
reset. Validated late replies are discarded using at most four retired contexts.
Protocol/Authority errors remain typed and cannot become Stopped during teardown.
Transport completion alone never fulfills a business request or device receipt.

Real Windows UDP tests cover concurrent Watch/Put, continuous Fetch, queued expiry,
capacity, active cancellation before/after partial send and other-slot progress.
The production Application/native candidate wiring and Android cross-network
latency are still pending. Test-only local IO pumping is not a second production
scheduler. Current installed phones/node remain unchanged.


## 2026-10-16 增量：原生单 owner 的类型化收发边界

输入 dcc6d1b6，NOVOCHAT335e9d1。NativeEndpoint 内部由直接 UdpSocket
改为同一 owner 的 NativeDatagramIo；原 bind/connect/listen/poll_with_receiver
公开 UDP 入口继续保留。握手、数据、修复和 ACK 都通过同一发送边界及原字节
预算；NativeObjectLink 的调度、重传、取消、会话代次和对象完成语义不变。
没有新线程、代理、SDK 或聊天协议。新 listen_clients_with_io 与
connect_clients_with_io 可接已有平台 IO owner；实现必须有界、非阻塞，提供
完整包和实际来源。WouldBlock 不推进发送；部分写入报告错误，不伪装整包成功。

NativeRoute 将真实 UDP 地址与载体身份分开；VerifiedCarrierRoute 只能由
已有 VerifiedTransportBinding 的本地认证结果派生，不用假的 SocketAddr。
它仅表示本地来源归属：原 NVHSHK01 没有签 TLS exporter，因此当前 Carrier
构造握手/业务会话仍明确拒绝，绝不能把类型化路由当作载体绑定已完成。
下一步复用原 BoundHandshake/Finished 语义并补结构化编码，再由原 Iroh
owner 驱动原生对象流；原 JSON 握手和旧 NVMBOX 不是静默后备。
实际 adapter 还须在部分流写入失败后退役该流、清理旧代次待发内容，并将
节点签名身份与实际载体身份分别验证。未完成这些步骤前不切换现场节点/手机。

上游4d0c16a2..dcc6d1b6另有原主链tick接线与native_query变化；NOVOCHAT
没有调用新增UdpTransport ingress或查询入口。静态核对未见手机调用链冲突，
不由此声称Android运行或主链吞吐已通过。构建及回归结果见生产台账最新条目。

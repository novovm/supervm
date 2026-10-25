# validator-B 阿里云：validator-C 实测连接，部署交还A

交接 `B-ACCESS-2026-10-25`，收件方 validator-A。用户最新决定将部署交回A；C只负责已完成的连接核实和资料交接，不继续安装或启动。覆盖本轮最初由C部署的安排。手机仍叫mobile-A/mobile-B，不混用。

## 已实际核实的连接

| 项 | 结果 |
| --- | --- |
| 公网IP / SSH端口 | `47.103.59.206` / `22` |
| 登录用户 | `ecs-user` |
| C原SSH别名 | `ccsop-test` |
| 服务器hostname | `iZuf69ojo1hbc14c3cvgyqZ` |
| 主机Ed25519指纹 | `SHA256:AKvetVMytykp2K/gMFFkANmTDk82R1iRNiXG+N0lXQE` |
| 认证 | 公钥认证、BatchMode成功、严格主机校验成功；新独立配置也已实连验证 |
| C私钥位置 | `C:/Users/leadb/.ssh/ccsop-test-key-2026.pem`，只告知位置，用户手动安全复制给A，不进Git |
| C连接交接目录 | `C:/Users/leadb/.ssh/validator-B-handoff/`，README、独立config和主机公钥，不含私钥 |

A在自己的用户目录`~/.ssh/`放置用户提供的pem、validator-B.conf及validator-B-known_hosts，执行 `ssh -F ~/.ssh/validator-B.conf -o BatchMode=yes validator-B "hostname; id -un"`。不覆盖原SSH配置，不使用GitHub密钥，不关闭主机校验。A实际认证尚未验证；C成功不代替A成功。

## 云端现状（只读清点）

Ubuntu24.04.4 LTS，Linux6.8.0-136-generic，x86_64，2 vCPU，内存3499MiB、可用约1383MiB，swap4095MiB已用约1815MiB；根盘59GiB、可用20GiB。资源是瞬时值，运行验证者前需限制资源并评估余量，不应在此主机做无约束全量编译。

内网172.16.0.48；NTP同步yes；sudo -n可用。现有novonet-mailbox、supervm-relay、nginx、Docker与KINGCLUB/ccsop容器保持运行，未重启。占用端口含22、80、443、3478、5349、41000、43171/43172/43173及本地应用/数据库端口；不得覆盖。

UDP41050当前没有监听，仅候选端口；未证明安全组或跨机可达，未改防火墙。未发现标准/opt/novovm、/etc/novovm、/var/lib/novovm、/srv/novovm目录；/opt/supervm、/opt/novonet-mailbox及旧暂存包存在，全部保留。未发现novovm验证者systemd单元，/opt/supervm限定搜索未发现node/client；不据有限搜索断言整机无任何副本。当前PATH无cargo/pwsh，未安装工具链。

## 部署状态与A的下一交付

状态 **SSH_VERIFIED / INVENTORIED / WAITING_FOR_SHARED_ACCEPTANCE_SHA_AND_CONFIG**，不是部署成功。当前远端交接要求A固定共同SHA与批准配置；本轮未收到chain_id、最终创世/协议承诺、四节点公钥权重路由及本机合法初始化材料的共同配置包。已向用户询问A是否另有未推送包，不能自己生成另一个创世来宣称部署完成。

请A接收用户手动交付的凭据，先独立验证连接，再负责共同验收SHA/配置、批准端口、原Linux打包及隔离服务安装，核验AOEM/制品与既有服务，按共同阶段启动和查询。C不继续部署。不复制其他节点私钥/数据库；两机互通不等于四节点最终确认。

不启用正式资产与生产收费；没有生成验证者密钥、清库、部署新服务或触碰手机。本报告只含经用户授权共享的连接元数据，不含私钥、口令或完整运行配置。Git通知已发布不等于A已接收。

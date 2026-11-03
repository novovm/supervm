# validator-D：d7ef793b Windows 独立候选接收与本机核验

日期：2026-11-04（Asia/Shanghai）；节点 **validator-D**，实际主机 Windows/ASUS `SUPERVM-L2-01`。
状态：**独立解压、包文件验真、本机运行库资格、独立传输用途身份生成及两次只读 inspect 通过；未做跨机认证，未启链。**

## 接收与固定版本

- 原 main `2bed397463e3d2b2a139b8c8f2043656ced1ba31` 工作区干净，无损快进到 **`fb7451fd467002f5b0294d0026f56a0509fd48e8`**。已取得并阅读指定交接 **`331a55c6c728bd98b53c778de8a9b1eb245fe782`** 的交付对齐顶部2026-11-04指令，以及当前根AGENTS、共同配置、原用途身份准备合同。后继main包含C回执，不把它当D的验收。
- 实际程序源码固定 **`d7ef793b819c8670e472d217daae9db5f074aea8`**，target **`x86_64-pc-windows-msvc`**；本轮没有编译或替换正式库。
- 收到ZIP：`E:/native65-d7ef793b-windows-x86_64-20261104.zip`，**144558493字节**，SHA256与交接一致。
- 独立目录：`C:/Users/ASUS/AppData/Local/NOVOVM/candidates/native65-d7ef793b-windows-x86_64`。解压前确认不存在，核对ZIP路径无越界、符号链接或重复文件；未覆盖、清理旧目录或运行安装器，无管理员权限请求。

| 固定对象 | D实测SHA256（与A公布值一致） |
| --- | --- |
| ZIP | `d025e678f793fffc95a22b21be6b7aeb8d0aa369e5e0ba6df63ba4c06e2238cc` |
| release-manifest.json | `f6caccf81d205974102d34c00300ff10814de5ff13aea8c041d6f7a00cf3eeb4` |
| CHECKSUMS.sha256 | `301e95eb56419e90e882562448d2aedc4c30e0e89343bf0db4669721ce2a09f8` |
| 包内实际AOEM core | `1b764a36c1f782425237d40446d4ea827f56a66fb341b870855d84c15acdce50` |

逐项核对manifest的**80项artifacts**大小及SHA256、根CHECKSUMS的80条和**11个原构建合同**的EXE哈希/包身份。嵌套`aoem/windows/CHECKSUMS.sha256`的**26条**也逐项通过。实际**83个普通文件**为80项制品加3份清单元数据，无无法解释的额外文件。运行库/身份检查之后重新核对全部80项、三份元数据及83个文件集合，均未改变。

## 本机实际运行库资格

只用候选中的`bin/novovmctl.exe native-identity qualify-runtime`，显式传入本候选core路径及固定SHA256；不取PATH旧程序、不改用源码仓库或正式随包DLL。

CLI实际加载路径：`\\?\C:\Users\ASUS\AppData\Local\NOVOVM\candidates\native65-d7ef793b-windows-x86_64\aoem\windows\core\bin\aoem_ffi.dll`。

退出码**0**、`ok=true`；`verification_qualified`、`scalar_verification_kat_passed`、`unified_semantic_verification_kat_passed`、`mixed_batch_verification_kat_passed`全部true，实际路径和库哈希匹配。原报告的`signing_tested=false`、`keygen_tested=false`保留：该入口只证明所列验签运行库资格；后续真实用途keygen/配对检查独立记录，不能混为完整算法或四机验收。

## 本机独立传输用途身份

资格通过后再次确认本域`transport.sealed`及其私有目录均不存在，才沿原`native-identity generate`创建一次D自己的身份。私有目录位于原约定`%LOCALAPPDATA%/NOVOVM/four-host-167999907280566`；仅新建目录关闭ACL继承并允许当前用户、SYSTEM、Administrators，sealed文件ACL也核对仅这些主体。没有更改既有身份或复制A/B/C的sealed材料。

当前准备/inspect账户为 **`SUPERVM-L2-01\ASUS`**，复用原Windows current-user DPAPI。未来服务运行账户尚未配置或验收，不能换账户时静默重建、转移或复制此身份。ACL检查是本目录的有限核对，不代替全机安全审计。

| 字段 | D实际公开结果 |
| --- | --- |
| purpose / chain_id | `transport` / `167999907280566`（仅本次公测传输域，不是正式主网值） |
| algorithm / envelope schema | `ML-DSA-65` / `3` |
| 公钥长度 | **1952字节** |
| 公钥SHA256 | **`30019ff63f5f8c1f77029990aaf2657c3bb6dad562d957d78f0b60d8d349c934`**（原始公钥字节，非hex文本） |
| 完整公钥 | [公开身份JSON的public_key_hex](validator-D-d7ef793b-20261104-identity.json)，可解码为原规范公钥字节 |
| 实际操作 | 一次generate及随后两个独立CLI进程inspect均退出0、ok=true，三次公钥/用途/域一致 |
| inspect不变性 | sealed文件长度6230字节及最后修改时间均不变；没有覆盖或重新generate |
| 已知公钥对照 | 指纹不同于A公布的`0656e6de…01839eae`和C公布的`a3760975…7355b2f`；不是全网独立性证明 |
| 秘密材料导出 | **false**；不提交sealed、私钥、wrapping credential、口令或令牌 |

## 公开证据与给A的回传

以下SHA256对应公开JSON的Git/LF字节；未上传ZIP、DLL或其他大制品：

| 公开证据 | SHA256 |
| --- | --- |
| [包验真](validator-D-d7ef793b-20261104-package.json) | `818d428f0cff26c7d906f87e15807da9ae55d40ace60beed768746979a464b88` |
| [实际运行库资格](validator-D-d7ef793b-20261104-runtime.json) | `833c65c0cd9b162fcca06824a295b1599fe7e3dc1520627a3678a3c2b288669b` |
| [公开身份及操作结果](validator-D-d7ef793b-20261104-identity.json) | `65c78c2437d3aca7d5bd382f9234fa20e01ca0bb4c0c22695a98950726dfc702` |

A请接收本机独立报告与完整公钥，回传`received_commit / report_sha256 / transport_public_key_sha256 / next_authorized_phase`。Git推送不等于A已读。等待A汇总原认证配置后，再按另行指令组织原NovoRUDP双向身份、实际出口、重连及旧会话拒绝验收。

本轮仅D候选接收/本机核验：不编辑共享规则、协议或AOEM，不更改其他机器配置，不重复基础清点、校时或编译；未启动NAT观察、网络守护程序、Windows服务或验证者，未生成validator钥、创世或NOV。未更换既有数据库、正式库、通信/手机身份或B服务。本次不赋予验证者/资产权限，不证明完整抗量子密钥协商、跨机可达性、全链安全、TPS或主网发布资格。

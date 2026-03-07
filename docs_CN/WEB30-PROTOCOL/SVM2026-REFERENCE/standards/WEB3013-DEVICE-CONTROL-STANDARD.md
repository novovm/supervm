# WEB3013: 设备控制接口标准 🦾

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  
**类比**: 执行器/肌肉 - 将 L0 心脏 + AI 大脑的决策转化为物理世界的行动

---

## 核心设计理念

**完整闭环**：
- 👁️ **WEB3012 (感知/输入)**: 传感器采集数据 → 上链
- 🧠 **WEB3011 (决策/处理)**: AI 分析数据 → 生成决策
- 🦾 **WEB3013 (执行/输出)**: 控制设备 → 改变物理世界
- ❤️ **L0 (协调中枢)**: MVCC 并行处理整个流程

### 为什么需要设备控制接口？

| 传统自动化 | WEB3013 链上控制 |
|-----------|-----------------|
| 中心化控制器 | **去中心化执行** |
| 单点故障 | **分布式容错** |
| 黑盒执行 | **链上可审计** |
| 延迟高 | **FastPath <100ms** |
| 无法跨系统 | **跨链互操作** |
| 固定规则 | **AI 自适应控制** |

---

## 架构：感知-决策-执行闭环

```
┌──────────────────────────────────────────────────────┐
│  1️⃣  感知层 (WEB3012 IoT)                           │
│  ┌────────┬────────┬────────┬────────┐             │
│  │ 温度   │ 湿度   │ 光照   │ 运动   │             │
│  │ 传感器 │ 传感器 │ 传感器 │ 检测   │             │
│  └────────┴────────┴────────┴────────┘             │
└──────────────────┬───────────────────────────────────┘
                   │ 数据上链
                   ▼
┌────────────────────────────────────────────────────┐
│  2️⃣  决策层 (WEB3011 AI + L0 MVCC)                │
│  ┌──────────────┬──────────────┬─────────────┐    │
│  │ AI 推理      │ 规则引擎     │ DAO 投票    │    │
│  │ (智能决策)   │ (条件触发)   │ (人工干预)  │    │
│  └──────────────┴──────────────┴─────────────┘    │
└──────────────┬─────────────────────────────────────┘
               │ 控制指令
               ▼
┌────────────────────────────────────────────────────┐
│  3️⃣  执行层 (WEB3013 设备控制)                    │
│  ┌──────────────┬──────────────┬─────────────┐    │
│  │ 空调         │ 灯光         │ 门锁        │    │
│  │ 控制         │ 控制         │ 控制        │    │
│  └──────────────┴──────────────┴─────────────┘    │
└────────────────────────────────────────────────────┘
```

---

## Rust Trait 接口

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WEB3013 设备控制接口核心 Trait
#[async_trait::async_trait]
pub trait WEB3013DeviceControl {
    // ============ 设备注册 ============
    
    /// 注册可控设备
    async fn register_actuator(
        &self,
        device_id: DeviceId,
        device_type: ActuatorType,
        owner: Address,
        capabilities: Vec<Capability>,
        metadata: DeviceMetadata,
    ) -> Result<TransactionHash, ControlError>;
    
    /// 更新设备能力（固件升级后）
    async fn update_capabilities(
        &self,
        device_id: DeviceId,
        new_capabilities: Vec<Capability>,
    ) -> Result<(), ControlError>;
    
    /// 撤销设备控制权限
    async fn revoke_control(
        &self,
        device_id: DeviceId,
        reason: String,
    ) -> Result<TransactionHash, ControlError>;
    
    // ============ 控制指令 ============
    
    /// 发送单条控制指令
    async fn send_command(
        &self,
        device_id: DeviceId,
        command: Command,
        signature: Signature,
    ) -> Result<CommandReceipt, ControlError>;
    
    /// 批量控制（场景模式）
    async fn batch_control(
        &self,
        scene: Scene,
        signature: Signature,
    ) -> Result<Vec<CommandReceipt>, ControlError>;
    
    /// 定时控制
    async fn schedule_command(
        &self,
        device_id: DeviceId,
        command: Command,
        execute_at: u64,  // Unix timestamp
    ) -> Result<ScheduleId, ControlError>;
    
    /// 取消定时任务
    async fn cancel_schedule(&self, schedule_id: ScheduleId) -> Result<(), ControlError>;
    
    // ============ 条件控制 ============
    
    /// 创建自动化规则（If-Then-Else）
    async fn create_automation(
        &self,
        rule: AutomationRule,
    ) -> Result<RuleId, ControlError>;
    
    /// 启用/禁用自动化
    async fn toggle_automation(
        &self,
        rule_id: RuleId,
        enabled: bool,
    ) -> Result<(), ControlError>;
    
    // ============ 状态查询 ============
    
    /// 查询设备当前状态
    async fn get_device_state(
        &self,
        device_id: DeviceId,
    ) -> Result<DeviceState, ControlError>;
    
    /// 查询控制历史
    async fn get_command_history(
        &self,
        device_id: DeviceId,
        start_time: u64,
        end_time: u64,
    ) -> Result<Vec<CommandRecord>, ControlError>;
    
    // ============ 权限管理 ============
    
    /// 授权控制权（分享设备控制权给其他地址）
    async fn grant_permission(
        &self,
        device_id: DeviceId,
        grantee: Address,
        permissions: Vec<Permission>,
        expires_at: Option<u64>,
    ) -> Result<TransactionHash, ControlError>;
    
    /// 撤销授权
    async fn revoke_permission(
        &self,
        device_id: DeviceId,
        grantee: Address,
    ) -> Result<TransactionHash, ControlError>;
    
    /// 检查控制权限
    fn check_permission(
        &self,
        device_id: DeviceId,
        caller: Address,
        command: &Command,
    ) -> Result<bool, ControlError>;
    
    // ============ 跨链控制 ============
    
    /// 跨链设备控制（从 Ethereum 控制 SuperVM 上的设备）
    async fn cross_chain_control(
        &self,
        source_chain: ChainId,
        device_id: DeviceId,
        command: Command,
        proof: CrossChainProof,
    ) -> Result<CommandReceipt, ControlError>;
    
    // ============ 安全与验证 ============
    
    /// 验证指令签名（防止未授权控制）
    fn verify_command(
        &self,
        device_id: DeviceId,
        command: &Command,
        signature: &Signature,
    ) -> Result<bool, ControlError>;
    
    /// 生成控制指令的零知识证明（隐私控制）
    async fn prove_control(
        &self,
        device_id: DeviceId,
        command: Command,
    ) -> Result<ZkProof, ControlError>;
    
    /// 紧急停止（安全机制）
    async fn emergency_stop(
        &self,
        device_id: DeviceId,
        reason: String,
        signature: Signature,
    ) -> Result<(), ControlError>;
    
    // ============ 反馈与监控 ============
    
    /// 订阅设备状态变化
    async fn subscribe_state_changes(
        &self,
        device_id: DeviceId,
        callback: Box<dyn Fn(DeviceState) + Send>,
    ) -> Result<SubscriptionId, ControlError>;
    
    /// 上报执行结果（设备端调用）
    async fn report_execution(
        &self,
        device_id: DeviceId,
        command_id: CommandId,
        result: ExecutionResult,
    ) -> Result<(), ControlError>;
}

// ============ 数据结构 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActuatorType {
    // 家居设备
    AirConditioner,       // 空调
    Light,                // 灯光
    SmartLock,            // 智能锁
    Curtain,              // 窗帘
    Speaker,              // 音箱
    
    // 工业设备
    Motor,                // 电机
    Valve,                // 阀门
    Pump,                 // 泵
    Conveyor,             // 传送带
    Robot,                // 机器人
    
    // 交通设备
    TrafficLight,         // 交通灯
    ParkingGate,          // 停车闸
    ChargingStation,      // 充电桩
    
    // 能源设备
    SolarPanel,           // 太阳能板
    Battery,              // 储能电池
    Generator,            // 发电机
    
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,           // e.g., "set_temperature"
    pub parameters: Vec<Parameter>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: ParamType,
    pub range: Option<Range>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamType {
    Integer,
    Float,
    Boolean,
    String,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: CommandId,
    pub action: String,          // e.g., "set_temperature"
    pub parameters: HashMap<String, String>,
    pub priority: Priority,
    pub timeout: Option<u64>,    // 超时时间（秒）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Normal,
    High,
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub commands: Vec<(DeviceId, Command)>,
    pub delay_between_commands: u64,  // 毫秒
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub name: String,
    pub condition: Condition,
    pub actions: Vec<Action>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    SensorValue {
        device_id: DeviceId,
        metric: String,
        operator: Operator,
        value: f64,
    },
    TimeRange {
        start_hour: u8,
        end_hour: u8,
    },
    DeviceState {
        device_id: DeviceId,
        state: String,
    },
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operator {
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub device_id: DeviceId,
    pub command: Command,
    pub delay: u64,  // 延迟执行（毫秒）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceState {
    pub device_id: DeviceId,
    pub online: bool,
    pub last_update: u64,
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub command_id: CommandId,
    pub device_id: DeviceId,
    pub timestamp: u64,
    pub status: CommandStatus,
    pub tx_hash: TransactionHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandStatus {
    Pending,
    Executing,
    Completed,
    Failed(String),
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub command_id: CommandId,
    pub caller: Address,
    pub timestamp: u64,
    pub command: Command,
    pub result: ExecutionResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub message: String,
    pub data: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Permission {
    Read,              // 只读设备状态
    Control,           // 控制设备
    Admin,             // 管理权限（授权/撤销）
    EmergencyStop,     // 紧急停止
}

pub type DeviceId = [u8; 32];
pub type CommandId = [u8; 32];
pub type RuleId = u64;
pub type ScheduleId = u64;
pub type SubscriptionId = u64;
```

---

## 对比：传统自动化 vs WEB3013

| 特性 | 传统自动化平台 | WEB3013 链上控制 |
|------|---------------|-----------------|
| **执行方式** | 中心化服务器 | ✅ **去中心化执行** |
| **指令审计** | ❌ 无法审计 | ✅ **链上不可篡改记录** |
| **权限管理** | ⚠️ 数据库存储 | ✅ **智能合约执行** |
| **响应速度** | ⚠️ 云端延迟 | ✅ **FastPath <100ms** |
| **跨平台** | ❌ 孤立生态 | ✅ **跨链互操作** |
| **AI 集成** | ⚠️ 需二次开发 | ✅ **原生 WEB3011 集成** |
| **故障容错** | ⚠️ 单点故障 | ✅ **分布式冗余** |
| **隐私控制** | ❌ 明文指令 | ✅ **零知识证明** |

---

## 实现示例 1：智能家居

```rust
use web3013_control::*;

pub struct SmartHome {
    control: Box<dyn WEB3013DeviceControl>,
    ac: DeviceId,
    lights: Vec<DeviceId>,
    lock: DeviceId,
}

impl SmartHome {
    /// 注册所有设备
    pub async fn setup(&self, owner: Address) -> Result<(), ControlError> {
        // 注册空调
        self.control.register_actuator(
            self.ac,
            ActuatorType::AirConditioner,
            owner,
            vec![
                Capability {
                    name: "set_temperature".to_string(),
                    parameters: vec![Parameter {
                        name: "temp".to_string(),
                        param_type: ParamType::Float,
                        range: Some(Range { min: 16.0, max: 30.0 }),
                        default: Some("25.0".to_string()),
                    }],
                    description: "设置目标温度".to_string(),
                },
                Capability {
                    name: "set_mode".to_string(),
                    parameters: vec![Parameter {
                        name: "mode".to_string(),
                        param_type: ParamType::Enum(vec![
                            "cool".to_string(),
                            "heat".to_string(),
                            "auto".to_string(),
                        ]),
                        range: None,
                        default: Some("auto".to_string()),
                    }],
                    description: "设置运行模式".to_string(),
                },
            ],
            DeviceMetadata { /* ... */ },
        ).await?;
        
        Ok(())
    }
    
    /// 创建自动化：温度过高时开启空调
    pub async fn create_auto_cooling(&self) -> Result<RuleId, ControlError> {
        self.control.create_automation(
            AutomationRule {
                name: "自动降温".to_string(),
                condition: Condition::SensorValue {
                    device_id: temp_sensor_id(),  // 从 WEB3012 获取
                    metric: "temperature".to_string(),
                    operator: Operator::Greater,
                    value: 28.0,
                },
                actions: vec![
                    Action {
                        device_id: self.ac,
                        command: Command {
                            id: generate_command_id(),
                            action: "set_temperature".to_string(),
                            parameters: [("temp".to_string(), "24.0".to_string())].into(),
                            priority: Priority::High,
                            timeout: Some(30),
                        },
                        delay: 0,
                    },
                    Action {
                        device_id: self.ac,
                        command: Command {
                            id: generate_command_id(),
                            action: "set_mode".to_string(),
                            parameters: [("mode".to_string(), "cool".to_string())].into(),
                            priority: Priority::High,
                            timeout: Some(30),
                        },
                        delay: 1000,  // 1秒后执行
                    },
                ],
                enabled: true,
            }
        ).await
    }
    
    /// 场景模式：离家模式
    pub async fn leave_home(&self) -> Result<(), ControlError> {
        let scene = Scene {
            name: "离家模式".to_string(),
            commands: vec![
                // 关闭所有灯
                (self.lights[0], Command {
                    id: generate_command_id(),
                    action: "turn_off".to_string(),
                    parameters: HashMap::new(),
                    priority: Priority::Normal,
                    timeout: Some(10),
                }),
                // 关闭空调
                (self.ac, Command {
                    id: generate_command_id(),
                    action: "turn_off".to_string(),
                    parameters: HashMap::new(),
                    priority: Priority::Normal,
                    timeout: Some(10),
                }),
                // 锁门
                (self.lock, Command {
                    id: generate_command_id(),
                    action: "lock".to_string(),
                    parameters: HashMap::new(),
                    priority: Priority::High,
                    timeout: Some(15),
                }),
            ],
            delay_between_commands: 500,  // 每个指令间隔 500ms
        };
        
        let signature = sign_scene(&scene);
        self.control.batch_control(scene, signature).await?;
        
        Ok(())
    }
    
    /// 临时授权：给保洁人员开门权限（2小时后过期）
    pub async fn grant_cleaning_access(&self, cleaner: Address) -> Result<(), ControlError> {
        let expires_at = chrono::Utc::now().timestamp() as u64 + 2 * 3600;
        
        self.control.grant_permission(
            self.lock,
            cleaner,
            vec![Permission::Control],  // 只能控制门锁，无法管理
            Some(expires_at),
        ).await?;
        
        Ok(())
    }
}
```

---

## 实现示例 2：工业自动化

```rust
use web3013_control::*;

pub struct SmartFactory {
    control: Box<dyn WEB3013DeviceControl>,
    conveyor: DeviceId,
    robot_arm: DeviceId,
}

impl SmartFactory {
    /// 生产流水线控制
    pub async fn start_production(&self, product_count: u32) -> Result<(), ControlError> {
        // 1. 启动传送带
        self.control.send_command(
            self.conveyor,
            Command {
                id: generate_command_id(),
                action: "set_speed".to_string(),
                parameters: [("speed".to_string(), "1.5".to_string())].into(),
                priority: Priority::High,
                timeout: Some(5),
            },
            sign_command(),
        ).await?;
        
        // 2. 定时停止（生产完成后）
        let production_time = product_count * 10;  // 每件产品 10 秒
        self.control.schedule_command(
            self.conveyor,
            Command {
                id: generate_command_id(),
                action: "stop".to_string(),
                parameters: HashMap::new(),
                priority: Priority::Normal,
                timeout: Some(5),
            },
            chrono::Utc::now().timestamp() as u64 + production_time as u64,
        ).await?;
        
        // 3. 机械臂协同
        for i in 0..product_count {
            self.control.send_command(
                self.robot_arm,
                Command {
                    id: generate_command_id(),
                    action: "pick_and_place".to_string(),
                    parameters: [
                        ("x".to_string(), "100".to_string()),
                        ("y".to_string(), "200".to_string()),
                        ("z".to_string(), "50".to_string()),
                    ].into(),
                    priority: Priority::High,
                    timeout: Some(10),
                },
                sign_command(),
            ).await?;
            
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        
        Ok(())
    }
    
    /// 紧急停止（安全机制）
    pub async fn emergency_shutdown(&self) -> Result<(), ControlError> {
        let devices = vec![self.conveyor, self.robot_arm];
        
        for device in devices {
            self.control.emergency_stop(
                device,
                "操作员触发紧急停止".to_string(),
                sign_emergency(),
            ).await?;
        }
        
        Ok(())
    }
}
```

---

## 实现示例 3：与 AI 协同（完整闭环）

```rust
use web3011_ai::*;
use web3012_iot::*;
use web3013_control::*;

pub struct AIControlledGreenhouse {
    iot: Box<dyn WEB3012IoT>,
    ai: Box<dyn WEB3011AI>,
    control: Box<dyn WEB3013DeviceControl>,
    temp_sensor: DeviceId,
    humidity_sensor: DeviceId,
    irrigation: DeviceId,
    ventilation: DeviceId,
}

impl AIControlledGreenhouse {
    /// 完整的 AI 驱动自动化流程
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 感知层 (WEB3012)：订阅传感器数据
        self.iot.subscribe(
            self.temp_sensor,
            Box::new(|data: SensorData| {
                tokio::spawn(async move {
                    self.process_sensor_data(data).await;
                });
            }),
        ).await?;
        
        Ok(())
    }
    
    /// 处理传感器数据 → AI 决策 → 设备控制
    async fn process_sensor_data(&self, data: SensorData) -> Result<(), Box<dyn std::error::Error>> {
        // 2. 决策层 (WEB3011)：AI 分析
        let prompt = format!(
            "当前温度 {}°C，湿度 {}%。请决定是否需要调整灌溉和通风。",
            extract_temp(&data),
            await self.get_humidity()
        );
        
        let ai_response = self.ai.infer(
            ModelType::GPT4,
            prompt,
            vec![],
            InferConfig {
                temperature: 0.3,
                max_tokens: 200,
                ..Default::default()
            },
        ).await?;
        
        // 3. 执行层 (WEB3013)：根据 AI 建议控制设备
        if ai_response.output.contains("开启灌溉") {
            self.control.send_command(
                self.irrigation,
                Command {
                    id: generate_command_id(),
                    action: "start".to_string(),
                    parameters: [("duration".to_string(), "300".to_string())].into(),
                    priority: Priority::High,
                    timeout: Some(10),
                },
                sign_command(),
            ).await?;
        }
        
        if ai_response.output.contains("增强通风") {
            self.control.send_command(
                self.ventilation,
                Command {
                    id: generate_command_id(),
                    action: "set_speed".to_string(),
                    parameters: [("level".to_string(), "3".to_string())].into(),
                    priority: Priority::Normal,
                    timeout: Some(10),
                },
                sign_command(),
            ).await?;
        }
        
        Ok(())
    }
}
```

---

## Solidity 兼容层

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IWEB3013DeviceControl {
    // ============ 事件 ============
    
    event ActuatorRegistered(
        bytes32 indexed deviceId,
        uint8 deviceType,
        address indexed owner
    );
    
    event CommandExecuted(
        bytes32 indexed commandId,
        bytes32 indexed deviceId,
        address indexed caller,
        string action,
        uint256 timestamp
    );
    
    event AutomationCreated(
        uint256 indexed ruleId,
        string name,
        bool enabled
    );
    
    event PermissionGranted(
        bytes32 indexed deviceId,
        address indexed grantee,
        uint8[] permissions,
        uint256 expiresAt
    );
    
    event EmergencyStop(
        bytes32 indexed deviceId,
        address indexed caller,
        string reason
    );
    
    // ============ 设备管理 ============
    
    function registerActuator(
        bytes32 deviceId,
        uint8 deviceType,
        string memory metadataURI
    ) external;
    
    function revokeControl(bytes32 deviceId, string memory reason) external;
    
    // ============ 控制指令 ============
    
    function sendCommand(
        bytes32 deviceId,
        string memory action,
        string memory parameters,  // JSON string
        uint8 priority,
        bytes memory signature
    ) external returns (bytes32 commandId);
    
    function scheduleCommand(
        bytes32 deviceId,
        string memory action,
        string memory parameters,
        uint256 executeAt
    ) external returns (uint256 scheduleId);
    
    function cancelSchedule(uint256 scheduleId) external;
    
    // ============ 自动化 ============
    
    function createAutomation(
        string memory name,
        bytes memory conditionData,
        bytes memory actionData
    ) external returns (uint256 ruleId);
    
    function toggleAutomation(uint256 ruleId, bool enabled) external;
    
    // ============ 权限管理 ============
    
    function grantPermission(
        bytes32 deviceId,
        address grantee,
        uint8[] memory permissions,
        uint256 expiresAt
    ) external;
    
    function revokePermission(bytes32 deviceId, address grantee) external;
    
    function checkPermission(
        bytes32 deviceId,
        address caller,
        string memory action
    ) external view returns (bool);
    
    // ============ 查询 ============
    
    function getDeviceState(bytes32 deviceId)
        external view returns (
            bool online,
            uint256 lastUpdate,
            string memory properties
        );
    
    function getCommandHistory(
        bytes32 deviceId,
        uint256 startTime,
        uint256 endTime,
        uint256 limit
    ) external view returns (bytes32[] memory commandIds);
    
    // ============ 安全 ============
    
    function emergencyStop(
        bytes32 deviceId,
        string memory reason,
        bytes memory signature
    ) external;
}
```

---

## 应用场景

### 1. **智能交通**
```rust
// 根据实时车流量自动调整红绿灯
control.create_automation(
    AutomationRule {
        name: "智能红绿灯".to_string(),
        condition: Condition::SensorValue {
            device_id: traffic_camera,
            metric: "vehicle_count".to_string(),
            operator: Operator::Greater,
            value: 50.0,
        },
        actions: vec![Action {
            device_id: traffic_light,
            command: Command {
                action: "extend_green".to_string(),
                parameters: [("duration".to_string(), "30".to_string())].into(),
                ..Default::default()
            },
            delay: 0,
        }],
        enabled: true,
    }
).await?;
```

### 2. **能源管理**
```rust
// 电价低谷时自动充电储能电池
control.create_automation(
    AutomationRule {
        name: "低谷充电".to_string(),
        condition: Condition::TimeRange {
            start_hour: 23,  // 晚上 11 点
            end_hour: 7,     // 早上 7 点
        },
        actions: vec![Action {
            device_id: battery,
            command: Command {
                action: "start_charging".to_string(),
                parameters: [("target_soc".to_string(), "90".to_string())].into(),
                ..Default::default()
            },
            delay: 0,
        }],
        enabled: true,
    }
).await?;
```

### 3. **农业自动化**
```rust
// 土壤湿度低时自动灌溉
control.create_automation(
    AutomationRule {
        name: "自动灌溉".to_string(),
        condition: Condition::SensorValue {
            device_id: soil_moisture_sensor,
            metric: "moisture".to_string(),
            operator: Operator::Less,
            value: 30.0,  // 低于 30%
        },
        actions: vec![Action {
            device_id: irrigation_valve,
            command: Command {
                action: "open".to_string(),
                parameters: [("duration".to_string(), "600".to_string())].into(),  // 10 分钟
                ..Default::default()
            },
            delay: 0,
        }],
        enabled: true,
    }
).await?;
```

---

## 与其他 WEB30 协议的集成

```rust
// 完整闭环示例：IoT → AI → 控制 → 支付

// 1. WEB3012 (感知)：传感器上报能耗数据
iot.submit_data(energy_meter, consumption_data).await?;

// 2. WEB3011 (AI 决策)：分析能耗趋势
let ai_advice = ai.infer(
    ModelType::GPT4,
    format!("本月能耗 {}kWh，分析是否需要优化", total_kwh),
    vec![],
    config
).await?;

// 3. WEB3013 (控制)：调整设备运行策略
if ai_advice.output.contains("降低功率") {
    control.send_command(
        hvac_system,
        Command {
            action: "set_power_limit".to_string(),
            parameters: [("limit".to_string(), "80".to_string())].into(),
            ..Default::default()
        },
        signature,
    ).await?;
}

// 4. WEB30 (支付)：自动支付电费
let bill_amount = calculate_bill(total_kwh);
web30.transfer(electricity_provider, bill_amount).await?;
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | 设备注册与基础控制 | 📋 设计中 |
| **Phase 2** | 自动化规则引擎 | 📋 规划中 |
| **Phase 3** | 场景模式与批量控制 | 📋 规划中 |
| **Phase 4** | 跨链设备控制 | 📋 规划中 |
| **Phase 5** | AI 集成（WEB3011） | 📋 规划中 |
| **Phase 6** | 隐私控制（零知识证明） | 📋 规划中 |

---

## 总结

**WEB3013 = 执行器/肌肉 🦾**

- **完整闭环**: 感知(WEB3012) → 思考(WEB3011+L0) → 执行(WEB3013)
- **去中心化控制**: 链上指令审计，防止单点故障
- **AI 驱动**: 与 WEB3011 深度集成，智能自适应控制
- **跨链互操作**: 从任意链控制 SuperVM 设备

SuperVM 现在拥有：**心脏（L0）+ 大脑（AI）+ 眼睛（IoT 感知）+ 肌肉（设备控制）** = **完整的智能生命体** 🌐🚀

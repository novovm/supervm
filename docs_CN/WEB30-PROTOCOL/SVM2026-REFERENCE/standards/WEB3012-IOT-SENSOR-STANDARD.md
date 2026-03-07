# WEB3012: 物联网感知接口标准 👁️

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  
**类比**: 传感器/神经系统 - 连接 L0 心脏的外界感知层

---

## 核心设计理念

如果 L0 MVCC 内核是 **心脏**，WEB3011 是 **大脑**，那么 WEB3012 就是 **传感器与神经系统**（接收外部世界的实时数据流）。

### 为什么需要 IoT 感知接口？

| 传统 IoT | WEB3012 链上 IoT |
|---------|------------------|
| 中心化云平台 | **去中心化存储** |
| 数据可篡改 | **不可篡改审计** |
| 单点故障 | **分布式容错** |
| 延迟高 | **实时流处理** |
| 无隐私保护 | **环签名隐私** |
| 孤立设备 | **跨链互操作** |

---

## 架构：感知-心脏-大脑协同

```
┌──────────────────────────────────────────────────────┐
│  🌍 外部世界（物理/数字）                             │
│  ┌────────┬────────┬────────┬────────┬────────┐     │
│  │ 温湿度 │ GPS    │ 相机   │ 支付   │ API    │     │
│  │ 传感器 │ 定位   │ 图像   │ 终端   │ 数据源 │     │
│  └────────┴────────┴────────┴────────┴────────┘     │
└──────────────────┬───────────────────────────────────┘
                   │ MQTT/HTTP/WebSocket
                   ▼
┌────────────────────────────────────────────────────┐
│  👁️ WEB3012 IoT 感知接口层                        │
│  ┌──────────────┬──────────────┬─────────────┐    │
│  │ 数据采集     │ 实时流       │ 事件触发    │    │
│  │ (Ingest)     │ (Stream)     │ (Trigger)   │    │
│  └──────────────┴──────────────┴─────────────┘    │
└──────────────┬─────────────────────────────────────┘
               │ 签名验证 + 时间戳
               ▼
┌────────────────────────────────────────────────────┐
│  🧠 WEB3011 AI 大脑（可选）                        │
│      - 实时分析传感器数据                          │
│      - 异常检测与预测                              │
└──────────────┬─────────────────────────────────────┘
               │
               ▼
┌────────────────────────────────────────────────────┐
│  ❤️ L0 MVCC 内核                                   │
│  ┌──────────────┬──────────────┬─────────────┐    │
│  │ 并行写入     │ 跨链同步     │ 隐私保护    │    │
│  │ 495K TPS     │ 设备间交互   │ 零知识证明  │    │
│  └──────────────┴──────────────┴─────────────┘    │
└────────────────────────────────────────────────────┘
```

---

## Rust Trait 接口

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WEB3012 IoT 感知接口核心 Trait
#[async_trait::async_trait]
pub trait WEB3012IoT {
    // ============ 设备注册 ============
    
    /// 注册 IoT 设备
    async fn register_device(
        &self,
        device_id: DeviceId,
        device_type: DeviceType,
        owner: Address,
        metadata: DeviceMetadata,
    ) -> Result<TransactionHash, IoTError>;
    
    /// 更新设备状态
    async fn update_device_status(
        &self,
        device_id: DeviceId,
        status: DeviceStatus,
    ) -> Result<(), IoTError>;
    
    /// 撤销设备（设备丢失/损坏）
    async fn revoke_device(
        &self,
        device_id: DeviceId,
        reason: String,
    ) -> Result<TransactionHash, IoTError>;
    
    // ============ 数据上链 ============
    
    /// 单条数据上链
    async fn submit_data(
        &self,
        device_id: DeviceId,
        data: SensorData,
        signature: Signature,
    ) -> Result<DataHash, IoTError>;
    
    /// 批量数据上链（减少 Gas）
    async fn batch_submit(
        &self,
        device_id: DeviceId,
        data_batch: Vec<SensorData>,
        signature: Signature,
    ) -> Result<Vec<DataHash>, IoTError>;
    
    /// 实时数据流（WebSocket 持续推送）
    async fn stream_data(
        &self,
        device_id: DeviceId,
        stream: Box<dyn Stream<Item = SensorData>>,
    ) -> Result<(), IoTError>;
    
    // ============ 事件订阅 ============
    
    /// 订阅设备数据更新
    async fn subscribe(
        &self,
        device_id: DeviceId,
        callback: Box<dyn Fn(SensorData) + Send>,
    ) -> Result<SubscriptionId, IoTError>;
    
    /// 条件触发器（温度 > 30°C 时触发智能合约）
    async fn create_trigger(
        &self,
        condition: TriggerCondition,
        action: TriggerAction,
    ) -> Result<TriggerId, IoTError>;
    
    /// 取消订阅
    async fn unsubscribe(&self, subscription_id: SubscriptionId) -> Result<(), IoTError>;
    
    // ============ 数据查询 ============
    
    /// 查询历史数据
    async fn query_history(
        &self,
        device_id: DeviceId,
        start_time: u64,
        end_time: u64,
        limit: u32,
    ) -> Result<Vec<SensorData>, IoTError>;
    
    /// 聚合查询（平均温度、最大值等）
    async fn aggregate(
        &self,
        device_id: DeviceId,
        metric: AggregateMetric,
        time_range: TimeRange,
    ) -> Result<f64, IoTError>;
    
    // ============ 跨链同步 ============
    
    /// 跨链同步设备数据（SuperVM → Ethereum/Solana）
    async fn sync_cross_chain(
        &self,
        device_id: DeviceId,
        target_chain: ChainId,
        data: SensorData,
    ) -> Result<CrossChainReceipt, IoTError>;
    
    // ============ 隐私保护 ============
    
    /// 隐私数据上链（环签名 + 加密）
    async fn submit_private(
        &self,
        device_id: DeviceId,
        encrypted_data: Vec<u8>,
        ring_signature: RingSignature,
    ) -> Result<DataHash, IoTError>;
    
    /// 零知识证明（证明数据在某范围内，但不泄露具体值）
    async fn prove_range(
        &self,
        data: f64,
        min: f64,
        max: f64,
    ) -> Result<ZkProof, IoTError>;
    
    // ============ 数据验证 ============
    
    /// 验证数据签名（防止伪造）
    fn verify_signature(
        &self,
        device_id: DeviceId,
        data: &SensorData,
        signature: &Signature,
    ) -> Result<bool, IoTError>;
    
    /// 验证数据完整性（Merkle Proof）
    fn verify_data_integrity(
        &self,
        data_hash: DataHash,
        merkle_proof: MerkleProof,
    ) -> Result<bool, IoTError>;
}

// ============ 数据结构 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    TemperatureSensor,
    HumiditySensor,
    GPS,
    Camera,
    Microphone,
    SmartMeter,         // 智能电表
    POS,                // 支付终端
    Wearable,           // 可穿戴设备
    Drone,              // 无人机
    Vehicle,            // 车辆
    APIDataSource,      // API 数据源（天气/股票等）
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMetadata {
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub firmware_version: String,
    pub location: Option<GeoLocation>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceStatus {
    Active,
    Inactive,
    Maintenance,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub device_id: DeviceId,
    pub timestamp: u64,
    pub data_type: DataType,
    pub value: DataValue,
    pub unit: String,
    pub confidence: f32,  // 数据可信度 0.0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataType {
    Temperature,
    Humidity,
    Location,
    Image,
    Video,
    Audio,
    Payment,
    Energy,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataValue {
    Float(f64),
    Integer(i64),
    String(String),
    Binary(Vec<u8>),
    JSON(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub device_id: DeviceId,
    pub condition: String,  // e.g., "temperature > 30.0"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerAction {
    CallContract {
        contract_address: Address,
        function_name: String,
        params: Vec<String>,
    },
    SendNotification {
        recipients: Vec<Address>,
        message: String,
    },
    CrossChainSync {
        target_chain: ChainId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregateMetric {
    Average,
    Min,
    Max,
    Sum,
    Count,
    StdDev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: u64,
    pub end: u64,
}

pub type DeviceId = [u8; 32];
pub type DataHash = [u8; 32];
pub type SubscriptionId = u64;
pub type TriggerId = u64;
pub type ChainId = u32;
```

---

## 对比：传统 IoT vs WEB3012

| 特性 | 传统 IoT 平台 | WEB3012 链上 IoT |
|------|--------------|-----------------|
| **数据存储** | 中心化数据库 | ✅ **去中心化不可篡改** |
| **数据验证** | ⚠️ 需信任平台 | ✅ **签名验证 + Merkle Proof** |
| **实时性** | ⚠️ 云端延迟 | ✅ **FastPath <100ms** |
| **并发写入** | ⚠️ 数据库瓶颈 | ✅ **MVCC 并行 495K TPS** |
| **跨平台** | ❌ 孤立生态 | ✅ **跨链原生互操作** |
| **隐私保护** | ❌ 明文存储 | ✅ **环签名 + 零知识证明** |
| **事件触发** | ⚠️ 中心化规则引擎 | ✅ **智能合约自动执行** |
| **数据市场** | ❌ 无 | ✅ **数据 NFT 化交易** |

---

## 实现示例：智能温室

```rust
use web3012_iot::*;

pub struct SmartGreenhouse {
    iot: Box<dyn WEB3012IoT>,
    temp_sensor: DeviceId,
    humidity_sensor: DeviceId,
    irrigation_system: DeviceId,
}

impl SmartGreenhouse {
    /// 注册所有传感器
    pub async fn setup(&self, owner: Address) -> Result<(), IoTError> {
        // 注册温度传感器
        self.iot.register_device(
            self.temp_sensor,
            DeviceType::TemperatureSensor,
            owner,
            DeviceMetadata {
                name: "温室温度传感器".to_string(),
                manufacturer: "SensorCo".to_string(),
                model: "TH-2024".to_string(),
                firmware_version: "1.0.0".to_string(),
                location: Some(GeoLocation {
                    latitude: 31.2304,
                    longitude: 121.4737,
                    altitude: Some(10.0),
                }),
                capabilities: vec!["temperature".to_string()],
            },
        ).await?;
        
        // 创建温度过高触发器（自动开启灌溉）
        self.iot.create_trigger(
            TriggerCondition {
                device_id: self.temp_sensor,
                condition: "temperature > 35.0".to_string(),
            },
            TriggerAction::CallContract {
                contract_address: self.irrigation_system.into(),
                function_name: "start_irrigation".to_string(),
                params: vec!["5".to_string()],  // 持续 5 分钟
            },
        ).await?;
        
        Ok(())
    }
    
    /// 实时监控并上链
    pub async fn monitor(&self) -> Result<(), IoTError> {
        // 订阅温度数据
        self.iot.subscribe(
            self.temp_sensor,
            Box::new(|data: SensorData| {
                if let DataValue::Float(temp) = data.value {
                    println!("当前温度: {}°C", temp);
                    
                    // 如果温度异常，触发告警（通过 AI 分析）
                    if temp > 40.0 || temp < 10.0 {
                        tokio::spawn(async move {
                            alert_owner(data).await;
                        });
                    }
                }
            }),
        ).await?;
        
        Ok(())
    }
    
    /// 查询历史数据并生成报告
    pub async fn generate_report(&self) -> Result<String, IoTError> {
        let now = chrono::Utc::now().timestamp() as u64;
        let one_week_ago = now - 7 * 24 * 3600;
        
        // 聚合查询：过去一周平均温度
        let avg_temp = self.iot.aggregate(
            self.temp_sensor,
            AggregateMetric::Average,
            TimeRange { start: one_week_ago, end: now },
        ).await?;
        
        // 最高温度
        let max_temp = self.iot.aggregate(
            self.temp_sensor,
            AggregateMetric::Max,
            TimeRange { start: one_week_ago, end: now },
        ).await?;
        
        Ok(format!(
            "过去7天: 平均温度 {:.1}°C, 最高温度 {:.1}°C",
            avg_temp, max_temp
        ))
    }
}
```

---

## 实现示例：隐私保护的可穿戴设备

```rust
use web3012_iot::*;

pub struct PrivateHealthTracker {
    iot: Box<dyn WEB3012IoT>,
    device_id: DeviceId,
}

impl PrivateHealthTracker {
    /// 上传心率数据（零知识证明：在正常范围内，但不泄露具体值）
    pub async fn submit_heart_rate(&self, heart_rate: f64) -> Result<(), IoTError> {
        // 生成零知识证明：证明心率在 50-150 bpm 范围内
        let proof = self.iot.prove_range(heart_rate, 50.0, 150.0).await?;
        
        // 加密实际数据（只有用户自己能解密）
        let encrypted = encrypt_for_owner(heart_rate.to_string().as_bytes());
        
        // 上链（外界只能验证数据在正常范围，看不到具体值）
        self.iot.submit_private(
            self.device_id,
            encrypted,
            generate_ring_signature(self.device_id),
        ).await?;
        
        Ok(())
    }
    
    /// 保险公司验证：健康指标在范围内（无需查看具体数据）
    pub async fn verify_health_for_insurance(&self, proof: ZkProof) -> Result<bool, IoTError> {
        // 保险公司可以验证用户健康，但看不到具体心率值
        // 保护用户隐私的同时，满足保险合规要求
        Ok(verify_zk_proof(proof))
    }
}
```

---

## Solidity 兼容层

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IWEB3012IoT {
    // ============ 事件 ============
    
    event DeviceRegistered(
        bytes32 indexed deviceId,
        uint8 deviceType,
        address indexed owner,
        uint256 timestamp
    );
    
    event DataSubmitted(
        bytes32 indexed deviceId,
        bytes32 dataHash,
        uint256 timestamp
    );
    
    event TriggerCreated(
        uint256 indexed triggerId,
        bytes32 indexed deviceId,
        string condition
    );
    
    event TriggerFired(
        uint256 indexed triggerId,
        bytes32 indexed deviceId,
        bytes actionData
    );
    
    // ============ 设备管理 ============
    
    function registerDevice(
        bytes32 deviceId,
        uint8 deviceType,
        string memory metadata
    ) external;
    
    function updateDeviceStatus(bytes32 deviceId, uint8 status) external;
    
    function revokeDevice(bytes32 deviceId, string memory reason) external;
    
    // ============ 数据上链 ============
    
    function submitData(
        bytes32 deviceId,
        bytes32 dataHash,
        uint256 timestamp,
        bytes memory signature
    ) external;
    
    function batchSubmit(
        bytes32 deviceId,
        bytes32[] memory dataHashes,
        uint256[] memory timestamps,
        bytes memory signature
    ) external;
    
    // ============ 触发器 ============
    
    function createTrigger(
        bytes32 deviceId,
        string memory condition,
        address targetContract,
        bytes memory callData
    ) external returns (uint256 triggerId);
    
    function removeTrigger(uint256 triggerId) external;
    
    // ============ 查询 ============
    
    function getDeviceInfo(bytes32 deviceId) 
        external view returns (
            address owner,
            uint8 deviceType,
            uint8 status,
            uint256 dataCount
        );
    
    function verifyData(
        bytes32 deviceId,
        bytes32 dataHash,
        bytes memory signature
    ) external view returns (bool);
}
```

---

## 应用场景

### 1. **供应链溯源**
```rust
// 货物从生产到配送全程上链
let track = iot.stream_data(
    gps_device,
    Box::new(stream! {
        loop {
            yield get_current_location().await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    })
).await?;
```

### 2. **智能城市**
```rust
// 交通信号灯根据实时车流量自动调整
iot.create_trigger(
    TriggerCondition {
        device_id: traffic_camera,
        condition: "vehicle_count > 50".to_string(),
    },
    TriggerAction::CallContract {
        contract_address: traffic_light_contract,
        function_name: "extend_green_light".to_string(),
        params: vec!["30".to_string()],
    },
).await?;
```

### 3. **碳排放认证**
```rust
// 工厂能耗数据实时上链，自动计算碳信用
let energy_data = iot.query_history(
    smart_meter,
    start_of_month,
    end_of_month,
    10000
).await?;

let total_kwh = calculate_total(energy_data);
let carbon_credits = calculate_carbon_credits(total_kwh);
mint_carbon_nft(carbon_credits).await?;
```

### 4. **去中心化天气预报**
```rust
// 全球气象站数据聚合（对抗中心化平台）
let global_temp = iot.aggregate(
    weather_network,
    AggregateMetric::Average,
    TimeRange { start: today, end: now },
).await?;
```

---

## 与 L0 内核和 AI 的协同

```rust
// IoT → L0 → AI 完整流程

// 1. IoT 感知层：采集数据
let sensor_data = iot.submit_data(device_id, data, signature).await?;

// 2. L0 内核：并行写入（495K TPS）
let tx_hash = l0_mvcc.write(sensor_data).await?;

// 3. AI 大脑：实时分析
let analysis = ai.infer(
    ModelType::GPT4,
    format!("分析传感器数据: {:?}，预测下一小时趋势", sensor_data),
    vec![],
    config
).await?;

// 4. AI 决策触发 IoT 动作
if analysis.output.contains("温度过高") {
    iot.create_trigger(
        TriggerCondition { device_id, condition: "temperature > 35.0".to_string() },
        TriggerAction::CallContract { /* ... */ }
    ).await?;
}
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | 设备注册与数据上链 | 📋 设计中 |
| **Phase 2** | 实时数据流处理 | 📋 规划中 |
| **Phase 3** | 事件触发器系统 | 📋 规划中 |
| **Phase 4** | 跨链 IoT 同步 | 📋 规划中 |
| **Phase 5** | 隐私保护（零知识证明） | 📋 规划中 |
| **Phase 6** | 数据市场（NFT 化） | 📋 规划中 |

---

## 总结

**WEB3012 = 传感器/神经系统 👁️**

- **与 L0 心脏配合**：MVCC 并行处理海量 IoT 数据
- **与 AI 大脑协同**：实时数据分析与智能决策
- **去中心化感知**：设备数据不可篡改、可验证
- **隐私保护**：零知识证明 + 环签名

SuperVM 有了心脏（L0）、大脑（AI）、神经系统（IoT），形成**完整的智能有机体** 🌐🚀

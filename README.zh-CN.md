# VectorOS

[English](README.md) | [中文](README.zh-CN.md)

---

基于 VPP (Vector Packet Processing) 的高性能开源路由器系统。

## 特性

- **高性能数据面**：基于 VPP 的用户态报文处理，DPDK/RDMA 驱动
- **PPPoE 拨号**：内置 PPPoE Client 插件，支持 CHAP/PAP 认证、VLAN/QinQ、断线重连、指数退避
- **现代控制面**：Rust (tokio + axum) 编写，内存安全、高性能
- **Web 管理界面**：Svelte + Tailwind CSS，暗色主题仪表盘
- **路由协议**：FRRouting 集成，支持 BGP/OSPF (via FPM)
- **网络服务**：DHCP、DNS、NAT

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│              Frontend (Svelte + Tailwind)                   │
│         仪表盘 / 接口管理 / 路由 / DHCP / DNS               │
├─────────────────────────────────────────────────────────────┤
│            Control Plane (Rust + Axum)                      │
│     REST API / 配置管理 / DHCP / DNS / 状态采集              │
│                    ↓ Binary API Socket                      │
├─────────────────────────────────────────────────────────────┤
│              VPP Data Plane (C, DPDK)                       │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  pppoeclient 插件 (3层架构)                           │   │
│  │  PPPoE 发现 → PPPoX 桥接 → 嵌入式 pppd               │   │
│  └──────────────────────────────────────────────────────┘   │
│  NAT / 路由 / 接口管理 / VLAN / QinQ                       │
├─────────────────────────────────────────────────────────────┤
│              FRRouting (BGP/OSPF) via FPM                   │
├─────────────────────────────────────────────────────────────┤
│              Linux + DPDK PMD / RDMA                        │
└─────────────────────────────────────────────────────────────┘
```

## 项目结构

```
vectoros/
├── Cargo.toml                      # Rust workspace
├── control-plane/                  # Rust 控制面
│   └── src/
│       ├── main.rs                 # 入口
│       ├── api/                    # REST API
│       ├── config/                 # 配置管理
│       ├── vpp/                    # VPP Binary API 客户端
│       └── services/               # DHCP, DNS 等服务
├── frontend/                       # Svelte 前端
│   └── src/routes/                 # 页面路由
├── vpp/                            # VPP 源码 (git submodule)
│   └── src/plugins/pppoeclient/    # PPPoE Client 插件
└── vpp-plugins/                    # 额外 VPP 插件
```

## 快速开始

### 环境要求

- Linux kernel 4.19+
- DPDK 兼容网卡
- Rust 1.70+
- Node.js 18+

### 克隆仓库

```bash
git clone --recursive https://github.com/Hi-Jiajun/vectoros.git
cd vectoros
```

如果已经 clone 但没有子模块：

```bash
git submodule update --init
```

### 编译控制面

```bash
cargo build --release
```

### 编译前端

```bash
cd frontend
npm install
npm run build
cd ..
```

### 运行

```bash
sudo ./target/release/vectoros --config config.toml
```

## API 接口

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/health` | GET | 健康检查 |
| `/api/config` | GET | 获取当前配置 |
| `/api/interfaces` | GET | 网络接口列表 |
| `/api/routes` | GET | 路由表 |
| `/api/dhcp/leases` | GET | DHCP 租约列表 |

## 配置文件

配置路径：`/etc/vectoros/config.toml`

```toml
[vpp]
socket_path = "/run/vpp/api.sock"

[network]
wan_interface = "eth0"
lan_interface = "eth1"

[network.pppoe]
username = "user"
password = "pass"
interface = "eth0"

[dhcp]
enabled = true
range_start = "192.168.1.100"
range_end = "192.168.1.200"
lease_time = 86400

[dns]
upstream = ["8.8.8.8", "1.1.1.1"]
cache_size = 1000
```

## 贡献

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/xxx`)
3. 提交更改
4. 推送到分支 (`git push origin feature/xxx`)
5. 创建 Pull Request

## 许可证

Apache-2.0

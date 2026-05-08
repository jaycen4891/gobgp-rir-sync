# gobgp-sync

GoBGP 路由同步服务 — 根据国家代码从 RIR 数据库自动同步 IP 前缀到 GoBGP。

## 环境要求

- Linux x86_64 或 macOS arm64
- GoBGP 服务已启动并运行（gobgpd）

## 使用方法

### 命令行

```bash
# 最基本的用法
./gobgp-sync

# 指定国家代码和 IP 版本
./gobgp-sync -C CN -i dual

# 全部路由，每天凌晨 3 点同步
./gobgp-sync -C ALL -s 03:00 -l /var/log/gobgp_sync.log

# 使用配置文件
./gobgp-sync -c config.toml

# 查看完整参数
./gobgp-sync --help
```

### 配置参数

| 参数                 | 短参数 | 说明                                  | 默认值                 |
| -------------------- | ------ | ------------------------------------- | ---------------------- |
| `--ip-version`       | `-i`   | IP 协议版本: `ipv4`, `ipv6`, `dual`   | `DUAL`                 |
| `--country`          | `-C`   | 国家代码，特殊值 `ALL` / `NONECN`     | `CN`                   |
| `--sync-time`        | `-s`   | 每日同步时间 (HH:MM)                  | `02:00`                |
| `--gobgp-api-host`   |        | GoBGP gRPC API 地址                   | `127.0.0.1`            |
| `--gobgp-api-port`   |        | GoBGP gRPC API 端口                   | `50051`                |
| `--gobgp-nexthop-ipv4` |      | 注入 IPv4 路由时传给 GoBGP 的下一跳   | `0.0.0.0`              |
| `--gobgp-nexthop-ipv6` |      | 注入 IPv6 路由时传给 GoBGP 的下一跳   | `::`                   |
| `--community-nexthop-ipv4` |  | 按国家/地区简写覆盖 IPv4 下一跳，格式 `CN=198.19.0.254` |                        |
| `--community-nexthop-ipv6` |  | 按国家/地区简写覆盖 IPv6 下一跳，格式 `CN=2001:db8::fe` |                        |
| `--region-community-prefix` | | 按 RIR 地区覆盖团体字前缀，格式 `RIPE=65167` |                        |
| `--region-nexthop-ipv4` |    | 按 RIR 地区覆盖 IPv4 下一跳，格式 `RIPE=198.19.1.254` |                        |
| `--region-nexthop-ipv6` |    | 按 RIR 地区覆盖 IPv6 下一跳，格式 `RIPE=2001:db8:1::fe` |                        |
| `--log-file`         | `-l`   | 日志文件路径                          | `./gobgp_sync.log`     |
| `--snapshot-dir`     | `-d`   | 快照文件目录                          | `/tmp`                 |
| `--community-prefix` |        | 团体字前缀，如 `3166` 生成 `3166:156` | `3166`                 |
| `--concurrency`      |        | 并发添加/删除路由的任务数             | `100`                  |
| `--config`           | `-c`   | TOML 配置文件路径                     |                        |

> **说明**：程序自带定时调度，首次启动立即执行一次，之后按 `--sync-time` 指定的时间每日自动同步，不需要额外配置 cron。

### 地区团体字与下一跳

TOML 配置支持按 RIR 地区覆盖团体字前缀和下一跳。地区名称为 `APNIC`、`RIPE`、`ARIN`、`LACNIC`、`AFRINIC`。程序仍然按国家/地区生成团体字后半部分，例如 CN 为 `156`；只是在生成前缀时先看该国家所属 RIR 是否配置了地区前缀。

```toml
[settings]
community_prefix = "3166"

[settings.region_community_prefix]
APNIC = "65166"
RIPE = "65167"
ARIN = "65168"
LACNIC = "65169"
AFRINIC = "65170"

[settings.community_nexthop_ipv4]
CN = "198.19.0.254"

[settings.region_nexthop_ipv4]
RIPE = "198.19.1.254"
```

下一跳匹配优先级为：国家/地区简写覆盖、RIR 地区覆盖、默认下一跳。快照文件存在且为当天时，程序会查询 GoBGP Global RIB，只追加快照中存在但 GoBGP 中缺失的路由。

不使用 TOML 时，也可以直接用二进制参数配置地区规则：

```bash
./gobgp-sync \
  --region-community-prefix APNIC=65166 \
  --region-community-prefix RIPE=65167 \
  --region-community-prefix ARIN=65168 \
  --region-community-prefix LACNIC=65169 \
  --region-community-prefix AFRINIC=65170 \
  --region-nexthop-ipv4 RIPE=198.19.1.254
```

---

## systemd 服务配置

将 gobgp-sync 注册为 systemd 服务，实现开机自启和自动管理。

### 1. 放置二进制文件

```bash
sudo cp target/x86_64-unknown-linux-musl/release/gobgp-sync /usr/local/bin/gobgp-sync
sudo chmod +x /usr/local/bin/gobgp-sync
```

### 2. 创建 systemd service 文件

```ini
# /etc/systemd/system/gobgp-sync.service
[Unit]
Description=GoBGP Route Sync Service
After=network-online.target gobpgd.service
Wants=network-online.target
Requires=gobgpd.service

[Service]
Type=simple
WorkingDirectory=/etc/gobgp-sync
ExecStart=/etc/gobgp-sync/gobgp-sync -c /etc/gobgp-sync/config.toml
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal

# 安全配置
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/etc/gobgp-sync /var/log/gobgp

[Install]
WantedBy=multi-user.target
```

### 3. 创建必要目录

```bash
sudo mkdir -p /var/lib/gobgp-sync /var/log/gobgp
```

### 4. 启动服务

```bash
sudo systemctl daemon-reload
sudo systemctl enable gobgp-sync
sudo systemctl start gobgp-sync
```

### 5. 查看状态与日志

```bash
# 查看服务状态
sudo systemctl status gobgp-sync

# 查看实时日志
sudo journalctl -u gobgp-sync -f

# 查看最近 100 行
sudo journalctl -u gobgp-sync -n 100
```

### 自定义参数

如需修改同步参数，编辑 service 文件中的 `ExecStart` 行：

```ini
ExecStart=/usr/local/bin/gobgp-sync -C ALL -s 03:00 -i ipv4 -l /var/log/gobgp/gobgp_sync.log
```

如 GoBGP API 不在本机默认端口，可加上：

```ini
ExecStart=/usr/local/bin/gobgp-sync --gobgp-api-host 10.64.129.53 --gobgp-api-port 50051 -C CN -i dual
```

需要向邻居通告指定下一跳时，可同时设置：

```ini
ExecStart=/usr/local/bin/gobgp-sync --gobgp-nexthop-ipv4 10.64.129.53 --gobgp-nexthop-ipv6 2001:db8::1 -C CN -i dual
```

也可以只针对某个国家/地区简写覆盖下一跳。程序会把简写转换成团体字后半部分，例如 `CN` 会转换成 `156`，并匹配 `3166:156`。TOML 配置还可以按 RIR 地区设置下一跳，国家/地区简写覆盖优先级更高：

```ini
ExecStart=/usr/local/bin/gobgp-sync --community-prefix 3166 --community-nexthop-ipv4 CN=198.19.0.254 --community-nexthop-ipv6 CN=2001:db8::fe -C CN -i dual
```

修改后重新加载：

```bash
sudo systemctl daemon-reload
sudo systemctl restart gobgp-sync
```

---

## 构建

```bash
# macOS (arm64)
cargo build --release

# Linux x86_64 (静态链接, musl)
cargo build --target x86_64-unknown-linux-musl --release
```

---

## 手动验证 GoBGP API

项目提供了 `examples/gobgp_route_test.rs`，用于手动验证 GoBGP gRPC API 添加和删除路由。

默认参数：

- API: `http://10.64.129.53:50051`
- 前缀: `1.1.1.1/32`
- 下一跳: `198.19.0.254`
- 团体字: `3166:156`

添加路由：

```bash
cargo run --example gobgp_route_test -- --action add
```

删除路由：

```bash
cargo run --example gobgp_route_test -- --action del
```

先添加再删除：

```bash
cargo run --example gobgp_route_test
```

自定义参数：

```bash
cargo run --example gobgp_route_test -- \
  --api http://127.0.0.1:50051 \
  --prefix 1.1.1.1/32 \
  --next-hop 198.19.0.254 \
  --community 3166:156 \
  --action both
```

说明：添加时会携带团体字，删除时只携带前缀和下一跳，不携带团体字，便于验证生产删除逻辑是否能正常移除路由。

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
| `--gobgp-path`       | `-g`   | gobgp 可执行文件路径                  | `/usr/local/bin/gobgp` |
| `--log-file`         | `-l`   | 日志文件路径                          | `./gobgp_sync.log`     |
| `--snapshot-dir`     | `-d`   | 快照文件目录                          | `./`                   |
| `--community-prefix` |        | 团体字前缀，如 `3166` 生成 `3166:156` | `3166`                 |
| `--concurrency`      |        | 并发添加/删除路由的任务数             | `100`                  |
| `--config`           | `-c`   | TOML 配置文件路径                     |                        |

> **说明**：程序自带定时调度，首次启动立即执行一次，之后按 `--sync-time` 指定的时间每日自动同步，不需要额外配置 cron。

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

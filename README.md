# Bugutv Check - Rust版本

这是一个用 Rust 编写的自动签到脚本，用于 [布谷TV](https://www.bugutv.vip) 网站。

## 功能

- 自动登录布谷TV网站
- 获取当前积分
- 执行每日签到
- 显示签到结果和积分变化
- 安全退出登录

## 使用方法

### 本地运行

1. 设置环境变量:
   ```bash
   export BUGUTV_USERNAME=你的用户名
   export BUGUTV_PASSWORD=你的密码
   ```

2. 运行程序:
   ```bash
   cargo run
   ```

### Docker运行

1. 构建Docker镜像:
   ```bash
   docker build -t bugutv-check-rust .
   ```

2. 运行容器:
   ```bash
   docker run -e BUGUTV_USERNAME=你的用户名 -e BUGUTV_PASSWORD=你的密码 bugutv-check-rust
   ```

## 环境变量

- `BUGUTV_USERNAME`: 布谷TV账户用户名
- `BUGUTV_PASSWORD`: 布谷TV账户密码

## 依赖

- Rust 1.70+
- reqwest: HTTP客户端
- tokio: 异步运行时
- scraper: HTML解析
- regex: 正则表达式
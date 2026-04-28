FROM node:22-alpine AS admin-ui

WORKDIR /ui
COPY admin-ui/package.json admin-ui/pnpm-lock.yaml ./
RUN corepack enable && corepack prepare pnpm@10.33.0 --activate && pnpm install --frozen-lockfile
COPY admin-ui ./
RUN pnpm run build

FROM rust:trixie AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./

# 创建一个虚拟的main.rs以优化Docker层缓存
RUN mkdir -p src/jobs && echo "fn main() {}" > src/main.rs

# 构建依赖项（利用Docker层缓存）
RUN cargo build --release
RUN rm -f target/release/deps/job_scheduler*

# 复制真实的源代码并构建
COPY src ./src
RUN cargo build --release

# 使用glibc为基础的运行时环境以确保Rust二进制文件兼容性
FROM debian:trixie-slim
# 安装ca-certificates以支持HTTPS请求
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/job_scheduler .
COPY --from=admin-ui /ui/dist ./admin-ui/dist

# 使用非root用户运行应用以增强安全性
RUN useradd -m -U scheduler && chown -R scheduler:scheduler /app
USER scheduler

EXPOSE 8080

# 默认运行 admin，compose 中的 worker 服务会覆盖 command
CMD ["./job_scheduler", "admin"]

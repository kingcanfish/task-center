FROM rust:1.89.0-trixie AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./

# 创建一个虚拟的main.rs以优化Docker层缓存
RUN mkdir src && echo "fn main() {}" > src/main.rs

# 构建依赖项（利用Docker层缓存）
RUN cargo build --release
RUN rm -f target/release/deps/bugutv_check*

# 复制真实的源代码并构建
COPY src ./src
RUN cargo build --release

# 使用glibc为基础的运行时环境以确保Rust二进制文件兼容性
FROM debian:trixie-slim
# 安装ca-certificates以支持HTTPS请求
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /root/

# 从构建阶段复制二进制文件
COPY --from=builder /app/target/release/bugutv_check .

# 使用非root用户运行应用以增强安全性
RUN useradd -m -U bugutv && chown bugutv:bugutv ./bugutv_check
USER bugutv

# 运行应用
CMD ["./bugutv_check"]
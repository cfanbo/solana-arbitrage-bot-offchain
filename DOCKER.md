# Docker Deployment Guide

## Setup Docker Hub Integration

### 1. Create Docker Hub Access Token

1. 登录到 [Docker Hub](https://hub.docker.com)
2. 点击右上角头像 → Account Settings
3. 点击 Security → New Access Token
4. 创建名为 `GitHub Actions` 的 token
5. 复制生成的 token（只显示一次）

### 2. Configure GitHub Secrets

在 GitHub 仓库中设置以下 secrets：

1. 进入仓库 → Settings → Secrets and variables → Actions
2. 点击 "New repository secret"
3. 添加以下 secret：
   - Name: `DOCKER_HUB_TOKEN`
   - Value: 刚才创建的 Docker Hub access token

### 3. Docker Build Optimizations

Dockerfile 使用了以下优化：
- **多阶段构建**: 减少最终镜像大小
- **依赖缓存**: 先复制真实的 lib.rs 和配置文件，用临时 main.rs 构建依赖
- **安全运行**: 使用非 root 用户运行应用

#### 构建流程说明：
1. **依赖构建阶段**: 复制 Cargo.toml、build.rs 和 src/lib.rs，用临时 main.rs 构建依赖
2. **应用构建阶段**: 复制真实源码，重新构建应用二进制
3. **运行时阶段**: 只包含运行时需要的文件

这种方式确保：
- ✅ 使用真实的模块结构 (lib.rs)
- ✅ build.rs 脚本正常运行
- ✅ 依赖层有效缓存
- ✅ 避免复杂的第三方工具

### 4. Workflow Triggers

工作流会在以下情况触发：
- Push to main 分支
- 创建版本标签 (v*)
- Pull Request

### 4. Image Tags

生成的 Docker 镜像将使用以下标签：
- `cfanbo/solana-arbitrage-bot:latest` (main 分支)
- `cfanbo/solana-arbitrage-bot:v1.0.0` (版本标签)
- `cfanbo/solana-arbitrage-bot:main-<sha>` (带 commit SHA)

## Local Docker Usage

### Build locally:
```bash
docker build -t solana-arbitrage-bot .
```

### Run with config:
```bash
# 首先创建配置文件
cp config.example.toml config.toml
# 编辑 config.toml

# 运行容器
docker run -v $(pwd)/config.toml:/app/config.toml solana-arbitrage-bot
```

### Run with environment-based config:
```bash
docker run -e RUST_LOG=debug \
  -v $(pwd)/config.toml:/app/config.toml \
  cfanbo/solana-arbitrage-bot:latest
```

## Production Deployment

### Using Docker Compose:
```yaml
version: '3.8'
services:
  arbitrage-bot:
    image: cfanbo/solana-arbitrage-bot:latest
    restart: unless-stopped
    volumes:
      - ./config.toml:/app/config.toml:ro
    environment:
      - RUST_LOG=info
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"
```

### Security Notes:
- 镜像使用非 root 用户运行
- 只包含必要的运行时依赖
- 配置文件通过 volume mount 提供
- 不在镜像中包含敏感信息
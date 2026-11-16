# Sluice

[![Docker](https://github.com/akioruri/sluice/actions/workflows/docker.yml/badge.svg)](https://github.com/akioruri/sluice/actions/workflows/docker.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Docker pulls](https://img.shields.io/badge/ghcr-ghcr.io%2Fakioruri%2Fsluice-blue)](https://github.com/akioruri/sluice/pkgs/container/sluice)

[English](README.md) · **中文**

自托管、透传式 LLM API 网关。一个 Rust 二进制文件，在多个上游模型厂商之前提供统一入口，厂商密钥只存在服务端，对外只暴露一个统一的访问凭证。

## 典型用法

- **密钥集中管理** —— 所有厂商 Key 只放在服务端，对外发一个 token，CLI agent、编辑器、脚本、队友共用
- **无感轮换** —— 吊销或重发客户端 token，不必动任何一家厂商的后台
- **给团队一个入口** —— 所有客户端配置里只填一个 `base_url`，而不是每人一份厂商 Key
- **国内免翻墙调用国外模型** —— 把 Sluice 部署在能直连厂商的机器上（家里有外网的设备，或一台海外 VPS），只把它的端口映射出去；客户端不需要任何代理，`base_url` 指向你的域名即可正常用 Claude、GPT 等模型

## 为什么需要它

你手上有多个模型厂商的 API Key，并且希望：

- 只对外发放**一个**凭证，而不是每个厂商一个
- 轮换或吊销该凭证时不必惊动各厂商
- 看到谁在调用什么、调用了多少

Sluice 按路径前缀路由，替换成厂商自己的 Key，并把响应逐字节流式回传。

## 特性

- **字节级透传** —— 请求体、请求头（鉴权头除外）、查询参数原样转发
- **前缀路由** —— `/<vendor>/<path>` 选择上游
- **密钥集中管理** —— 厂商密钥仅存服务端，落库前加密；客户端持有统一 token
- **SSE 流式回传** —— 响应分块转发，不做整体缓冲
- **模型白名单** —— 可限制某个供应商只能使用指定模型
- **用量统计** —— 管理后台提供按供应商、按密钥的每日折线图
- **网页管理后台** —— 供应商、API 密钥、统计全部在浏览器中管理
- **多语言** —— English、中文、日本語，跟随浏览器语言

## 设计取向

同类网关大多会把请求解析、改写成一个统一 schema。Sluice 刻意不这么做，取舍如下：

|                 | Sluice                               | 改写型网关                     |
| --------------- | ------------------------------------ | ------------------------------ |
| 请求体          | 原样转发，逐字节                     | 解析后按统一 schema 重新序列化 |
| 新厂商 / 新模型 | 无需改动，立刻可用                   | 要等上游适配                   |
| 响应格式        | 厂商返回什么就是什么                 | 归一化，可能丢掉厂商特有字段   |
| 流式            | 分块转发，不缓冲                     | 常为重新分块而整体缓冲         |
| 运行时          | 单个静态 Rust 二进制，无 Python      | 通常是 Python 服务加一堆依赖   |
| 凭据            | AES-256-GCM 落库加密，token 只存哈希 | 视实现而定                     |
| 成本            | 自托管，费用只有厂商本身             | 托管型还会加一层差价           |

代价是：没有自动重试、没有厂商无关的统一响应 schema、没有按模型的限流。需要这些的话，改写型网关更合适；如果你希望厂商 API 严格按文档表现、同时少一个存放密钥的地方，选 Sluice。

![管理后台](docs/screenshot-console.png)

<!-- 截图 TODO：截一张管理后台（供应商列表、创建密钥、用量折线图），
     保存为 docs/screenshot-console.png。一段流式请求的 GIF 比文字更能打动读者。 -->

## 工作原理

```
客户端 ──POST /anthropic/v1/messages──▶ Sluice
        Authorization: Bearer <统一 token>
                                          │
                    1. 校验 token ────────┤
                    2. 解析供应商 ────────┤
                    3. 解密厂商密钥 ──────┤
                    4. 替换鉴权头 ────────┘
                                          │
                          POST https://api.anthropic.com/v1/messages
                          x-api-key: <厂商密钥>
                                          │
客户端 ◀────────── 流式响应 ───────────────┘
```

路径第一段是供应商名，剩余部分拼接到该供应商配置的上游地址后面。因此
`/openai/v1/chat/completions` 会变成 `https://api.openai.com/v1/chat/completions`。

## 使用 Docker 快速开始

镜像会同时构建前端和 Rust 服务，宿主机无需安装任何依赖。推送 `v*` 标签会把镜像
发布到 GitHub Container Registry（`ghcr.io/akioruri/sluice`，
同时构建 `linux/amd64` 与 `linux/arm64`），不想本地构建的话，直接拉取
`ghcr.io/akioruri/sluice:latest`。`latest` 只跟随最新的
`v*` 标签，不会指向未打标签的提交。否则请先构建镜像
（见[从源码构建](#从源码构建)），再运行：

```bash
docker run -d --name sluice \
  -p 127.0.0.1:8080:8080 \
  -e SLUICE_BIND_ADDR=0.0.0.0:8080 \
  -e DATABASE_URL='postgres://sluice:sluice@host.docker.internal:5432/sluice' \
  -e SLUICE_MASTER_KEY="$(openssl rand -base64 32)" \
  -e SLUICE_ADMIN_PASSWORD='change-me' \
  ghcr.io/akioruri/sluice:latest

# 验证
curl -s http://127.0.0.1:8080/healthz          # -> ok
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/   # -> 200
```

然后打开 <http://localhost:8080/>，用上面设置的密码以 `root` 登录。

> [!IMPORTANT]
> 容器内 `SLUICE_BIND_ADDR` **必须**设为 `0.0.0.0:8080`。默认的
> `127.0.0.1:8080` 只监听容器自身的 loopback，宿主机访问不到。

> [!WARNING]
> `SLUICE_MASTER_KEY` 用于加密数据库里所有厂商 API Key。如果不设置，每次启动都会
> 随机生成一个，**此前存储的所有厂商密钥将无法解密**。请生成一个、妥善保管、并显式设置：
>
> ```bash
> openssl rand -base64 32
> ```

> [!IMPORTANT]
> `DATABASE_URL` 必须指向**你自己运行**的 PostgreSQL。没有可达的数据库时服务仍能启动，
> 但会拒绝代理所有请求 —— 没有令牌库就没有办法校验客户端凭证。

### Docker Compose

`docker-compose.example.yml` 只运行网关本身。它刻意不内置数据库：请把
`DATABASE_URL` 指向你自己运维的 PostgreSQL。

```bash
cp docker-compose.example.yml docker-compose.yml
docker compose up
```

| 服务     | 用途                          |
| -------- | ----------------------------- |
| `sluice` | 网关，发布在 `127.0.0.1:8080` |

环境变量可以写在 compose 文件同级的 `.env` 里，下方表格列出了全部可选项。

代码改动后重新构建：

```bash
docker compose up --build
```

### Apple container

macOS 上没有 Docker 时，可以用 Apple 的 [`container`](https://github.com/apple/container)
CLI 直接跑镜像。发布出来的镜像同时构建 `linux/amd64` 与 `linux/arm64`，Apple Silicon
会拉到原生那一份，不需要转译。

```bash
container system start

# package 默认是私有的，没改成公开的话先登录
container registry login ghcr.io
container image pull ghcr.io/akioruri/sluice:latest

container run -d --name sluice \
  -p 127.0.0.1:8080:8080 \
  -e SLUICE_BIND_ADDR=0.0.0.0:8080 \
  -e DATABASE_URL='postgres://sluice:sluice@<你的宿主机地址>:5432/sluice' \
  -e SLUICE_MASTER_KEY="$(openssl rand -base64 32)" \
  -e SLUICE_ADMIN_PASSWORD='change-me' \
  ghcr.io/akioruri/sluice:latest

# 验证
curl -s http://127.0.0.1:8080/healthz          # -> ok
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/   # -> 200
```

`container build -t sluice:latest .` 也可以直接从 Dockerfile 本地构建。它是插件命令，
所以必须先 `container system start`，否则 `container --help` 里根本看不到 `build`。
本地构建比拉镜像慢：前端和 Rust 服务都要在你机器上编译，而 CI 发布的是编译好的产物。

和 Docker 不同的一点：`container` **没有 `compose` 命令**，`docker-compose.example.yml`
用不上，上面的 `run` 就是等价写法；另外它也没有 `host.docker.internal` 别名，
`DATABASE_URL` 要换成本机地址。

## 暴露到公网的注意事项

「家里搭建 + 映射公网」这个用法很方便，但意味着持有你 token 的人就能花你的钱：

- **不要**把管理后台的 8080 端口直接映射到公网。管理后台没有公网防护设计，应只监听
  `127.0.0.1` 或用防火墙挡住，再套一层反向代理（Caddy / Nginx）只转发 `/api` 与
  `/<vendor>/` 路径，并开启 TLS
- 反向代理上加一层 Basic Auth 或 IP 白名单，避免网关变成公共代理
- token 只发给可信的人；Sluice 目前没有按 token 的速率限制，滥用等于直接烧厂商额度
- `SLUICE_MASTER_KEY` 一定要显式设置并备份，否则重启后所有厂商密钥全部作废

## 配置说明

| 变量                    | 说明                                                | 必填                       |
| ----------------------- | --------------------------------------------------- | -------------------------- |
| `SLUICE_BIND_ADDR`      | 监听地址，默认 `127.0.0.1:8080`                     | 否                         |
| `DATABASE_URL`          | PostgreSQL 连接串                                   | 是 —— 未设置时无法代理请求 |
| `SLUICE_MASTER_KEY`     | 32 字节 base64 密钥，用于加密厂商 API Key           | 生产环境必填               |
| `SLUICE_ADMIN_PASSWORD` | 管理后台密码，用户名固定为 `root`，默认 `change-me` | 否                         |

直接通过环境变量设置，或在 `docker compose` 场景下写入同级的 `.env` 文件。不设置 `DATABASE_URL` 服务仍能启动，但管理后台无数据可显示，**且无法代理任何请求** —— 没有令牌库就没有办法校验客户端凭证，网关会直接拒绝而不是放行任何非空令牌。

### 管理后台

所有操作都在浏览器 <http://localhost:8080/> 完成：

1. **登录** —— 用 `SLUICE_ADMIN_PASSWORD` 以 `root` 身份登录
2. **添加供应商** —— 名称、上游地址、认证头、认证格式、API Key。名称即路径前缀，
   例如 `openai` 对应 `/openai/...`
3. **选择模型** —— 可拉取上游模型列表，也可手动输入模型名。**留空表示允许该供应商的所有模型**
4. **创建 API 密钥** —— 需填写名称，完整密钥只显示一次
5. **调用网关** —— 用该密钥请求，控制台会给出对应的 curl 示例

保留名称（`api`、`_app`、`healthz`、`dashboard`、`vendors`、`tokens`、`stats`、
`auth`）会被拒绝 —— 它们会与网关自身路由冲突。名称只允许字母、数字、`-`、
`_`、`.`，上游地址必须是完整的 `http://` 或 `https://` URL；两者在写入数据库前就会校验。

## 从源码构建

需要 Rust 1.99+ 与 Node 22+（pnpm 9）。

```bash
# 后端
cargo build --workspace --release

# 前端
cd frontend
pnpm install
pnpm build          # 输出到 frontend/build
```

也可以直接构建容器镜像。Dockerfile 是多阶段构建，前端和 Rust 服务都在镜像内编译，
宿主机只需要一个容器运行时：

```bash
docker build -t sluice:latest .

# Apple 的 container CLI 构建同一个 Dockerfile
container build -t sluice:latest .
```

服务端通过相对路径 `./frontend/build` 读取前端产物，因此请在仓库根目录启动：

```bash
./target/release/sluice-server
```

### 测试与 lint

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

### 前端开发服务器

```bash
cd frontend
pnpm dev            # Vite 开发服务器，/api 代理到 :8080
```

## 目录结构

```
crates/
  sluice-core/      配置、加密、token 基础能力
  sluice-server/     axum 服务、代理、管理 API
    migrations/      sqlx 迁移（编译进二进制）
  sluice-cli/       命令行客户端
frontend/            Svelte 5 + SvelteKit + Tailwind v4 管理后台
Dockerfile           多阶段构建（前端 + 服务端）
.github/workflows/   CI：`v*` 标签推送时构建 amd64 + arm64 镜像并发布到 ghcr.io
```

## 安全说明

- 厂商 API Key 入库前使用 AES-256-GCM 加密
- 统一 token 仅存储 SHA-256 哈希
- 吊销的 token 立即失效，其用量历史予以保留
- 日志从不记录请求体、响应体、`Authorization` 头或任何密钥
- 请求日志只记录**路径**，不记录 query string —— 客户端可能把凭据放在那里
  （`?api-key=…`），否则会被原样抄进日志
- 会话 token 保存在内存中，闲置 12 小时后过期且数量有上限，重启服务后需重新登录
- 管理登录有频率限制，密码比对为常量时间
- 静态文件通过 `ServeDir` 提供，它会直接拒绝请求路径中的 `..`，而不是去解析它
- 请求体上限 64 MiB，上游调用不会无限期挂起

## License

MIT

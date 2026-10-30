# flashasr API 参考

> 面向接入方（第三方 App、输入法、桌面 / 移动客户端）的接口文档。照本文即可从零接入，不需要看服务端源码。
>
> - 服务版本：**flashasr 0.1.0**
> - 协议版本：**1**（连接码 `v=1`，`/v1/status` 返回 `"protocol": 1`）
> - 文档日期：2026-10-30
> - 参考实现：`tools/asr-client/feiyin_asr_client.py`（纯标准库 Python，可直接跑通全流程，也是服务端冒烟测试）

---

## 目录

1. [概览](#1-概览)
2. [快速开始](#2-快速开始)
3. [连接与配对](#3-连接与配对)
4. [传输安全与连接规则](#4-传输安全与连接规则)
5. [WebSocket 协议](#5-websocket-协议)
6. [文字处理流水线](#6-文字处理流水线)
7. [HTTP 接口](#7-http-接口)
8. [错误码全表](#8-错误码全表)
9. [时序图](#9-时序图)
10. [多语言示例](#10-多语言示例)
11. [版本与兼容规则](#11-版本与兼容规则)
12. [常见问题](#12-常见问题)

---

## 1. 概览

### 1.1 flashasr 是什么

flashasr 是一个跑在用户自己电脑上的**本地实时语音识别服务**。终端（手机、另一台电脑、同机的其它程序）通过加密的 WebSocket 把麦克风音频推给它，它边听边回**预览文字**，说完后回一份经过整理的**终稿**。音频与文字都不出用户自己的网络（只有终端主动打开「输出优化」且服务端配置了大模型时，文字才会发给服务端配置的大模型）。

### 1.2 能力

| 能力 | 说明 | 终端能否控制 |
| --- | --- | --- |
| 实时识别 | 中文、英文及中英混说；边说边出预览，说完出终稿 | — |
| 逐词时间戳 | 终稿每个词都有起止毫秒 | — |
| 热词 | 每段识别可带自己的词表 | ✅ `vocabulary` |
| 数字规整（ITN） | 「三点五公斤」⇒「3.5公斤」 | ✅ 开 / 关 |
| 标点 | 整段按语义重新打标点；关掉则去掉全部标点 | ✅ 开 / 关 |
| 去句末标点 | 只去掉末尾标点，句中保留 | ✅ 开 / 关 |
| 繁简 | 输出简体或繁体 | ✅ 简 / 繁 / 跟服务端 |
| 输出优化 | 用服务端配置的大模型整理文字（可带输入场景） | ✅ 开 / 关 |
| 翻译 | 终稿另给一份中↔英译文，方向按内容自动判定 | ✅ 开 / 关（可在说完时才决定） |
| 同音字纠错、去语气词与口吃重复、无效语音判定 | 固定内置 | ❌ 不可关 |
| 声纹 | 服务端为每个终端单独累积使用人声纹，攒够后自动滤掉旁人声音 | ✅ 可重置（§7.2） |

### 1.3 与阿里云百炼实时识别的关系

flashasr 的 WebSocket 协议**照阿里云百炼实时语音识别的 `run-task` 双工协议**设计：同名动作（`run-task` / `finish-task`）、同名事件（`task-started` / `result-generated` / `task-finished` / `task-failed`）、同名参数（`format`、`sample_rate`、`vocabulary`、`inverse_text_normalization_enabled`、`punctuation_prediction_enabled`、`translation_enabled`），译文结构与百炼 Gummy 的 `output.translations[]` 同形。已经对接过百炼的代码，改动以下几处即可接入：

| 项 | 百炼 | flashasr |
| --- | --- | --- |
| 地址 / 凭证 | 固定域名 + API Key | 来自**配对**（连接码 / 二维码），见 §3 |
| TLS | CA 证书校验 | **TLS 1.3 + 证书指纹钉扎**（自签证书），见 §4 |
| 终端身份 | 无 | `run-task` 必须带 `parameters.feiyin.device_id / device_name` |
| 音频格式 | pcm / opus / wav 等，多种采样率 | 只收 **PCM 16 kHz 单声道 16 位** |
| 句子 | 多句，每句一个 `sentence_id` | 整段一句：`sentence_id` 恒为 1，`text` 每次给**至今全文** |
| 心跳 | `heartbeat` 参数 | 不支持；用 WebSocket Ping 保活 |
| 扩展 | — | `output.feiyin.*`、`usage.speech_ms`、HTTP `/v1/status`、声纹重置 |

> 扩展字段统一放在 `feiyin` 命名空间下（`parameters.feiyin.*`、`output.feiyin.*`，这是协议的固定字段名）。只认百炼字段的客户端可以直接忽略它们。

### 1.4 版本与兼容承诺

- 协议版本 1 内**只做向后兼容的增补**：新增可选参数、新增响应字段、新增错误码。老终端不改代码也能继续工作。
- 终端必须**忽略不认识的字段**；服务端也忽略终端发来的不认识的字段。
- 不兼容的改动会升协议版本号（连接码 `v`、`/v1/status` 的 `protocol` 同时变）。详见 §11。

---

## 2. 快速开始

### 2.1 五步跑通

| 步 | 做什么 | 关键点 |
| --- | --- | --- |
| ① 拿连接码 | 在 flashasr 管理界面「添加终端」，得到二维码 / 连接码 `flash-asr://pair?...`；同机程序可直接读本机配对文件（§3.4） | 新 key 须在 **10 分钟内**完成首次识别 |
| ② TLS + 指纹钉扎 | TCP 连 `主机:端口` → TLS 1.3 握手 → 取服务端证书 DER 算 SHA-256 → 与连接码 `fp` 比对 | **比对通过才能发任何应用数据** |
| ③ WebSocket 升级 | `GET /`，带 `Authorization: Bearer <key>` | 101 成功；401 / 503 正文是错误码 |
| ④ `run-task` | 发 JSON，带 `device_id` / `device_name` 与开关参数 | 收到 `task-started` 后才开始发音频 |
| ⑤ 发音频、收结果 | 二进制帧发 PCM（建议 100 ms / 3200 字节一帧），说完发 `finish-task` | 预览 `sentence_end=false` 反复多条；终稿 `sentence_end=true` 一条；最后 `task-finished` |

### 2.2 最小可运行 Python 示例

依赖：Python 3.8+，`pip install "websockets>=13"`。输入 16 kHz / 单声道 / 16 位 WAV。完整版（带地址自动选择、心跳、错误处理）见 §10.1。

```python
# quickstart.py —— python quickstart.py "<连接码>" input.wav
import hashlib, ipaddress, json, socket, ssl, sys, threading, time, uuid, wave
from urllib.parse import unquote
from websockets.sync.client import connect

code, wav_path = sys.argv[1], sys.argv[2]

# ① 解析连接码
fields = dict(kv.partition("=")[::2] for kv in code.split("?", 1)[1].split("&"))
fields = {k: unquote(v) for k, v in fields.items()}
assert fields.get("v") == "1", "连接码版本不支持"
key = "".join(c for c in fields["key"] if not c.isspace() and c != "-").upper()
fp = "".join(c for c in fields["fp"] if not c.isspace() and c != ":").lower()
endpoint = fields.get("lan") or fields.get("wan")
host, _, port = endpoint.rpartition(":")
host, port = host.strip("[]"), int(port)

# ② TLS 1.3 + 指纹钉扎（先验指纹，再发 key）
ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
ctx.minimum_version = ssl.TLSVersion.TLSv1_3
ctx.check_hostname = False          # 自签证书：不验主机名
ctx.verify_mode = ssl.CERT_NONE     # 不做 CA 校验，改由下面的指纹比对保证安全
try:
    ipaddress.ip_address(host); sni = None
except ValueError:
    sni = host
tls = ctx.wrap_socket(socket.create_connection((host, port), timeout=10), server_hostname=sni)
if hashlib.sha256(tls.getpeercert(binary_form=True)).hexdigest() != fp:
    tls.close(); sys.exit("证书指纹不符：需要重新配对")

# ③ WebSocket 升级（TLS 已由上面做完，所以这里用 ws:// + 现成 socket）
host_hdr = f"[{host}]:{port}" if ":" in host else f"{host}:{port}"
with connect(f"ws://{host_hdr}/", sock=tls, compression=None,
             additional_headers={"Authorization": f"Bearer {key}"}) as ws:
    # ④ run-task
    task_id = uuid.uuid4().hex
    ws.send(json.dumps({
        "header": {"action": "run-task", "task_id": task_id, "streaming": "duplex"},
        "payload": {"task_group": "audio", "task": "asr", "function": "recognition",
                    "model": "flashasr", "input": {},
                    "parameters": {"format": "pcm", "sample_rate": 16000,
                                   "feiyin": {"device_id": "demo0000device01", "device_name": "quickstart"}}},
    }))

    with wave.open(wav_path) as w:
        assert (w.getframerate(), w.getnchannels(), w.getsampwidth()) == (16000, 1, 2)
        pcm = w.readframes(w.getnframes())

    def send_audio():                     # ⑤ 按实时节奏发音频，说完发 finish-task
        for i in range(0, len(pcm), 3200):
            ws.send(pcm[i:i + 3200]); time.sleep(0.1)
        ws.send(json.dumps({"header": {"action": "finish-task", "task_id": task_id, "streaming": "duplex"},
                            "payload": {"input": {}}}))

    for raw in ws:
        ev = json.loads(raw); name = ev["header"]["event"]
        if name == "task-started":
            threading.Thread(target=send_audio, daemon=True).start()
        elif name == "result-generated":
            s = ev["payload"]["output"]["sentence"]
            print(("[终稿] " if s["sentence_end"] else "[预览] ") + s["text"])
        elif name == "task-finished":
            print("用量：", ev["payload"]["usage"]); break
        elif name == "task-failed":
            print("失败：", ev["header"]["error_code"], ev["header"]["error_message"]); break
```

---

## 3. 连接与配对

### 3.1 连接码（二维码内容）

服务端「添加终端」后给出三种等价形式：二维码、连接码（二维码的文字版）、分开显示的地址 + key + 指纹。连接码格式：

```
flash-asr://pair?v=1&lan=192.168.1.5:7860&wan=asr.example.com:7860&key=MFRGGZDFMZTWQ2LKNNWG23TPOA&fp=3f9a0c...（64 位十六进制）
```

| 字段 | 是否出现 | 格式 | 说明 |
| --- | --- | --- | --- |
| `v` | 总是 | `1` | 协议版本。**不是 `1` ⇒ 拒绝并提示用户升级客户端** |
| `lan` | 总是出现，**值可能为空** | `主机:端口` | 内网地址。服务端找不到可用内网网卡时值为空串（`lan=`） |
| `wan` | 服务端配置了外网地址才出现 | `主机:端口` | 外网地址（IP 或域名）；IPv6 带方括号，如 `[2001:db8::1]:7860` |
| `key` | 总是 | 26 个字符 | 终端凭证：16 字节随机数的 RFC 4648 base32（大写、无填充） |
| `fp` | 总是 | 64 个字符 | 服务端证书指纹：证书 DER 的 SHA-256，**小写**十六进制 |

解析规则（终端须照做）：

1. 前缀必须是 `flash-asr://pair?`。
2. 按 `&` 拆分，按第一个 `=` 拆键值；值**先做百分号解码**（服务端目前不编码，但终端须兼容编码过的写法）。
3. 同名字段重复出现时**取最后一个**；**不认识的字段忽略**（以后加字段不破坏老终端）。
4. `lan`、`wan` 去掉空值后**至少剩一个**，否则视为无效连接码。
5. 地址没写端口 ⇒ 补默认端口 `7860`；IPv6 主机必须带方括号。
6. key 按 §3.2 规整；指纹比对时去掉空白和冒号、转小写（`3F:9A:…` 与 `3f9a…` 视为相同）。

### 3.2 手输

用户可以分别输入内网 / 外网地址（至少一个）、key、指纹（可空）。

- **地址**：规则同 §3.1（缺端口补 7860，IPv6 带方括号）。
- **key 规整**：去掉所有空白和 `-`、转大写后再用。服务端按同样规则比对，所以 `mfrg-gzdf mztw…` 与 `MFRGGZDFMZTW…` 等价。
- **指纹为空**：先只做 TLS 握手取回服务端证书指纹（**不发 key**），展示给用户，与服务端管理界面显示的指纹逐位核对；用户确认后才保存。**未经确认的指纹不得保存，也不得带 key 连接。**

命令行取指纹（调试用）：

```bash
openssl s_client -connect 192.168.1.5:7860 -tls1_3 </dev/null 2>/dev/null \
  | openssl x509 -outform DER | sha256sum
```

### 3.3 终端身份 `device_id` / `device_name`

每次 `run-task` 都要带（§5.2）：

| 字段 | 规则 |
| --- | --- |
| `device_id` | 终端第一次配对时随机生成（建议 16 位小写字母数字），**存本地、之后永久不变**——重新配对、升级 App 都沿用。换了 `device_id` 等于换了一台设备 |
| `device_name` | 给服务端用户看的名字（如「张三的 iPhone」），可随时改；改了下次 `run-task` 自动更新 |

两者去掉首尾空白后都不能为空，否则 `MissingDeviceInfo` 并断开连接。

### 3.4 本机配对文件（终端与 flashasr 在同一台电脑）

flashasr 每次启动都会保证有一个「本机终端」，并把它的连接码写到当前用户的配置目录，同机程序读这个文件即可免配对：

| 系统 | 路径 | 权限 |
| --- | --- | --- |
| Windows | `%APPDATA%\flashasr\local-pairing.json` | 用户配置目录默认只本人可读 |
| macOS | `~/Library/Application Support/flashasr/local-pairing.json` | 0600 |

```json
{
  "terminal_id": "pgzmdppb",
  "uri": "flash-asr://pair?v=1&lan=127.0.0.1:7860&key=MFRGGZDFMZTWQ2LKNNWG23TPOA&fp=3f9a0c...",
  "port": 7860
}
```

| 字段 | 说明 |
| --- | --- |
| `terminal_id` | 本机终端的内部编号（只用于排查） |
| `uri` | 连接码，按 §3.1 解析；地址固定为 `127.0.0.1:<端口>`，不含 `wan` |
| `port` | 当前监听端口 |

本机终端的特殊规则：

- key **永不过期**（不受 10 分钟首次使用时限约束）。
- key **只接受回环来源**（`127.0.0.1` / `::1`）的连接；从别的机器用它连 ⇒ `401 InvalidKey`。
- **不绑定设备**：同一台电脑上的多个程序可以共用这把 key（各自带自己的 `device_id`），服务端只记录最近一次连接的设备。
- 被撤销或文件丢失 ⇒ flashasr 下次启动时重建（新 key）；改了端口 ⇒ key 不变、地址更新。**终端连接失败时应重新读一次此文件再试。**
- 同样必须做指纹钉扎（`fp` 在连接码里）。

### 3.5 key 的生命周期

| 阶段 | 规则 | 终端看到的现象 |
| --- | --- | --- |
| 生成 | 管理界面「添加终端」时生成；服务端只存 key 的哈希，明文只在这一刻展示 | — |
| 首次使用时限 | 新 key 须在生成后 **10 分钟内**完成首次 `run-task`（握手和 `run-task` 时都会检查） | 超时 ⇒ `KeyExpired`；服务端重启或再次「添加终端」时会清理过期 key，之后再用会变成 `InvalidKey` |
| 绑定（远程终端） | 首次 `run-task` 成功时，key 与该次的 `device_id` 绑定；之后只有这台设备能用 | 换 `device_id` ⇒ `DeviceMismatch` |
| 绑定（本机终端） | **不绑定**，见 §3.4 | — |
| 撤销 | 服务端管理员可随时撤销；在线连接立即断开，该终端的声纹一并删除 | 进行中的识别先收到 `task-failed`（`KeyRevoked`）再断开；之后握手 ⇒ `401 KeyRevoked` |

`InvalidKey`、`KeyExpired`、`KeyRevoked`、`DeviceMismatch` 都应提示用户「**需要重新配对**」，**不要自动重试**（每次失败都计入 IP 锁定，见 §4.3）。

### 3.6 内网 / 外网地址选择建议

服务端不区分内外网入口（同一端口），选哪个地址由终端决定。建议终端提供「自动 / 只用内网 / 只用外网」三选一，默认自动：

1. 只有一个地址 ⇒ 直接用。
2. 两个都有 ⇒ **同时**对两个地址发起 TCP 连接（只探测，不做 TLS、不带 key）。
3. 内网先通 ⇒ 用内网。
4. 外网先通 ⇒ 再等内网 **300 ms**，内网仍不通才用外网。
5. 两个都不通 ⇒ 提示网络不可达。
6. **每次开始识别前重新挑**（用户进出家门时自动切换）。

---

## 4. 传输安全与连接规则

### 4.1 TLS

- 服务端**只支持 TLS 1.3**；TLS 1.2 及以下握手直接失败。
- 证书是服务端首次启动时生成的**自签证书**：终端**不做 CA 校验、不校验主机名**，改为**钉扎证书指纹**。
- 服务端不设 ALPN，带不带都能握手。
- 连 IP 地址时不要发 SNI（多数 TLS 库对 IP 不发 SNI；Node.js 需显式置空，见 §10.2）。

### 4.2 指纹钉扎：正确顺序（硬要求）

```
TCP 连接 → TLS 1.3 握手完成 → 取服务端证书 DER → SHA-256 → 与配对时的 fp 比较
   ├─ 一致   → 才允许发 WebSocket 升级请求 / HTTP 请求（这时 key 才离开终端）
   └─ 不一致 → 立即关闭连接，一个字节都不发；提示「服务端证书变了，需要重新配对」
```

key 放在升级请求的 `Authorization` 头里。**先发请求再验指纹，等于把 key 交给了中间人**。各语言的正确写法见 §10：

- 在 TLS 握手回调里验证（OkHttp 的 `X509TrustManager`、Swift 的 `URLSessionDelegate` 服务端信任挑战）——握手失败时 HTTP 请求根本不会发出；
- 或先自己完成 TLS 握手并比对，再把已验证的连接交给 WebSocket 库（Python、Node.js 示例的写法）。

### 4.3 IP 锁定

| 规则 | 值 |
| --- | --- |
| 触发 | 同一来源 IP **连续 5 次**鉴权失败 |
| 锁定时长 | **10 分钟** |
| 锁定期间 | 该 IP 的新连接在 TCP 层被直接断开（连 TLS 握手都不做），终端看到的是「连接被重置 / 握手失败」 |
| 计为失败 | 升级请求没带 key / key 无效 / 过期 / 已撤销 / 本机 key 从非回环地址来；`run-task` 时的 `MissingDeviceInfo`、`DeviceMismatch` 及上述 key 错误；HTTP 接口返回 401 |
| 不计为失败 | `503 ServicePaused`；`InvalidParameter`、`UnsupportedFormat` 等参数错误 |
| 清零 | 该 IP 某次 `run-task` 鉴权成功；或锁定期满后重新计数 |

> 注意：只有 `run-task` 鉴权成功才清零计数，单纯升级成功或 `/v1/status` 成功不清零。程序 bug（如漏传 `device_id`）连续重试 5 次也会被锁 10 分钟。

### 4.4 连接规则

| 项 | 值 |
| --- | --- |
| 默认端口 | `7860`（服务端可改，以连接码为准） |
| 握手时限 | TLS 握手 + WebSocket 升级（或 HTTP 请求头）须在 **10 秒**内完成，否则断开 |
| 空闲超时 | **60 秒**收不到终端任何消息（音频、文本、Ping、Pong 都算）⇒ 服务端断开。服务端不主动发 Ping，计时只看终端发来的消息 |
| 保活建议 | 连接期间每 **≤ 30 秒**发一次 WebSocket Ping；**等终稿期间也要发**（开了翻译时，离线翻译模型首次加载可能要几十秒） |
| 单条消息上限 | 1 MiB（1,048,576 字节，含单帧上限）；超过 ⇒ 连接被断开 |
| 压缩 | 不支持 `permessage-deflate`（服务端不协商），客户端建议关闭 |
| 子协议 | 无，不要带 `Sec-WebSocket-Protocol` |
| 同时在线连接上限 | **64**（整个服务）；满了新连接在 TCP 层被直接断开，不排队 |
| 每连接并发 | 同一连接**同一时刻只能有一段识别**；收到 `task-finished` / `task-failed` 后才能发下一个 `run-task`。要多路并发就开多条连接 |
| 识别通道 | 服务端同时进行的识别段数受硬件限制，满了 ⇒ `ServiceBusy` |
| 服务端主动断开 | 撤销、鉴权失败、空闲超时、超大消息、服务端退出 |

---

## 5. WebSocket 协议

### 5.1 升级请求

```http
GET / HTTP/1.1
Host: 192.168.1.5:7860
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==
Sec-WebSocket-Version: 13
Authorization: Bearer MFRGGZDFMZTWQ2LKNNWG23TPOA
```

- **路径不限**（服务端不看路径，建议 `/`；`/v1/status` 与 `/v1/profile/voiceprint/reset` 两个路径保留给 HTTP 接口，见 §7）。
- `Authorization` 的方案名 `Bearer` 大小写不限；key 用 §3.2 规整后的形式（不规整也能认，服务端会再规整一次）。

| 结果 | 正文 | 含义 | 终端处理 |
| --- | --- | --- | --- |
| `101 Switching Protocols` | — | 成功 | 进入 §5.2 |
| `401` | `InvalidKey` | 没带 key / key 不存在 / 本机 key 从非回环地址来 | 提示重新配对，不重试 |
| `401` | `KeyExpired` | 新 key 超过 10 分钟未完成首次识别 | 提示重新配对，不重试 |
| `401` | `KeyRevoked` | key 已撤销 | 提示重新配对，不重试 |
| `503` | `ServicePaused` | 服务端暂停接入（暂停优先于 key 检查） | 提示「对方已暂停」，稍后再试 |
| TCP 被直接断 / TLS 握手失败 | — | IP 被锁定、连接数满、非 TLS 1.3，或网络不通 | **不要循环重连** |

被拒时正文是纯文本错误码（ASCII，很短）。有 `Content-Length` 就按它读；没有就读到连接关闭。

### 5.2 消息总则

- 终端 → 服务端：文本帧是 JSON `{"header": {...}, "payload": {...}}`；二进制帧是音频。
- 服务端 → 终端：只发文本帧，形如 `{"header": {"task_id", "event", "attributes": {}}, "payload": {...}}`。失败事件的 `header` 另带 `error_code`、`error_message`。
- 服务端不发二进制帧。Ping 由服务端自动回 Pong。
- 所有事件的 `header.task_id` 等于对应 `run-task` 的 `task_id`（JSON 解析失败时为空串）。

### 5.3 `run-task`（终端 → 服务端）：开始一段识别

完整示例（所有可选参数都写出）：

```json
{
  "header": {
    "action": "run-task",
    "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d",
    "streaming": "duplex"
  },
  "payload": {
    "task_group": "audio",
    "task": "asr",
    "function": "recognition",
    "model": "flashasr",
    "input": {},
    "parameters": {
      "format": "pcm",
      "sample_rate": 16000,
      "inverse_text_normalization_enabled": true,
      "punctuation_prediction_enabled": true,
      "translation_enabled": false,
      "vocabulary": ["飞书", "张一鸣", "鸿蒙系统"],
      "feiyin": {
        "device_id": "a1b2c3d4e5f6g7h8",
        "device_name": "张三的 iPhone",
        "output_optimization_enabled": false,
        "strip_trailing_punct": false,
        "chinese_script": "simplified",
        "scene": { "kind": "chat", "style_hint": "口语、简短", "multiline_safe": false }
      }
    }
  }
}
```

最小示例（只带必填项，其余取缺省）：

```json
{
  "header": { "action": "run-task", "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d" },
  "payload": { "parameters": { "feiyin": { "device_id": "a1b2c3d4e5f6g7h8", "device_name": "张三的 iPhone" } } }
}
```

#### 字段表

| 字段 | 类型 | 必填 | 缺省 | 说明 |
| --- | --- | --- | --- | --- |
| `header.action` | string | ✅ | — | 固定 `run-task` |
| `header.task_id` | string | ✅ | — | 本段识别的编号，每段新生成（建议 32 位十六进制 UUID）。空或缺 ⇒ `InvalidParameter` |
| `header.streaming` | string | | — | 忽略（照百炼写 `duplex` 即可） |
| `payload.task_group` / `task` / `function` / `model` / `input` | — | | — | 忽略（保留是为了与百炼代码通用） |
| `parameters.format` | string | | `"pcm"` | 只收 `pcm`（大小写不限），其它 ⇒ `UnsupportedFormat` |
| `parameters.sample_rate` | integer | | `16000` | 只收 `16000`，其它 ⇒ `UnsupportedFormat` |
| `parameters.inverse_text_normalization_enabled` | boolean | | `true` | 数字规整（ITN）：「三点五公斤」⇒「3.5公斤」，「一百二十」⇒「120」 |
| `parameters.punctuation_prediction_enabled` | boolean | | `true` | 标点。`true` ⇒ 终稿整段按语义重新打标点；`false` ⇒ 预览、终稿、译文**全部去掉标点** |
| `parameters.translation_enabled` | boolean | | `false` | 终稿另给一份译文（§5.7）。方向按内容自动判定，不可指定。可在 `finish-task` 里改（§5.5） |
| `parameters.vocabulary` | string[] 或 object | | 无 | 热词，见下文「热词规则」 |
| `parameters.feiyin.device_id` | string | ✅ | — | 设备代码，§3.3。去空白后为空 ⇒ `MissingDeviceInfo` 并断开 |
| `parameters.feiyin.device_name` | string | ✅ | — | 设备名称，§3.3。去空白后为空 ⇒ `MissingDeviceInfo` 并断开 |
| `parameters.feiyin.output_optimization_enabled` | boolean | | `false` | 输出优化：用服务端配置的大模型整理文字。服务端没配大模型 ⇒ 跳过、照常出结果，并在终稿标明（`optimization_skipped`）。与翻译的关系见 §6.3 |
| `parameters.feiyin.strip_trailing_punct` | boolean | | `false` | 去掉终稿句末标点（句中保留）。只作用于原文，不作用于译文 |
| `parameters.feiyin.chinese_script` | string | | 跟服务端设置 | `"simplified"`（简体）/ `"traditional"`（繁体），大小写与首尾空白不限；其它值或缺省 ⇒ 跟服务端设置 |
| `parameters.feiyin.scene` | object | | 场景未知 | 输入场景，供输出优化 / 大模型翻译调整写法。不是对象或解析失败 ⇒ 视为未知 |
| `parameters.feiyin.scene.kind` | string | | `""` | 见下方场景取值表 |
| `parameters.feiyin.scene.style_hint` | string | | `""` | 自由文字风格提示，如「正式」「口语、简短」 |
| `parameters.feiyin.scene.multiline_safe` | boolean | | `false` | 目标输入框能否接受换行（决定输出优化是否可分段） |

不认识的参数（如百炼的 `translation_target_languages`、`heartbeat`、`language_hints`）一律忽略。

#### `scene.kind` 取值

| 取值（大小写不限） | 场景 | 也接受的写法 |
| --- | --- | --- |
| `chat` | 聊天 / 即时通讯 | — |
| `agent` | 与 AI 助手 / 编程智能体对话 | `ai_agent`、`ai-agent`、`coding_agent` |
| `email` | 邮件 | — |
| `doc` | 文档编辑 | — |
| `ide_terminal` | 代码编辑器 / 终端 | `ide-terminal`、`ide/terminal` |
| `browser` | 浏览器 | — |
| 其它 / 空 | 未知场景 | — |

> **隐私**：不要传窗口标题、程序名等信息，协议也没有对应字段。

#### 热词规则（`vocabulary`）

- **写法**：字符串数组（推荐，**数组靠前的优先**），或百炼的 `{"词": 权重}` 对象（权重被忽略；对象写法的键顺序不保证，优先级不可控）。数组里的非字符串元素被忽略。
- **每个终端独立**：服务端只用本次 `run-task` 带来的词，不掺服务端本机词库、也不掺其它终端的词。
- **注入识别模型时的过滤**（被过滤的词仍参与 `wordbook_hits` 统计）：
  - 首尾空白去掉，空串与重复项去掉；
  - **纯 ASCII 词条（纯英文 / 数字，如 `LiveKit`、`iPhone15`）不注入**——实测这类词会带偏中文识别；含至少一个非 ASCII 字符的词（如 `鸿蒙OS`）照常注入；
  - 单条超过 **10 个字符**不注入；
  - 最多 **200 条**，且总长度受模型预算约束，超出部分按数组顺序截掉。

#### 成功与失败

- 成功 ⇒ 服务端立即回 `task-started`（§5.6.1），之后开始发音频。
- 参数 / 鉴权层面的失败 ⇒ 直接回 `task-failed`，**不会**先回 `task-started`：`InvalidParameter`、`ServicePaused`、`UnsupportedFormat`、`MissingDeviceInfo`、`InvalidKey`、`KeyExpired`、`KeyRevoked`、`DeviceMismatch`。其中后五个之后服务端**断开连接**。
- 识别引擎层面的失败 ⇒ **先回 `task-started`，紧接着回 `task-failed`**：`ModelNotReady`、`ServiceBusy`、`EngineRestarted`、`InternalError`（以及极少见的引擎侧 `InvalidParameter`）。终端不能把 `task-started` 当作「一定能识别成功」。

### 5.4 二进制音频帧（终端 → 服务端）

| 项 | 要求 |
| --- | --- |
| 编码 | PCM，**16 kHz、单声道、16 位有符号、小端**，裸数据，**不带 WAV 头** |
| 帧长 | 建议每帧 **100 ms = 3200 字节**；帧长可以不固定，但须为偶数字节，单帧 ≤ 1 MiB |
| 节奏 | 建议**按实时节奏**发送。服务端每段识别的音频缓冲约 256 帧，远快于实时发送（如一次性灌入长文件）时超出部分会被**丢弃** |
| 时机 | 收到 `task-started` 后开始发；`finish-task` 后不要再发 |
| 被丢弃的音频 | 没有进行中的识别时（`run-task` 之前、`task-finished` / `task-failed` 之后）发来的音频直接丢弃，不报错 |

采样率不是 16 kHz 的设备（如 44.1 / 48 kHz）需要终端自己重采样、混成单声道后再发。

### 5.5 `finish-task`（终端 → 服务端）：说完了

```json
{
  "header": { "action": "finish-task", "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d", "streaming": "duplex" },
  "payload": { "input": {} }
}
```

**可选：说完时才决定翻不翻**。`payload.parameters.translation_enabled`（boolean）覆盖 `run-task` 里的值；不带（或不是布尔值）⇒ 沿用 `run-task` 的。翻译只在终稿阶段做，所以在说完时决定不会晚：

```json
{
  "header": { "action": "finish-task", "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d", "streaming": "duplex" },
  "payload": { "input": {}, "parameters": { "translation_enabled": true } }
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `header.action` | string | ✅ | 固定 `finish-task` |
| `header.task_id` | string | ✅ | 必须等于进行中识别的 `task_id` |
| `payload.parameters.translation_enabled` | boolean | | 终稿阶段最终的翻译开关 |

- `task_id` 对不上 / 没有进行中的识别 ⇒ `task-failed`（`InvalidParameter`，`error_message` = `no such task`），**连接保持**。
- 发完继续收事件，直到 `task-finished` 或 `task-failed`。等待期间请照常发 Ping 保活（§4.4）。

### 5.6 取消

协议没有取消消息：**直接关闭 WebSocket 连接**即取消。服务端丢弃该段，不出终稿、不计入用量。要继续识别就重新连接。

### 5.7 服务端事件

#### 5.7.1 `task-started`

```json
{
  "header": { "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d", "event": "task-started", "attributes": {} },
  "payload": {}
}
```

收到后开始发音频。注意 §5.3 末尾：之后仍可能紧跟 `task-failed`。

#### 5.7.2 `result-generated`：预览

```json
{
  "header": { "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d", "event": "result-generated", "attributes": {} },
  "payload": {
    "output": {
      "sentence": {
        "sentence_id": 1,
        "begin_time": 120,
        "end_time": 1460,
        "text": "今天买了3.5公斤苹",
        "sentence_end": false,
        "words": [
          { "begin_time": 120,  "end_time": 300,  "text": "今",  "punctuation": "" },
          { "begin_time": 300,  "end_time": 480,  "text": "天",  "punctuation": "" },
          { "begin_time": 480,  "end_time": 640,  "text": "买",  "punctuation": "" },
          { "begin_time": 640,  "end_time": 760,  "text": "了",  "punctuation": "" },
          { "begin_time": 760,  "end_time": 1100, "text": "3.5", "punctuation": "" },
          { "begin_time": 1100, "end_time": 1220, "text": "公",  "punctuation": "" },
          { "begin_time": 1220, "end_time": 1340, "text": "斤",  "punctuation": "" },
          { "begin_time": 1340, "end_time": 1460, "text": "苹",  "punctuation": "" }
        ]
      },
      "feiyin": {
        "tentative": 1,
        "ts_source": ["rough", "rough", "rough", "rough", "rough", "rough", "rough", "rough"]
      }
    },
    "usage": null
  }
}
```

#### 5.7.3 `result-generated`：终稿

只出现一次，`sentence_end = true`，之后不再变化。下例开了翻译、带了词表：

```json
{
  "header": { "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d", "event": "result-generated", "attributes": {} },
  "payload": {
    "output": {
      "sentence": {
        "sentence_id": 1,
        "begin_time": 120,
        "end_time": 2380,
        "text": "今天在飞书买了3.5公斤苹果。",
        "sentence_end": true,
        "words": [
          { "begin_time": 120,  "end_time": 300,  "text": "今",  "punctuation": "" },
          { "begin_time": 300,  "end_time": 480,  "text": "天",  "punctuation": "" },
          { "begin_time": 480,  "end_time": 620,  "text": "在",  "punctuation": "" },
          { "begin_time": 620,  "end_time": 800,  "text": "飞",  "punctuation": "" },
          { "begin_time": 800,  "end_time": 980,  "text": "书",  "punctuation": "" },
          { "begin_time": 980,  "end_time": 1140, "text": "买",  "punctuation": "" },
          { "begin_time": 1140, "end_time": 1260, "text": "了",  "punctuation": "" },
          { "begin_time": 1260, "end_time": 1700, "text": "3.5", "punctuation": "" },
          { "begin_time": 1700, "end_time": 1860, "text": "公",  "punctuation": "" },
          { "begin_time": 1860, "end_time": 2020, "text": "斤",  "punctuation": "" },
          { "begin_time": 2020, "end_time": 2200, "text": "苹",  "punctuation": "" },
          { "begin_time": 2200, "end_time": 2380, "text": "果",  "punctuation": "。" }
        ]
      },
      "feiyin": {
        "tentative": 0,
        "ts_source": ["aligned", "aligned", "aligned", "aligned", "aligned", "aligned",
                      "aligned", "aligned", "aligned", "aligned", "aligned", "estimated"],
        "wordbook_hits": ["飞书"]
      },
      "translations": [
        { "sentence_id": 1, "lang": "en", "text": "Today I bought 3.5 kg of apples on Feishu.", "sentence_end": true }
      ]
    },
    "usage": null
  }
}
```

#### 字段表：`payload.output.sentence`

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `sentence_id` | integer | 恒为 `1`（整段一句） |
| `text` | string | **本段识别至今的全文**（不是增量）。每条预览都**整体替换**上一条，不要拼接。终稿可能是空串（没听到有效语音，或只有「嗯」「啊」之类） |
| `sentence_end` | boolean | `false` = 预览，后面还会变；`true` = 终稿 |
| `begin_time` | integer | 首词起点（毫秒）；没有词时为 `0` |
| `end_time` | integer \| null | 末词终点（毫秒）；没有词时为 `null` |
| `words` | array | 逐词时间，见下表；没有时间时为空数组 |

`words[]` 每项：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `begin_time` / `end_time` | integer | 毫秒，**相对本段第一帧音频** |
| `text` | string | 词文本。中文一字一词；连写的英文 / 数字合成一词（如 `3.5`、`hello`） |
| `punctuation` | string | 挂在该词后面的标点与空格（可能为空串） |

保证：所有词的 `text + punctuation` 依次拼接**逐字符等于** `sentence.text`（空格也挂在词上）。

#### 字段表：`payload.output.feiyin`（扩展，可忽略）

| 字段 | 类型 | 出现时机 | 说明 |
| --- | --- | --- | --- |
| `tentative` | integer | 总是 | 预览里 `text` **末尾**还没稳定的字符数（建议浅色显示）；终稿恒 `0` |
| `ts_source` | string[] | 总是 | 与 `words` 一一对应的时间来源：`aligned`（强制对齐，准）/ `rough`（识别模型自带，粗）/ `estimated`（插值估算）。终稿每个词都有时间，补不出的标 `estimated` |
| `wordbook_hits` | string[] | 仅终稿，非空时 | `vocabulary` 中出现在终稿 `text` 里的词（区分大小写的子串匹配；去重；按 `vocabulary` 顺序）。终端可据此统计词库命中 |
| `learn_suggestions` | string[] | 仅终稿，非空时 | 输出优化建议加进词库的词（只有输出优化由大模型做成时才可能有）。是否加入由终端 / 用户决定 |
| `optimization_skipped` | string | 仅终稿，要了输出优化但没做成时 | `llm_not_configured`：服务端没启用大模型或没填地址 / key；`llm_failed`：调用失败或返回空。此时终稿是不经大模型的结果 |

没有值的可选字段**不出现**（不是 `null` 或空数组）。

#### 字段表：`payload.output.translations`（仅终稿、翻译开启且成功时）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `sentence_id` | integer | 恒为 `1` |
| `lang` | string | `en`（原文含汉字时）或 `zh`（原文不含汉字时） |
| `text` | string | 译文；不带逐词时间 |
| `sentence_end` | boolean | 恒为 `true` |

翻译失败、终稿为空、或没开翻译 ⇒ **没有** `translations` 键，原文照常。

#### 预览与终稿的区别

| | 预览 | 终稿 |
| --- | --- | --- |
| 条数 | 多条，随说随出 | 一条 |
| `sentence_end` | `false` | `true` |
| 文字处理 | 只做同音字纠错、数字规整、关标点时去标点 | 全部节点（§6） |
| 标点 | 可能有也可能没有，以终稿为准 | 按参数处理后的最终标点 |
| `words` | 粗略时间，可能为空数组 | 每个词都有时间 |
| `tentative` | ≥ 0 | 0 |
| 扩展字段 / 译文 | 无 | 按需出现 |
| `usage` | `null` | `null`（用量在 `task-finished` 里） |

#### 5.7.4 `task-finished`

```json
{
  "header": { "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d", "event": "task-finished", "attributes": {} },
  "payload": { "output": {}, "usage": { "duration": 5, "speech_ms": 4448 } }
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `usage.duration` | integer | 本段收到的音频秒数，**向上取整** |
| `usage.speech_ms` | integer | 去掉静音后的有效语音毫秒数；服务端语音检测不可用时等于整段音频长度。终端可直接用于用量统计 |

之后可以在同一连接发下一个 `run-task`。

#### 5.7.5 `task-failed`

```json
{
  "header": {
    "task_id": "2f6b5d3c9a4e4f1b8c7d6e5f4a3b2c1d",
    "event": "task-failed",
    "error_code": "ServiceBusy",
    "error_message": "capacity",
    "attributes": {}
  },
  "payload": {}
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `header.error_code` | string | 机器可读错误码，见 §8。**按它做判断** |
| `header.error_message` | string | 补充说明（英文短语，如 `missing task_id`、`no such task`），只用于日志，内容可能变化 |

本段识别随之结束。是否断开连接见 §8。

---

## 6. 文字处理流水线

### 6.1 终稿处理顺序

终稿依次经过以下节点（预览只做其中第 3、4 步和「标点关时全部去掉」）：

| 顺序 | 节点 | 受哪个参数控制 |
| --- | --- | --- |
| 1 | 去语气词与口吃重复；繁简规整 | 固定开；繁简跟 `chinese_script` |
| 2 | 先去掉识别出的标点，留给第 8 步统一重打 | 仅 `punctuation_prediction_enabled = true` 时 |
| 3 | 无效语音判定：只剩语气词 ⇒ 终稿为空串，后续全部跳过（也不翻译） | 固定开 |
| 4 | 同音字纠错 | 固定开 |
| 5 | 数字规整（ITN） | `inverse_text_normalization_enabled` |
| 6 | 输出优化（大模型）；没做（没要 / 没配 / 失败）⇒ 繁简转换 + 英文大小写规整 | `feiyin.output_optimization_enabled`（开翻译时不在这一步做，见 §6.3） |
| 7 | 单位符号补充规整 | `inverse_text_normalization_enabled` |
| 8 | 补标点：已有标点则保留；日文 / 韩文用专用规则；其余用标点模型按语义整段重打 | `punctuation_prediction_enabled = true` 且第 6 步**不是**大模型做成的（大模型已经打好标点） |
| 9 | 标点收尾（见下表） | `punctuation_prediction_enabled`、`feiyin.strip_trailing_punct` |
| 10 | 翻译（另出一份，不改原文） | `translation_enabled`（`finish-task` 可覆盖） |

第 9 步「标点收尾」：

| `punctuation_prediction_enabled` | `strip_trailing_punct` | 结果 |
| --- | --- | --- |
| `false` | 任意 | 去掉全部标点 |
| `true` | `true` | 去掉句末标点，句中保留 |
| `true` | `false` | 短句（≤ 5 个单位：每个汉字 / 假名算 1，每个连写的英文词或数字算 1）去掉句末标点；更长的保留 |

译文也做一遍标点收尾：标点关 ⇒ 全部去掉；标点开 ⇒ 短译文去句末标点；`strip_trailing_punct` 不作用于译文。

### 6.2 翻译方向与引擎

- **方向**：终稿原文含汉字 ⇒ 译成英文（`lang = "en"`）；不含汉字 ⇒ 译成中文（`lang = "zh"`）。不可指定。
- **翻译的原文**：第 1–9 步处理完的终稿文字。
- **引擎**由该段的「输出优化」开关决定：
  - 没开输出优化 ⇒ 服务端**离线翻译模型**。首次使用要加载模型，可能多等几秒到几十秒；单次最长等 60 秒，超时则不给译文。
  - 开了输出优化 ⇒ 服务端大模型**一次调用**完成「优化 + 翻译」。服务端大模型未启用、未通过连通性验证、或调用失败 ⇒ 自动退回离线翻译模型。

### 6.3 输出优化与翻译的组合

| 输出优化 | 翻译 | 原文（`sentence.text`） | 译文（`translations`） | 终稿扩展字段 |
| --- | --- | --- | --- | --- |
| 关 | 关 | 本地处理 | — | 可能有 `wordbook_hits` |
| 开 | 关 | 大模型优化；没配 / 失败 ⇒ 退回本地处理 | — | 可能有 `learn_suggestions`；没做成时有 `optimization_skipped` |
| 关 | 开 | 本地处理 | 离线翻译模型 | — |
| 开 | 开 | 本地处理（**原文不再单独过大模型**） | 大模型一次完成「优化 + 翻译」；不可用时退回离线翻译 | **不会**出现 `learn_suggestions` 与 `optimization_skipped` |

开翻译时每段最多调用一次大模型。判断「开没开翻译」以 `finish-task` 时的最终值为准。

### 6.4 服务端没配大模型时

- `/v1/status` 的 `llm_configured = false`。
- 要了输出优化（且没开翻译）⇒ 终稿照常出，文字走本地处理，`output.feiyin.optimization_skipped = "llm_not_configured"`。终端可提示用户「对方未配置大模型，已按普通模式输出」。
- 开了翻译 ⇒ 用离线翻译模型，照常给译文。
- 大模型调用超时 / 出错 ⇒ `optimization_skipped = "llm_failed"`（或翻译退回离线模型），**不会**导致整段识别失败。

---

## 7. HTTP 接口

与 WebSocket **同一端口、同一 TLS**，同样必须**先钉扎指纹再发请求**（§4.2）。

通用约定：

- 每个请求单独一条连接：服务端回完即关闭（`Connection: close`）。不能在已升级的 WebSocket 连接上发 HTTP 请求。
- 鉴权：`Authorization: Bearer <key>`，与识别用同一把 key；本机 key 同样只接受回环来源。
- 响应头：`Content-Type: application/json`、`Content-Length`、`Cache-Control: no-store`、`Connection: close`。
- 请求头须在握手时限（10 秒）内发完；查询串被忽略；请求正文被忽略。
- 鉴权失败计入 IP 锁定（§4.3）。
- 服务端暂停接入期间这两个接口**照常工作**。
- 方法或路径不匹配（如 `POST /v1/status`）⇒ 被当作 WebSocket 升级处理，会以 401 或握手错误失败。

### 7.1 `GET /v1/status`：服务状态

终端据此决定能否开始识别、要不要提示「稍后再试」或「请升级」。

请求：

```http
GET /v1/status HTTP/1.1
Host: 192.168.1.5:7860
Authorization: Bearer MFRGGZDFMZTWQ2LKNNWG23TPOA
```

成功响应 `200 OK`：

```json
{
  "service": "flashasr",
  "version": "0.1.0",
  "protocol": 1,
  "engine_online": true,
  "local_realtime_ready": true,
  "llm_configured": false,
  "paused": false,
  "max_sessions": 8,
  "active_sessions": 1,
  "free_sessions": 7
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `service` | string | 恒为 `flashasr` |
| `version` | string | 服务程序版本（语义化版本号） |
| `protocol` | integer | 协议版本，当前 `1` |
| `engine_online` | boolean | 识别引擎进程是否在线（崩溃后约 5 秒自动重启，期间为 `false`） |
| `local_realtime_ready` | boolean | 识别模型是否已加载完。`false` 时 `run-task` 会得到 `ModelNotReady` |
| `llm_configured` | boolean | 服务端是否启用大模型并填了地址和 key（输出优化可用） |
| `paused` | boolean | 服务端是否暂停接入。`true` 时新的升级请求得到 `503`，`run-task` 得到 `ServicePaused` |
| `max_sessions` | integer | 服务端能同时进行的识别路数上限（所有终端合计）。模型没加载完 / 引擎不在线时为 `0` |
| `active_sessions` | integer | 此刻正在进行的识别路数（所有终端合计） |
| `free_sessions` | integer | `max_sessions - active_sessions`（不小于 0）。为 `0` 时新的 `run-task` 多半会得到 `ServiceBusy` |

`engine_online = false` 时，`local_realtime_ready` 与 `llm_configured` 恒为 `false`，`max_sessions` / `free_sessions` 为 `0`。

**`max_sessions` 的取值口径**：上限由显存决定，是**估算值**。识别模型加载完时按当时的空闲显存算一次（每路按约 150MB 计，先给本机留 1GB，硬顶 8 路；查不到显存时取硬顶 8），之后不再变。运行中显存被其他程序占用时，实际能开的路数可能更少——以 `run-task` 是否得到 `ServiceBusy` 为准。旧版服务端没有这三个字段，终端应按「缺字段 = 未知」处理。

**能开始识别** ⇔ `engine_online && local_realtime_ready && !paused`。

### 7.2 `POST /v1/profile/voiceprint/reset`：重置本终端声纹

服务端为每个终端单独累积使用人的声纹，攒够后自动滤掉其他人的声音（旁边的人说话、视频声音等）。终端换了使用人，或觉得过滤有误时，可以清空**本终端自己的**声纹，之后从零重新累积。终端由 key 确定，不收其它参数。

请求（无正文）：

```http
POST /v1/profile/voiceprint/reset HTTP/1.1
Host: 192.168.1.5:7860
Authorization: Bearer MFRGGZDFMZTWQ2LKNNWG23TPOA
Content-Length: 0
```

成功响应 `200 OK`：

```json
{ "reset": true }
```

引擎离线时同样返回成功（服务端直接删除声纹档案，引擎重启后从空白开始）。

### 7.3 HTTP 错误

| 状态 | 正文 | 触发 | 处理 |
| --- | --- | --- | --- |
| `401 Unauthorized` | `{"error":"InvalidKey"}` | 没带 key、key 无效 / 过期 / 已撤销、本机 key 从非回环地址来（**一律报 `InvalidKey`**，不细分） | 提示重新配对；不重试 |
| 连接被直接断开 | — | IP 被锁定、连接数已满 | 不要循环重试 |

---

## 8. 错误码全表

| 错误码 | 出现在 | 触发条件 | 连接 | 建议处理 | 重试 | 重新配对 |
| --- | --- | --- | --- | --- | --- | --- |
| `InvalidKey` | 升级 401 正文；`run-task` 的 `task-failed`；HTTP 401 | 没带 key / key 不存在 / 格式不对 / 本机 key 从非回环来源连接；过期 key 被服务端清理后 | **断开** | 「需要重新配对」 | ❌ | ✅ |
| `KeyExpired` | 升级 401；`task-failed` | 新 key 生成后 10 分钟内没完成首次识别 | **断开** | 「连接码已过期，请在服务端重新添加终端」 | ❌ | ✅ |
| `KeyRevoked` | 升级 401；`task-failed` | key 已被撤销（进行中的识别会先收到此事件再断开） | **断开** | 「已被对方移除，需要重新配对」 | ❌ | ✅ |
| `DeviceMismatch` | `task-failed` | key 已绑定其它 `device_id` | **断开** | 「需要重新配对」；检查 `device_id` 是否被意外重置 | ❌ | ✅ |
| `MissingDeviceInfo` | `task-failed` | `device_id` 或 `device_name` 缺失 / 去空白后为空 | **断开** | 程序错误，修代码 | ❌ | ❌ |
| `ServicePaused` | 升级 503 正文；`task-failed` | 服务端暂停接入（进行中的识别不受影响） | 503 时无连接；`task-failed` 时保持 | 「对方已暂停，稍后再试」；可轮询 `/v1/status` 的 `paused` | ✅ 稍后 | ❌ |
| `InvalidParameter` | `task-failed` | JSON 解析失败（`bad json`）；未知 `action`（`unknown action`）；缺 `task_id`（`missing task_id`）；已有进行中的识别又发 `run-task`（`a task is already running`）；`finish-task` 的 `task_id` 对不上或没有进行中的识别（`no such task`）；引擎拒绝本段参数 | 保持 | 程序错误，记日志 | ❌ | ❌ |
| `UnsupportedFormat` | `task-failed` | `format` 不是 `pcm` 或 `sample_rate` 不是 16000 | 保持 | 程序错误：改为 PCM 16 kHz | ❌ | ❌ |
| `ModelNotReady` | `task-failed`（在 `task-started` 之后） | 识别模型还没加载完，或引擎不在线 | 保持 | 「对方识别模型未就绪，稍后再试」；可轮询 `/v1/status` 直到 `local_realtime_ready = true` | ✅ 稍后 | ❌ |
| `ServiceBusy` | `task-failed`（在 `task-started` 之后） | 服务端识别通道已满 | 保持 | 「对方正忙，稍后再试」；退避重试（如 1 s、2 s、4 s） | ✅ 退避 | ❌ |
| `EngineRestarted` | `task-failed` | 进行中的识别遇到服务端引擎崩溃（约 5 秒后自动拉起并重新加载模型） | 保持 | 「识别中断，请重说」；之后的 `run-task` 可能先得到 `ModelNotReady` | ✅ 重说 | ❌ |
| `InternalError` | `task-failed` | 其它服务端内部错误 | 保持 | 「识别失败，可重试」 | ✅ | ❌ |

其它网络层现象：

| 现象 | 可能原因 | 处理 |
| --- | --- | --- |
| TCP 连上即被断开 / TLS 握手失败 | IP 被锁定（§4.3）、连接数满（64）、客户端未启用 TLS 1.3 | 不要循环重连；检查 TLS 版本；等 10 分钟 |
| 证书指纹不符 | 服务端重装 / 证书重新生成，或存在中间人 | 立即断开，提示「服务端证书变了，需要重新配对」 |
| 连接在 60 秒左右无故断开 | 空闲超时 | 发 Ping 保活 |
| 连接突然关闭且无 `task-failed` | 发了超过 1 MiB 的消息、服务端退出 | 检查帧长；重连 |

---

## 9. 时序图

### 9.1 一次识别全过程

```mermaid
sequenceDiagram
    participant C as 终端
    participant S as flashasr
    C->>S: TCP 连接 + TLS 1.3 握手
    Note over C: 取证书 DER 算 SHA-256，与 fp 比对<br/>不一致 ⇒ 断开（key 从未发出）
    C->>S: GET / + Authorization: Bearer key（WebSocket 升级）
    alt key 无效 / 过期 / 撤销
        S-->>C: 401 + 错误码正文（计入 IP 锁定）
    else 暂停接入
        S-->>C: 503 ServicePaused
    else 成功
        S-->>C: 101 Switching Protocols
    end
    C->>S: run-task（device_id / device_name / 开关参数）
    Note over S: 首次使用的远程 key 在此绑定 device_id
    S-->>C: task-started
    loop 说话中（每 100 ms）
        C->>S: 二进制音频 3200 字节
        S-->>C: result-generated（预览，sentence_end=false，整体替换显示）
    end
    C->>S: finish-task
    Note over C,S: 终端每 ≤30 s 发 Ping，等终稿期间也不停
    S-->>C: result-generated（终稿，sentence_end=true；可含 translations / 扩展字段）
    S-->>C: task-finished（usage.duration / speech_ms）
    opt 同一连接继续
        C->>S: run-task（新 task_id）
    end
```

### 9.2 录音中途决定翻译

`run-task` 时还不知道要不要翻译（例如用户说到一半才按下「翻译」键）：

```mermaid
sequenceDiagram
    participant U as 用户
    participant C as 终端
    participant S as flashasr
    C->>S: run-task（translation_enabled: false）
    S-->>C: task-started
    C->>S: 音频…
    S-->>C: result-generated（预览）
    U->>C: 说到一半按下「翻译」
    Note over C: 只记下状态，不需要发任何消息
    C->>S: 音频…（继续）
    U->>C: 说完
    C->>S: finish-task（payload.parameters.translation_enabled: true）
    Note over S: 终稿阶段按 true 处理：<br/>开了输出优化 ⇒ 大模型一次「优化+翻译」，原文不再单独优化<br/>没开 ⇒ 离线翻译模型
    S-->>C: result-generated（终稿 + translations）
    S-->>C: task-finished
```

反过来也成立：`run-task` 里开了翻译、说完时在 `finish-task` 里传 `false`，终稿就不带译文（且开了输出优化时原文会照常由大模型优化）。

### 9.3 引擎重启

```mermaid
sequenceDiagram
    participant C as 终端
    participant S as flashasr（网关）
    participant E as 识别引擎
    C->>S: run-task
    S-->>C: task-started
    C->>S: 音频…
    E--xS: 引擎崩溃
    S-->>C: task-failed（EngineRestarted），连接保持
    Note over C: 提示「识别中断，请重说」
    Note over S,E: 约 5 秒后自动拉起引擎，重新加载模型
    C->>S: run-task（用户重说）
    S-->>C: task-started
    S-->>C: task-failed（ModelNotReady）—— 模型尚未加载完
    loop 轮询（新连接，每 1–2 秒）
        C->>S: GET /v1/status
        S-->>C: {"engine_online": true, "local_realtime_ready": false, ...}
    end
    S-->>C: {"engine_online": true, "local_realtime_ready": true, ...}
    C->>S: run-task
    S-->>C: task-started → 正常识别
```

---

## 10. 多语言示例

所有示例中的 `<KEY>`、`<FINGERPRINT>` 为占位符，请替换为连接码中的 `key`（规整后）与 `fp`（64 位小写十六进制）。

### 10.1 Python（完整）

依赖：Python 3.8+，`pip install "websockets>=13"`。

功能：解析连接码 → 自动选内网 / 外网 → TLS 1.3 + 指纹钉扎 → 升级（处理 401 / 503）→ `run-task` → 按实时节奏发 WAV → 打印预览 / 终稿 / 译文 → Ping 保活 → `task-finished`。

```python
#!/usr/bin/env python3
"""flashasr Python 客户端示例。

用法：
    python flashasr_client.py "<连接码>" input.wav [--translate] [--optimize] [--vocab 飞书,张一鸣]
input.wav 必须是 16 kHz / 单声道 / 16 位。
"""
import argparse
import hashlib
import ipaddress
import json
import os
import secrets
import socket
import ssl
import sys
import threading
import time
import uuid
import wave
from urllib.parse import unquote

from websockets.exceptions import ConnectionClosed, InvalidStatus
from websockets.sync.client import connect

DEFAULT_PORT = 7860
FRAME_BYTES = 3200                 # 100 ms @ 16 kHz s16le mono
PING_EVERY = 20                    # 秒，须 < 60 秒空闲超时
DEVICE_FILE = os.path.expanduser("~/.flashasr-client.json")
REPAIR_CODES = {"InvalidKey", "KeyExpired", "KeyRevoked", "DeviceMismatch"}


# ---------- 配对 ----------
def parse_endpoint(s):
    """'host:port' / 'host' / '[v6]:port' / '[v6]' -> (host, port)；空串 -> None"""
    s = (s or "").strip()
    if not s:
        return None
    if s.startswith("["):
        host, _, rest = s[1:].partition("]")
        port = int(rest[1:]) if rest.startswith(":") else DEFAULT_PORT
    elif s.count(":") == 1:
        host, port = s.split(":")[0], int(s.split(":")[1])
    elif ":" not in s:
        host, port = s, DEFAULT_PORT
    else:
        raise ValueError(f"IPv6 地址须带方括号：{s}")
    return host, port


def parse_pair_code(code):
    prefix = "flash-asr://pair?"
    code = code.strip()
    if not code.startswith(prefix):
        raise ValueError("不是 flashasr 连接码")
    f = {}
    for kv in code[len(prefix):].split("&"):
        k, _, v = kv.partition("=")
        f[k] = unquote(v)                         # 先解码；重复字段取最后一个
    if f.get("v") != "1":
        raise ValueError("连接码版本不支持，请升级客户端")
    key = "".join(c for c in f.get("key", "") if not c.isspace() and c != "-").upper()
    fp = "".join(c for c in f.get("fp", "") if not c.isspace() and c != ":").lower()
    lan, wan = parse_endpoint(f.get("lan")), parse_endpoint(f.get("wan"))
    if not key or len(fp) != 64 or not (lan or wan):
        raise ValueError("连接码不完整")
    return {"lan": lan, "wan": wan, "key": key, "fp": fp}


def device_id():
    """首次生成、永久保存（重新配对也沿用）。"""
    try:
        with open(DEVICE_FILE, encoding="utf-8") as fh:
            did = json.load(fh)["device_id"].strip()
            if did:
                return did
    except (OSError, ValueError, KeyError, AttributeError):
        pass
    did = secrets.token_hex(8)                    # 16 位十六进制
    with open(DEVICE_FILE, "w", encoding="utf-8") as fh:
        json.dump({"device_id": did}, fh)
    return did


# ---------- 选地址 ----------
def tcp_ok(ep, timeout=3.0):
    try:
        with socket.create_connection(ep, timeout=timeout):
            return True
    except OSError:
        return False


def pick_endpoint(p):
    """两个地址同时探测：内网通就用内网；外网先通则再等内网 300 ms。"""
    if not (p["lan"] and p["wan"]):
        return p["lan"] or p["wan"]
    res = {}
    for name in ("lan", "wan"):
        threading.Thread(target=lambda n=name: res.__setitem__(n, tcp_ok(p[n])), daemon=True).start()
    wan_ok_at = None
    while True:
        if res.get("lan") is True:
            return p["lan"]
        if res.get("lan") is False and res.get("wan") is not None:
            if res["wan"]:
                return p["wan"]
            raise ConnectionError("内网、外网都连不上")
        if res.get("wan") is True:
            wan_ok_at = wan_ok_at or time.monotonic()
            if time.monotonic() - wan_ok_at >= 0.3:
                return p["wan"]
        time.sleep(0.01)


# ---------- TLS + 钉扎 ----------
class FingerprintMismatch(Exception):
    pass


def pinned_tls(host, port, expected_fp, timeout=10.0):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    ctx.maximum_version = ssl.TLSVersion.TLSv1_3
    ctx.check_hostname = False                    # 自签证书：不验主机名
    ctx.verify_mode = ssl.CERT_NONE               # 不做 CA 校验，安全性由指纹比对保证
    try:
        ipaddress.ip_address(host)
        sni = None                                # IP 地址不发 SNI
    except ValueError:
        sni = host
    raw = socket.create_connection((host, port), timeout=timeout)
    try:
        tls = ctx.wrap_socket(raw, server_hostname=sni)
    except (ssl.SSLError, OSError):
        raw.close()
        raise ConnectionError("TLS 1.3 握手失败（服务端不可达，或本 IP 被锁定 10 分钟）")
    actual = hashlib.sha256(tls.getpeercert(binary_form=True)).hexdigest()
    if actual != expected_fp:                     # 先验指纹 —— 不一致时 key 一个字节都没发
        tls.close()
        raise FingerprintMismatch(f"证书指纹不符（期望 {expected_fp}，实际 {actual}），需要重新配对")
    return tls


def open_ws(endpoint, key, fp):
    host, port = endpoint
    tls = pinned_tls(host, port, fp)
    host_hdr = f"[{host}]:{port}" if ":" in host else f"{host}:{port}"
    try:
        # TLS 已完成并验证，这里用 ws:// 把现成的 TLS socket 交给 websockets
        return connect(
            f"ws://{host_hdr}/",
            sock=tls,
            additional_headers={"Authorization": f"Bearer {key}"},
            compression=None,
            max_size=1 << 20,
            open_timeout=10,
        )
    except InvalidStatus as e:
        status = e.response.status_code
        body = (e.response.body or b"").decode("ascii", "replace").strip()
        if status == 401:
            raise PermissionError(f"{body}：需要重新配对")
        if status == 503:
            raise RuntimeError("对方已暂停接入，稍后再试")
        raise RuntimeError(f"升级被拒：HTTP {status} {body}")


# ---------- 消息 ----------
def run_task_msg(task_id, args):
    params = {
        "format": "pcm",
        "sample_rate": 16000,
        "inverse_text_normalization_enabled": True,
        "punctuation_prediction_enabled": True,
        "translation_enabled": args.translate,
        "feiyin": {
            "device_id": device_id(),
            "device_name": args.device_name,
            "output_optimization_enabled": args.optimize,
        },
    }
    if args.vocab:
        params["vocabulary"] = [w.strip() for w in args.vocab.split(",") if w.strip()]
    return {
        "header": {"action": "run-task", "task_id": task_id, "streaming": "duplex"},
        "payload": {"task_group": "audio", "task": "asr", "function": "recognition",
                    "model": "flashasr", "input": {}, "parameters": params},
    }


def finish_task_msg(task_id, translate=None):
    payload = {"input": {}}
    if translate is not None:                    # 说完时才决定翻不翻
        payload["parameters"] = {"translation_enabled": translate}
    return {"header": {"action": "finish-task", "task_id": task_id, "streaming": "duplex"},
            "payload": payload}


def load_wav(path):
    with wave.open(path, "rb") as w:
        if (w.getframerate(), w.getnchannels(), w.getsampwidth()) != (16000, 1, 2):
            raise ValueError("WAV 必须是 16 kHz / 单声道 / 16 位")
        return w.readframes(w.getnframes())


# ---------- 识别 ----------
def recognize(ws, pcm, args):
    task_id = uuid.uuid4().hex
    ws.send(json.dumps(run_task_msg(task_id, args)))
    done = threading.Event()

    def keepalive():                              # 整段期间（含等终稿）持续 Ping
        while not done.wait(PING_EVERY):
            try:
                ws.ping()
            except Exception:
                return

    def send_audio():
        try:
            t = time.monotonic()
            for i in range(0, len(pcm), FRAME_BYTES):
                if done.is_set():
                    return
                chunk = pcm[i:i + FRAME_BYTES]
                ws.send(chunk)                    # bytes ⇒ 二进制帧
                t += len(chunk) / 32000           # 实时节奏
                time.sleep(max(0.0, t - time.monotonic()))
            ws.send(json.dumps(finish_task_msg(task_id)))
        except Exception:
            pass                                  # 主线程会收到断开 / 失败事件

    threading.Thread(target=keepalive, daemon=True).start()
    try:
        for raw in ws:
            ev = json.loads(raw)
            h = ev["header"]
            name = h["event"]
            if name == "task-started":
                threading.Thread(target=send_audio, daemon=True).start()
            elif name == "result-generated":
                out = ev["payload"]["output"]
                s, fy = out["sentence"], out.get("feiyin", {})
                if not s["sentence_end"]:
                    n = fy.get("tentative", 0)
                    text = s["text"]
                    shown = text if n == 0 else f"{text[:-n]}[{text[-n:]}]"
                    print(f"\r预览：{shown}\033[K", end="", flush=True)
                    continue
                print(f"\r终稿：{s['text'] or '（空：没听到有效语音）'}\033[K")
                for tr in out.get("translations", []):
                    print(f"译文[{tr['lang']}]：{tr['text']}")
                if "wordbook_hits" in fy:
                    print("词库命中：", fy["wordbook_hits"])
                if "learn_suggestions" in fy:
                    print("建议加入词库：", fy["learn_suggestions"])
                if "optimization_skipped" in fy:
                    print("输出优化未生效：", fy["optimization_skipped"])
            elif name == "task-finished":
                u = ev["payload"]["usage"]
                print(f"完成：音频 {u['duration']} 秒，有效语音 {u['speech_ms']} 毫秒")
                return 0
            elif name == "task-failed":
                code = h.get("error_code")
                print(f"\n失败：{code}（{h.get('error_message')}）")
                if code in REPAIR_CODES:
                    print("需要重新配对")
                return 3
        print("\n连接在 task-finished 之前被关闭")
        return 8
    except ConnectionClosed as e:
        print(f"\n连接被关闭：{e}")
        return 8
    finally:
        done.set()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("code")
    ap.add_argument("wav")
    ap.add_argument("--translate", action="store_true")
    ap.add_argument("--optimize", action="store_true")
    ap.add_argument("--vocab", default="")
    ap.add_argument("--device-name", default="python-demo")
    args = ap.parse_args()

    pairing = parse_pair_code(args.code)
    pcm = load_wav(args.wav)                      # 先检查音频，格式不对就不占连接
    endpoint = pick_endpoint(pairing)
    with open_ws(endpoint, pairing["key"], pairing["fp"]) as ws:   # 退出 with 即关闭连接
        return recognize(ws, pcm, args)


if __name__ == "__main__":
    sys.exit(main())
```

查询服务状态（复用上面的 `pinned_tls`）：

```python
def get_status(endpoint, key, fp):
    host, port = endpoint
    tls = pinned_tls(host, port, fp)              # 同样先钉扎
    try:
        tls.sendall((f"GET /v1/status HTTP/1.1\r\nHost: {host}\r\n"
                     f"Authorization: Bearer {key}\r\nConnection: close\r\n\r\n").encode())
        data = b""
        while chunk := tls.recv(4096):
            data += chunk
    finally:
        tls.close()
    head, _, body = data.partition(b"\r\n\r\n")
    status = int(head.split(b" ", 2)[1])
    return status, json.loads(body or b"{}")
```

### 10.2 JavaScript（Node.js，`ws` 库）

依赖：Node.js 18+，`npm i ws@^8.18`（需要 `createConnection` 选项）。

做法：先用 `node:tls` 自己完成握手并比对指纹，**通过后**再把这条已验证的 TLS 连接交给 `ws` 发升级请求。不要依赖 `checkServerIdentity`——对自签证书它不会被调用。

```js
// flashasr-client.mjs —— node flashasr-client.mjs input.pcm
import tls from "node:tls";
import net from "node:net";
import crypto from "node:crypto";
import fs from "node:fs";
import WebSocket from "ws";

const HOST = "192.168.1.5";
const PORT = 7860;
const KEY = "<KEY>";                  // 规整后的 key
const FINGERPRINT = "<FINGERPRINT>";  // 64 位小写十六进制
const DEVICE_ID = "a1b2c3d4e5f6g7h8"; // 首次生成后持久保存
const DEVICE_NAME = "node-demo";

/** TLS 1.3 握手 + 指纹比对；通过才 resolve（此时尚未发送任何应用数据）。 */
function pinnedTls(host, port, expected) {
  return new Promise((resolve, reject) => {
    const sock = tls.connect({
      host,
      port,
      servername: net.isIP(host) ? "" : host, // IP 地址不发 SNI
      minVersion: "TLSv1.3",
      maxVersion: "TLSv1.3",
      rejectUnauthorized: false,              // 自签证书：不做 CA 校验，改由指纹保证
    });
    sock.setTimeout(10_000, () => sock.destroy(new Error("TLS 握手超时")));
    sock.once("error", reject);
    sock.once("secureConnect", () => {
      const der = sock.getPeerCertificate(true).raw;
      const actual = crypto.createHash("sha256").update(der).digest("hex");
      if (actual !== expected) {
        sock.destroy();
        reject(new Error(`证书指纹不符（实际 ${actual}），需要重新配对`));
        return;
      }
      sock.setTimeout(0);
      sock.removeListener("error", reject);
      resolve(sock);
    });
  });
}

async function main(pcmPath) {
  const sock = await pinnedTls(HOST, PORT, FINGERPRINT);
  const hostHdr = net.isIPv6(HOST) ? `[${HOST}]:${PORT}` : `${HOST}:${PORT}`;
  // TLS 已完成：用 ws:// 并通过 createConnection 交出已验证的连接
  const ws = new WebSocket(`ws://${hostHdr}/`, {
    createConnection: () => sock,
    headers: { Authorization: `Bearer ${KEY}` },
    perMessageDeflate: false,
    maxPayload: 1 << 20,
    handshakeTimeout: 10_000,
  });

  ws.on("unexpected-response", (_req, res) => {
    let body = "";
    res.on("data", (c) => (body += c));
    res.on("end", () => {
      if (res.statusCode === 401) console.error(`${body.trim()}：需要重新配对`);
      else if (res.statusCode === 503) console.error("对方已暂停接入，稍后再试");
      else console.error(`升级被拒：HTTP ${res.statusCode} ${body}`);
      ws.terminate();
    });
  });

  const taskId = crypto.randomUUID().replaceAll("-", "");
  let pinger, sender;

  ws.on("open", () => {
    pinger = setInterval(() => ws.ping(), 20_000); // 保活（含等终稿期间）
    ws.send(JSON.stringify({
      header: { action: "run-task", task_id: taskId, streaming: "duplex" },
      payload: {
        task_group: "audio", task: "asr", function: "recognition", model: "flashasr", input: {},
        parameters: {
          format: "pcm",
          sample_rate: 16000,
          inverse_text_normalization_enabled: true,
          punctuation_prediction_enabled: true,
          translation_enabled: false,
          vocabulary: ["飞书", "张一鸣"],
          feiyin: { device_id: DEVICE_ID, device_name: DEVICE_NAME },
        },
      },
    }));
  });

  ws.on("message", (data, isBinary) => {
    if (isBinary) return;
    const ev = JSON.parse(data.toString("utf8"));
    const { event, error_code, error_message } = ev.header;
    switch (event) {
      case "task-started": {
        // 按实时节奏发 16k s16le 裸 PCM，每 100 ms 一帧 3200 字节
        const pcm = fs.readFileSync(pcmPath);
        let off = 0;
        sender = setInterval(() => {
          if (off >= pcm.length) {
            clearInterval(sender);
            ws.send(JSON.stringify({
              header: { action: "finish-task", task_id: taskId, streaming: "duplex" },
              payload: { input: {} },
            }));
            return;
          }
          ws.send(pcm.subarray(off, off + 3200)); // Buffer ⇒ 二进制帧
          off += 3200;
        }, 100);
        break;
      }
      case "result-generated": {
        const out = ev.payload.output;
        const s = out.sentence;
        if (!s.sentence_end) {
          process.stdout.write(`\r预览：${s.text}\x1b[K`);
        } else {
          console.log(`\r终稿：${s.text}\x1b[K`);
          for (const t of out.translations ?? []) console.log(`译文[${t.lang}]：${t.text}`);
          if (out.feiyin?.optimization_skipped) console.log("输出优化未生效：", out.feiyin.optimization_skipped);
        }
        break;
      }
      case "task-finished":
        console.log("完成：", ev.payload.usage);
        ws.close();
        break;
      case "task-failed":
        console.error(`失败：${error_code}（${error_message}）`);
        ws.close();
        break;
    }
  });

  ws.on("close", () => { clearInterval(pinger); clearInterval(sender); });
  ws.on("error", (e) => console.error("连接错误：", e.message));
}

main(process.argv[2]).catch((e) => { console.error(e.message); process.exit(1); });
```

### 10.3 Kotlin（Android / JVM，OkHttp）

依赖：OkHttp 4.x。Android 10+（API 29+）才支持 TLS 1.3。

做法：自定义 `X509TrustManager`，在 TLS 握手阶段比对证书 DER 的 SHA-256。比对失败 ⇒ 握手失败 ⇒ 升级请求（含 key）根本不会发出。

> 不要用 OkHttp 自带的 `CertificatePinner`：它钉扎的是公钥（SPKI）哈希，且要求证书先通过 CA 校验，不适用于本协议的「证书 DER 指纹」。

```kotlin
import okhttp3.*
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.json.JSONArray
import org.json.JSONObject
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.UUID
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLContext
import javax.net.ssl.X509TrustManager

/** 只认配对时的证书指纹（证书 DER 的 SHA-256，小写十六进制）。 */
class FingerprintTrustManager(private val expected: String) : X509TrustManager {
    override fun checkServerTrusted(chain: Array<X509Certificate>, authType: String) {
        val leaf = chain.firstOrNull() ?: throw CertificateException("服务端没有证书")
        val actual = MessageDigest.getInstance("SHA-256").digest(leaf.encoded)
            .joinToString("") { "%02x".format(it) }
        if (actual != expected) throw CertificateException("证书指纹不符，需要重新配对")
    }
    override fun checkClientTrusted(chain: Array<X509Certificate>, authType: String) =
        throw CertificateException("不支持客户端证书")
    override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
}

fun buildClient(fingerprint: String): OkHttpClient {
    val tm = FingerprintTrustManager(fingerprint)
    val ssl = SSLContext.getInstance("TLSv1.3").apply { init(null, arrayOf(tm), SecureRandom()) }
    val tls13 = ConnectionSpec.Builder(ConnectionSpec.MODERN_TLS)
        .tlsVersions(TlsVersion.TLS_1_3)
        .build()
    return OkHttpClient.Builder()
        .sslSocketFactory(ssl.socketFactory, tm)
        .hostnameVerifier { _, _ -> true }      // 自签证书无主机名可验；身份由指纹保证
        .connectionSpecs(listOf(tls13))
        .connectTimeout(10, TimeUnit.SECONDS)
        .pingInterval(20, TimeUnit.SECONDS)     // 保活，须 < 60 秒空闲超时
        .build()
}

class FlashAsrSession(
    private val client: OkHttpClient,
    private val hostPort: String,               // 如 "192.168.1.5:7860"，IPv6 写 "[fe80::1]:7860"
    private val key: String,
    private val deviceId: String,
    private val deviceName: String,
    private val onPreview: (String) -> Unit,
    private val onFinal: (text: String, translation: String?) -> Unit,
    private val onError: (code: String, needRepair: Boolean) -> Unit,
) {
    private val taskId = UUID.randomUUID().toString().replace("-", "")
    @Volatile private var started = false
    private lateinit var ws: WebSocket

    fun start(translate: Boolean = false, vocabulary: List<String> = emptyList()) {
        val req = Request.Builder()
            .url("wss://$hostPort/")
            .header("Authorization", "Bearer $key")
            .build()
        ws = client.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                val params = JSONObject()
                    .put("format", "pcm")
                    .put("sample_rate", 16000)
                    .put("inverse_text_normalization_enabled", true)
                    .put("punctuation_prediction_enabled", true)
                    .put("translation_enabled", translate)
                    .put("vocabulary", JSONArray(vocabulary))
                    .put("feiyin", JSONObject()
                        .put("device_id", deviceId)
                        .put("device_name", deviceName)
                        .put("scene", JSONObject().put("kind", "chat")))
                val msg = JSONObject()
                    .put("header", JSONObject()
                        .put("action", "run-task").put("task_id", taskId).put("streaming", "duplex"))
                    .put("payload", JSONObject()
                        .put("task_group", "audio").put("task", "asr").put("function", "recognition")
                        .put("model", "flashasr").put("input", JSONObject()).put("parameters", params))
                webSocket.send(msg.toString())
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                val ev = JSONObject(text)
                val h = ev.getJSONObject("header")
                when (h.getString("event")) {
                    "task-started" -> started = true      // 之后 sendAudio 才会真正发送
                    "result-generated" -> {
                        val out = ev.getJSONObject("payload").getJSONObject("output")
                        val s = out.getJSONObject("sentence")
                        if (!s.getBoolean("sentence_end")) {
                            onPreview(s.getString("text"))
                        } else {
                            val tr = out.optJSONArray("translations")
                                ?.optJSONObject(0)?.optString("text")
                            onFinal(s.getString("text"), tr)
                        }
                    }
                    "task-finished" -> webSocket.close(1000, null)
                    "task-failed" -> {
                        started = false
                        val code = h.optString("error_code")
                        onError(code, code in setOf("InvalidKey", "KeyExpired", "KeyRevoked", "DeviceMismatch"))
                        webSocket.close(1000, null)
                    }
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                started = false
                when (response?.code) {
                    401 -> onError(response.body?.string()?.trim() ?: "InvalidKey", true)
                    503 -> onError("ServicePaused", false)
                    else -> onError(t.message ?: "NetworkError", false) // 含指纹不符（SSLHandshakeException）
                }
            }
        })
    }

    /** 录音线程调用：16 kHz / 单声道 / PCM16 LE，建议每次 3200 字节（100 ms）。 */
    fun sendAudio(buf: ByteArray, len: Int) {
        if (started) ws.send(buf.toByteString(0, len))
    }

    /** 说完了；translateAtFinish 非空时覆盖 run-task 的翻译开关。 */
    fun finish(translateAtFinish: Boolean? = null) {
        val payload = JSONObject().put("input", JSONObject())
        translateAtFinish?.let { payload.put("parameters", JSONObject().put("translation_enabled", it)) }
        ws.send(JSONObject()
            .put("header", JSONObject()
                .put("action", "finish-task").put("task_id", taskId).put("streaming", "duplex"))
            .put("payload", payload).toString())
    }

    /** 取消 = 直接关连接。 */
    fun cancel() = ws.cancel()
}
```

Android 录音示意（`AudioRecord` 直接给 16 kHz 单声道 PCM16，无需重采样）：

```kotlin
val rec = AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, 16000,
    AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, 3200 * 4)
val buf = ByteArray(3200)
rec.startRecording()
while (recording) {
    val n = rec.read(buf, 0, buf.size)
    if (n > 0) session.sendAudio(buf, n)
}
rec.stop(); rec.release()
session.finish()
```

### 10.4 Swift（iOS 15+ / macOS 12+，`URLSessionWebSocketTask`）

做法：在 `URLSessionDelegate` 的服务端信任挑战里比对证书 DER 的 SHA-256。该回调发生在 TLS 握手阶段、HTTP 升级请求发出之前；比对失败就取消挑战，key 不会发出。

```swift
import Foundation
import CryptoKit

final class FlashAsrClient: NSObject, URLSessionWebSocketDelegate {
    private let hostPort: String          // "192.168.1.5:7860"，IPv6 写 "[fe80::1]:7860"
    private let key: String               // "<KEY>"
    private let fingerprint: String       // "<FINGERPRINT>"，64 位小写十六进制
    private let deviceId: String
    private let deviceName: String
    private var session: URLSession!
    private var task: URLSessionWebSocketTask!
    private var pingTimer: Timer?
    private let taskId = UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
    private(set) var started = false

    var onPreview: ((String) -> Void)?
    var onFinal: ((_ text: String, _ translation: String?) -> Void)?
    var onError: ((_ code: String, _ needRepair: Bool) -> Void)?

    init(hostPort: String, key: String, fingerprint: String, deviceId: String, deviceName: String) {
        self.hostPort = hostPort; self.key = key; self.fingerprint = fingerprint
        self.deviceId = deviceId; self.deviceName = deviceName
        super.init()
        let cfg = URLSessionConfiguration.ephemeral
        cfg.tlsMinimumSupportedProtocolVersion = .TLSv13
        session = URLSession(configuration: cfg, delegate: self, delegateQueue: nil)
    }

    // MARK: 指纹钉扎（TLS 握手阶段，升级请求发出之前）
    func urlSession(_ session: URLSession, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              let trust = challenge.protectionSpace.serverTrust,
              let chain = SecTrustCopyCertificateChain(trust) as? [SecCertificate],
              let leaf = chain.first else {
            completionHandler(.cancelAuthenticationChallenge, nil); return
        }
        let der = SecCertificateCopyData(leaf) as Data
        let actual = SHA256.hash(data: der).map { String(format: "%02x", $0) }.joined()
        if actual == fingerprint {
            completionHandler(.useCredential, URLCredential(trust: trust))
        } else {
            completionHandler(.cancelAuthenticationChallenge, nil)   // key 不会发出
            onError?("FingerprintMismatch", true)
        }
    }

    // MARK: 连接 + run-task
    func start(translate: Bool = false, vocabulary: [String] = []) {
        var req = URLRequest(url: URL(string: "wss://\(hostPort)/")!)
        req.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        req.timeoutInterval = 10
        task = session.webSocketTask(with: req)
        task.maximumMessageSize = 1 << 20
        task.resume()

        let runTask: [String: Any] = [
            "header": ["action": "run-task", "task_id": taskId, "streaming": "duplex"],
            "payload": [
                "task_group": "audio", "task": "asr", "function": "recognition",
                "model": "flashasr", "input": [String: Any](),
                "parameters": [
                    "format": "pcm",
                    "sample_rate": 16000,
                    "inverse_text_normalization_enabled": true,
                    "punctuation_prediction_enabled": true,
                    "translation_enabled": translate,
                    "vocabulary": vocabulary,
                    "feiyin": ["device_id": deviceId, "device_name": deviceName],
                ] as [String: Any],
            ] as [String: Any],
        ]
        sendJSON(runTask)
        receiveLoop()
        DispatchQueue.main.async {
            self.pingTimer = Timer.scheduledTimer(withTimeInterval: 20, repeats: true) { [weak self] _ in
                self?.task.sendPing { _ in }                    // 保活，须 < 60 秒
            }
        }
    }

    // MARK: 发音频：16 kHz / 单声道 / Int16 LE，建议每次 3200 字节（100 ms）
    func sendAudio(_ pcm: Data) {
        guard started else { return }
        task.send(.data(pcm)) { _ in }
    }

    /// 说完了；translateAtFinish 非空时覆盖 run-task 的翻译开关。
    func finish(translateAtFinish: Bool? = nil) {
        var payload: [String: Any] = ["input": [String: Any]()]
        if let t = translateAtFinish { payload["parameters"] = ["translation_enabled": t] }
        sendJSON(["header": ["action": "finish-task", "task_id": taskId, "streaming": "duplex"],
                  "payload": payload])
    }

    /// 取消 = 直接关连接。
    func cancel() { close() }

    // MARK: 收事件
    private func receiveLoop() {
        task.receive { [weak self] result in
            guard let self else { return }
            switch result {
            case .failure:
                self.reportHandshakeFailure()
                self.close()
            case .success(.string(let text)):
                self.handle(text)
                self.receiveLoop()
            case .success:
                self.receiveLoop()                              // 服务端不发二进制，忽略
            }
        }
    }

    private func handle(_ text: String) {
        guard let ev = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any],
              let h = ev["header"] as? [String: Any],
              let event = h["event"] as? String else { return }
        switch event {
        case "task-started":
            started = true
        case "result-generated":
            guard let out = (ev["payload"] as? [String: Any])?["output"] as? [String: Any],
                  let s = out["sentence"] as? [String: Any],
                  let txt = s["text"] as? String else { return }
            if (s["sentence_end"] as? Bool) == true {
                let tr = (out["translations"] as? [[String: Any]])?.first?["text"] as? String
                onFinal?(txt, tr)
            } else {
                onPreview?(txt)
            }
        case "task-finished":
            close()
        case "task-failed":
            started = false
            let code = h["error_code"] as? String ?? "InternalError"
            onError?(code, ["InvalidKey", "KeyExpired", "KeyRevoked", "DeviceMismatch"].contains(code))
            close()
        default:
            break
        }
    }

    private func reportHandshakeFailure() {
        guard !started else { onError?("ConnectionClosed", false); return }
        let status = (task.response as? HTTPURLResponse)?.statusCode
        switch status {
        case 401: onError?("Unauthorized", true)       // 正文为 InvalidKey / KeyExpired / KeyRevoked
        case 503: onError?("ServicePaused", false)
        default:  onError?("NetworkError", false)
        }
    }

    private func sendJSON(_ obj: [String: Any]) {
        let data = try! JSONSerialization.data(withJSONObject: obj)
        task.send(.string(String(decoding: data, as: UTF8.self))) { _ in }
    }

    private func close() {
        started = false
        DispatchQueue.main.async { self.pingTimer?.invalidate() }
        task?.cancel(with: .normalClosure, reason: nil)
    }
}
```

麦克风采集提示：`AVAudioEngine` 的输入节点通常是 44.1 / 48 kHz Float32，需要用 `AVAudioConverter` 转成 16 kHz、单声道、Int16 后，按 3200 字节一块调用 `sendAudio`。

---

## 11. 版本与兼容规则

### 11.1 版本号

| 版本 | 位置 | 当前值 | 含义 |
| --- | --- | --- | --- |
| 连接码版本 | 连接码 `v` | `1` | 连接码格式版本；终端不认识 ⇒ 拒绝并提示升级 |
| 协议版本 | `/v1/status` → `protocol` | `1` | WebSocket / HTTP 协议的大版本；不兼容的改动才会升 |
| 服务版本 | `/v1/status` → `version` | `0.1.0` | flashasr 程序版本（语义化版本号），用于判断某个增补功能是否可用 |

### 11.2 兼容约定

服务端承诺（协议 1 内）：

- 只新增**可选**的请求参数，缺省值保持原有行为；
- 只新增响应字段 / 事件 / 错误码，不删除、不改名、不改类型；
- 忽略终端发来的不认识的字段与参数。

终端须做到：

- **忽略不认识的字段**（包括 `output.feiyin` 下的新字段）；
- **忽略不认识的事件**（`header.event`）；
- 遇到不认识的 `error_code` 按 `InternalError` 处理（提示失败、可重试）；
- 可选扩展字段可能整体不出现，不要假定存在；
- 连接码 `v ≠ 1`、或 `/v1/status` 的 `protocol` 大于自己支持的版本 ⇒ 提示用户升级客户端。

### 11.3 如何判断服务端能力

连接前（或首次配对后）请求一次 `GET /v1/status`：

1. `protocol` 等于终端支持的版本 ⇒ 可接入。
2. `version` 用于判断增补功能。0.1.0 已包含：本机配对文件、`scene` 场景参数、终稿扩展字段（`wordbook_hits` / `learn_suggestions` / `optimization_skipped`）、`usage.speech_ms`、`EngineRestarted` 错误码、`finish-task` 改翻译开关、HTTP `/v1/status` 与声纹重置、本机终端不绑设备。
3. `llm_configured` 决定是否在界面上提供「输出优化」（没配时开了也会被跳过）。

> `/v1/status` 本身是 0.1.0 加入的。请求它得到非 HTTP 响应或握手失败（而网络正常）时，可视为更早的服务端：只支持基础识别、翻译与三项开关。

---

## 12. 常见问题

**Q1：TLS 握手总是失败 / 连上就断。**
依次检查：① 客户端是否启用了 TLS 1.3（服务端只支持 1.3；Android 需 10+）；② 同一 IP 最近是否连续 5 次鉴权失败（锁 10 分钟，期间连 TLS 都不做）；③ 服务端连接数是否已满 64；④ 防火墙 / 端口是否正确。

**Q2：本机 key 在别的电脑上用，返回 `InvalidKey`。**
本机终端的 key 只接受 `127.0.0.1` / `::1` 来源。其它设备请在管理界面「添加终端」拿自己的连接码。

**Q3：昨天拿到的连接码今天用不了，报 `KeyExpired` 或 `InvalidKey`。**
新 key 必须在 10 分钟内完成首次识别。过期后报 `KeyExpired`；服务端重启或再次「添加终端」时会清理过期 key，之后报 `InvalidKey`。重新添加终端即可。

**Q4：重装 App 后报 `DeviceMismatch`。**
`device_id` 丢了、重新生成了。远程终端的 key 首次使用时就和 `device_id` 绑定。请把 `device_id` 存在不会随 App 数据清除的位置（如 iOS Keychain）；已经丢了只能重新配对。

**Q5：收到了 `task-started`，紧接着又收到 `task-failed`（`ModelNotReady` / `ServiceBusy`）。**
这是正常的：`task-started` 只表示请求被受理，引擎侧的失败随后才回来。可先查 `/v1/status` 的 `local_realtime_ready` 再开始录音。

**Q6：预览文字会回退、改字。**
正常。预览是边听边猜，后面会用更准的结果整体替换。每条 `result-generated` 的 `text` 是**至今全文**，整体替换显示即可；末尾 `tentative` 个字建议浅色显示。最终以 `sentence_end = true` 的终稿为准。

**Q7：终稿是空串。**
没听到有效语音，或只有「嗯」「啊」之类语气词。此时没有译文、没有扩展字段，`task-finished` 照常到来。

**Q8：热词不生效。**
检查：纯英文 / 数字的词条不注入识别（`LiveKit` 不行，`鸿蒙OS` 可以）；单条不超过 10 个字符；最多 200 条且靠前优先；词表要每次 `run-task` 都带（服务端不保存终端词表）。

**Q9：开了输出优化，文字没有变化。**
看终稿 `output.feiyin.optimization_skipped`：`llm_not_configured` 表示服务端没配大模型（`/v1/status` 的 `llm_configured = false`）；`llm_failed` 表示调用失败。另外，同时开了翻译时，原文**不会**单独经过大模型（大模型的那一次调用用于「优化 + 翻译」，结果在 `translations` 里）。

**Q10：开了翻译却没有 `translations`。**
可能是：终稿为空；翻译失败或超时（离线翻译模型首次加载较慢，单次最长等 60 秒）；`finish-task` 里传了 `translation_enabled: false`。翻译失败不影响原文。

**Q11：能指定翻译成日文 / 从英文翻成中文吗？**
方向固定按内容判定：原文含汉字 ⇒ 英文；不含 ⇒ 中文。`translation_target_languages` 等参数被忽略。

**Q12：等终稿时连接断了。**
60 秒内终端没有发任何消息会被断开。说完后也要继续每 ≤ 30 秒发 Ping，直到收到 `task-finished` / `task-failed`。

**Q13：我想一次性把录音文件灌进去，越快越好。**
服务端每段识别的音频缓冲约 256 帧，远快于实时会导致超出部分被丢弃，识别结果缺字。请按实时节奏发送（100 ms 一帧）。

**Q14：怎么取消正在进行的识别？**
直接关闭 WebSocket 连接。没有取消消息。

**Q15：一个连接能同时识别两段吗？**
不能。同一连接同一时刻只能一段，否则 `InvalidParameter`（`a task is already running`）。要并发请开多条连接（受服务端 64 连接与识别通道数限制）。

**Q16：服务端证书指纹变了。**
服务端重装或证书被重新生成时指纹会变，也可能是网络里有中间人。终端必须断开并提示重新配对，**绝不能**自动接受新指纹。

**Q17：iOS 上连不上。**
① 确认在信任挑战里返回了 `.useCredential`（§10.4）；② 若 App Transport Security 拦截了对域名的连接，需要在 `Info.plist` 为该域名配置例外（连接局域网 IP 一般不受影响）；③ 首次访问局域网时系统会弹「本地网络」权限，需在 `Info.plist` 填写 `NSLocalNetworkUsageDescription`。

**Q18：换了使用人，旁人声音过滤不准。**
调用 `POST /v1/profile/voiceprint/reset` 清空本终端声纹，之后从零累积。终端被撤销时声纹会自动删除。

**Q19：标点开着，为什么短句末尾没有句号？**
标点收尾规则：不超过 5 个单位（每个汉字算 1、每个英文词 / 数字算 1）的短句自动去掉句末标点，适合聊天输入。需要始终去掉句末标点时用 `feiyin.strip_trailing_punct = true`；需要完全无标点时把 `punctuation_prediction_enabled` 设为 `false`。

**Q20：`model`、`task_group` 这些字段要填什么？**
随便填或不填，服务端忽略。保留它们只是为了与百炼代码通用。

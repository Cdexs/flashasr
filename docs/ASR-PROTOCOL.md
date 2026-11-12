# flashasr · 终端接入协议 v1

> 给终端（飞音输入法、安卓 / iOS / 第三方程序）照着对接的契约。只描述线上能看到的行为，不依赖本仓库任何源码。
> 本文与前台程序 `flashasr`（`src/front/`）逐条对应；两者不一致时以程序行为为准，并回来改本文。
> 参考实现：`tools/asr-client/feiyin_asr_client.py`（Python，可直接跑通全流程，见 §9）。
> 版本：v1（2026-10-17 一期；2026-10-29 增补 §1.5、§4.1 场景、§5.2 终稿扩展、§5.4 `speech_ms` / `EngineRestarted`、§5.5 HTTP 接口；2026-11-12 增补 6 位数字配对码校验、用户维度 `user_id` 跨终端共享词库/声纹、WebSocket 动作 `pair-terminal` / `get-wordbook` / `update-wordbook`，均向后兼容）。
>
> **完整接入文档见 `docs/API.md`**（含各语言示例）；本文与 API.md 不一致时**以 API.md 为准**。

## 0. 一句话

**照阿里云百炼实时识别的 `run-task` WebSocket 协议发**（同名事件、同名字段），差别只有四处：
① 地址、key、证书指纹及 **6 位数字配对码**来自**配对**（§1）；② 走 `wss://` 且**只认配对时的证书指纹**（§2）；
③ 鉴权/识别必须带用户与本机身份 `feiyin.user_id / device_id / device_name`（首次远程配对必带 `feiyin.pair_code`）（§1.4 / §4.1）；
④ 一期整段一句：`sentence_id` 恒为 1，`text` 每次给**至今全文**（§5.2）。并在同一条 WebSocket 上扩展了 `pair-terminal`、`get-wordbook`、`update-wordbook`（§4.5）。

---

## 1. 配对：拿到地址、key、6 位配对码、指纹

服务端（运行 flashasr 的电脑）「添加终端」后给出二维码、连接码（二维码的文字版）、分开显示的地址 + key + **6 位数字配对码（`pair_code`）**。

### 1.1 连接码 / 二维码内容

```
flash-asr://pair?v=1&lan=192.168.1.5:7860&wan=asr.example.com:7860&key=ABCDEFGHIJKLMNOPQRSTUVWXYZ&fp=3f9a…（64 位十六进制）
```

> **注意**：6 位数字配对码（`pair_code`）不拼入二维码与连接码 URI 中，而是独立展示在服务端管理界面弹窗中，由用户在客户端配对时手输校验（防止连接码文本意外泄露即导致终端被冒用）。

| 字段 | 必有 | 说明 |
| --- | --- | --- |
| `v` | ✅ | 协议版本，当前只认 `1`；不是 `1` ⇒ 拒绝并提示升级 |
| `lan` | 二选一 | 内网地址 `主机:端口`。**字段总会出现，但值可能为空**（服务端取不到本机内网地址时）⇒ 当作没有内网地址，只用 `wan` |
| `wan` | 二选一 | 外网地址 `主机:端口`（服务端没配外网时不出现） |
| `key` | ✅ | 终端凭证：RFC 4648 base32（大写、无填充），16 字节随机数 ⇒ 26 字符 |
| `fp` | ✅ | 服务端证书指纹：证书 DER 的 SHA-256，**小写**十六进制 64 字符 |

- 值可能被百分号编码，解析时**先解码、再按 §1.2 规整 key**。**不认识的字段忽略**（以后加字段不破坏老终端）；同名字段重复出现时取最后一个。
- 指纹比对时宽容处理：去掉空白和冒号、转小写后再比（`3F:9A:…` 与 `3f9a…` 视为相同）。
- 地址没写端口 ⇒ 补 `7860`；IPv6 主机须带方括号（`[fe80::1]:7860`）。

### 1.2 手输

用户分别输入内网 / 外网地址（至少一个）与 key；指纹可空。地址规则同 §1.1（缺端口补 7860、IPv6 带方括号）。
- key 规整：**去掉所有空白和 `-`、转大写**后再用（服务端也按同样规则比对）。
- 指纹为空 ⇒ 先只做 TLS 握手取回服务端证书指纹（不发 key），显示给用户核对（服务端设置页有同一串指纹），用户确认后才保存。**未经确认的指纹不得保存、不得带 key 连接。**

### 1.3 key 与 6 位配对码的生命周期（终端需要知道的部分）

- 新 key 与 **6 位数字配对码（`pair_code`）** 在 **10 分钟内**必须完成首次绑定（发 `pair-terminal`、`run-task`、`get-wordbook` 或 `update-wordbook`），否则过期（`KeyExpired`）⇒ 让用户重新添加终端。服务端会定期清理过期的 key，清理之后再用它报 `InvalidKey`。
- 首次绑定时服务端同时校验 `key` + **6 位数字 `pair_code`**（错码或缺失 ⇒ `InvalidPairCode`）+ `user_id` + `device_id` + `device_name`。全部通过后，该终端永久绑定 `(user_id, device_id)`，之后日常重连**无需再传 `pair_code`**（若传了也须正确）。
- 后续重连校验 `user_id` 与 `device_id` 不得变更（换用户 ⇒ `UserMismatch`，换设备 ⇒ `DeviceMismatch`）。**本机终端（§1.5）例外**：免 `pair_code` 校验且不绑死单台设备。
- 服务端可随时撤销（`KeyRevoked`），撤销时在线连接立即断开。若同一 `user_id` 下还有其它未撤销终端，该用户的声纹与词库**保留不变**。
- 以上认证错误终端都应提示「需要重新配对」，**不要自动重试**。

### 1.4 用户与本机身份

| 字段 | 规则 |
| --- | --- |
| `user_id` | **1～12 位大小写字母 + 数字，且必须以字母开头**（`^[A-Za-z][A-Za-z0-9]{0,11}$`，区分大小写）。服务端将词库与声纹挂载在 `user_id` 下，同一用户在电脑、手机等多个终端连接共享同一份词库和声纹 |
| `device_id` | 由前端根据设备 **CPU + 主板序列号**生成的唯一序列号，**1～32 位**非空白 ASCII 字符，存本地且固定不变 |
| `device_name` | 给服务端用户看的名字（如「Gavin 的 iPhone」），可随时改；改了下次请求自动更新 |
| `pair_code` | 服务端生成的 **6 位数字配对码（`000000`～`999999`）**，首次绑定远程终端时必填 |

### 1.5 本机自动配对（终端与 flashasr 在同一台电脑）

flashasr 每次启动都会保证有一个「本机终端」，并把它的连接码写到当前用户目录：

| 系统 | 路径 |
| --- | --- |
| Windows | `%APPDATA%\flashasr\local-pairing.json` |
| macOS | `~/Library/Application Support/flashasr/local-pairing.json`（权限 0600） |

```json
{ "terminal_id": "pgzmdppb", "uri": "flash-asr://pair?v=1&lan=127.0.0.1:7860&key=…&fp=…", "port": 7860 }
```

- `uri` 按 §1.1 解析即可，地址固定 `127.0.0.1`。
- 本机终端的 key **永不过期**，但**只接受回环地址（127.0.0.1 / ::1）来的连接**；从别的机器用它连 ⇒ `InvalidKey`。
- 本机终端**不绑设备**（§1.3 的 `DeviceMismatch` 不适用）：本机上的任何程序都可以用它（输入法、测试客户端轮流用互不影响），服务端只记最近一次连接的 `device_id` / `device_name`。
- 被撤销或文件丢失 ⇒ flashasr 下次启动重建；改了端口 ⇒ key 不变、地址更新。终端连接失败时应重新读一次此文件。

---

## 2. 连接

### 2.1 选地址（内网 / 外网）

终端提供「自动 / 内网 / 外网」三选一，默认自动。**自动的建议做法**：两个地址同时试 TCP 连接（不带 key）；内网通了直接用内网；外网先通 ⇒ 再等内网 300ms，内网仍不通才用外网。每次开始识别前重新挑（进出家门自动切换）。

### 2.2 TLS（必须先验指纹，再发 key）

- 只用 **TLS 1.3**。服务端是自签证书：**不做 CA 校验、不校验主机名**，改为**钉扎指纹**——握手完成后取服务端证书 DER，算 SHA-256，与配对时的 `fp` 比较（忽略大小写），不一致 ⇒ 立即断开，提示「服务端证书变了，需要重新配对」。
- 🔴 **顺序硬要求**：指纹比对通过之后才能发 WebSocket 升级请求。key 在升级请求头里，先发后验等于把 key 交给中间人。

### 2.3 WebSocket 升级

```
GET wss://<主机:端口>/ HTTP/1.1
Authorization: Bearer <key>
```

路径不限（服务端不看路径，建议用 `/`）。key 用 §1.2 规整后的形式。被拒时正文是纯文本错误码（ASCII，很短；有 `Content-Length` 按它读，没有就读到连接关闭）。

| 升级结果 | 含义 | 终端处理 |
| --- | --- | --- |
| `101` | 成功 | 进入 §3 |
| `401`，正文 `InvalidKey` / `KeyExpired` / `KeyRevoked` | key 不对 / 过期 / 撤销 | 提示重新配对，不重试 |
| `503`，正文 `ServicePaused` | 服务端暂停接入 | 提示「对方已暂停」，稍后再试 |
| TCP 直接被断 / 握手无响应 | 同一 IP **连续 5 次**鉴权失败 ⇒ 锁 **10 分钟**（锁定期内连 TLS 都不做）；或网络不通 | 不要循环重连 |

握手（TLS + 升级）须在 **10 秒**内完成。

---

## 3. 连接上的规则

| 项 | 值 |
| --- | --- |
| 同一连接可依次跑多段识别 | ✅，但**同一时刻只能一段**（上一段收到 `task-finished` / `task-failed` 后才能发下一个 `run-task`） |
| 空闲断开 | **60 秒**收不到终端任何消息（含音频、Ping）⇒ 服务端断开。长时间挂着不说话的终端请每 ≤30 秒发一次 WebSocket Ping。识别进行中持续发音频即算活动，不必另发；`finish-task` 之后等终稿通常只需数秒 |
| 单条消息上限 | 1 MB |
| 服务端同时在线连接上限 | 64 |
| 服务端主动断开 | 撤销、鉴权失败、空闲超时、服务端退出 |

---

## 4. 终端 → 服务端

所有文本消息都是 JSON：`{"header": {...}, "payload": {...}}`。

### 4.1 `run-task`（开始一段识别）

```json
{
  "header": { "action": "run-task", "task_id": "8c1f…（每段新生成，建议 32 位十六进制）", "streaming": "duplex" },
  "payload": {
    "task_group": "audio", "task": "asr", "function": "recognition",
    "model": "feiyin-local",
    "input": {},
    "parameters": {
      "format": "pcm",
      "sample_rate": 16000,
      "inverse_text_normalization_enabled": true,
      "punctuation_prediction_enabled": true,
      "translation_enabled": false,
      "vocabulary": ["飞音", "LiveKit"],
      "feiyin": {
        "user_id": "Gavin01",
        "device_id": "CPU9F8A7B6CMB123456",
        "device_name": "Gavin 的 iPhone",
        "pair_code": "482910"
      }
    }
  }
}
```

| 字段 | 必填 | 缺省 | 说明 |
| --- | --- | --- | --- |
| `header.task_id` | ✅ | — | 空 ⇒ `InvalidParameter`。后续 `finish-task` 与所有事件都带它 |
| `parameters.format` | | `pcm` | 一期**只收** `pcm`（大小写不限），其它 ⇒ `UnsupportedFormat` |
| `parameters.sample_rate` | | `16000` | 一期**只收** 16000，其它 ⇒ `UnsupportedFormat` |
| `parameters.inverse_text_normalization_enabled` | | `true` | 数字规整（「三点五公斤」⇒「3.5公斤」） |
| `parameters.punctuation_prediction_enabled` | | `true` | 补标点；`false` ⇒ 预览、终稿、译文**全部去标点** |
| `parameters.translation_enabled` | | `false` | 终稿另给一份译文（§5.3）。方向按内容自动：含汉字 ⇒ 译成英文，否则译成中文；`translation_target_languages` 忽略。**翻译引擎跟输出优化开关**：开了输出优化 ⇒ 大模型一次完成「优化 + 翻译」（此时原文不再单独优化）；没开 ⇒ 服务端离线翻译模型 |
| `parameters.vocabulary` | | 无 | 额外热词（字符串数组或 `{词: 权重}`）。识别时服务端会自动读取该 `user_id` 存储在服务端的词库（用户词条 + 系统词条，按命中频次排序），并与这里附加的词去重合并注入精解与大模型 |
| `parameters.feiyin.user_id` | ✅ | — | §1.4，1～12 位字母开头+字母数字。不合规 ⇒ `InvalidUserId` 并断开；同一连接已鉴权后可省略 |
| `parameters.feiyin.device_id` / `device_name` | ✅ | — | §1.4，最长 32 位硬件序列号与设备名。缺/超长 ⇒ `MissingDeviceInfo` 并断开；同一连接已鉴权后可省略 |
| `parameters.feiyin.pair_code` | 首次 | — | 6 位数字配对码（首次绑定远程终端必填；已绑定或本机终端可省略）。错/缺失 ⇒ `InvalidPairCode` 并断开 |
| `parameters.feiyin.output_optimization_enabled` | | `false` | 输出优化（大模型整理文字）。大模型用服务端统一配置；服务端没配 ⇒ 跳过此步、照常出结果 |
| `parameters.feiyin.strip_trailing_punct` | | `false` | 去掉句末标点（句中标点保留） |
| `parameters.feiyin.chinese_script` | | 服务端设置 | `"simplified"` / `"traditional"`；其它值 ⇒ 跟服务端设置 |
| `parameters.feiyin.word_times` | | `true` | 终稿要不要精确逐字时间；`false` ⇒ 说完后最后一段不做强制对齐（终稿约快 0.25s，最后一段时间改估算，文字不变） |
| `parameters.feiyin.scene` | | 未知场景 | 输入场景，供输出优化 / 大模型翻译调整写法：`{"kind": "chat", "style_hint": "", "multiline_safe": false}`。`kind` 取 `chat` / `agent` / `email` / `doc` / `ide_terminal` / `browser`，其它 ⇒ 未知；`style_hint` 自由文字风格提示（如「正式」）；`multiline_safe` = 目标输入框能否接受换行。窗口标题、程序名**不要传**（隐私） |
| `header.streaming`、`payload.task_group / task / function / model / input` | | — | 服务端忽略，照百炼写即可（方便同一份代码对接百炼） |

**固定内置、不可关**：同音字纠错、去口水词与口吃重复、无效文本判定（只有「嗯啊」之类 ⇒ 终稿为空）。

**声纹**：服务端将声纹挂载在 **`user_id`** 维度下（同一用户的电脑、手机终端共享），攒够后自动剔除别人的声音。可由用户主动重置（§5.5），或当该用户全部终端均被撤销时清理。

**系统词库自动学习**：终稿输出优化产生的 `learn_suggestions` 会在服务端按现有候选校验与晋升阈值自动累积并写入该 `user_id` 的系统词库（`source="system"`），同时记录词频命中次数。

成功 ⇒ 服务端回 `task-started`（§5.1），**之后**才能发音频。失败 ⇒ `task-failed`（§5.4）；**注意 `task-started` 只代表请求被受理，不代表识别通道已开好**：`ModelNotReady` / `ServiceBusy` 是在 `task-started` 之后才以 `task-failed` 告知的（识别中途还可能 `EngineRestarted`），终端收到 `task-started` 后仍须处理 `task-failed`。其中 `KeyRevoked` / `KeyExpired` / `InvalidKey` / `DeviceMismatch` / `MissingDeviceInfo` 之后服务端**断开连接**。

### 4.2 音频：二进制帧

- **16 kHz、单声道、16 位有符号小端 PCM**，裸数据（无 WAV 头）。
- 建议每帧 **100 ms**（3200 字节），实时节奏发送。帧长不必固定，但单帧 ≤ 1 MB。
- `task-started` 之前或没有进行中的识别时发的音频**被丢弃**。

### 4.3 `finish-task`（说完了）

```json
{ "header": { "action": "finish-task", "task_id": "<同 run-task>", "streaming": "duplex" }, "payload": { "input": {} } }
```

可选：`payload.parameters.translation_enabled`（布尔）——**说完时**定下的翻译开关，覆盖 `run-task` 里的值；不带 ⇒ 沿用 `run-task` 的。用于「录音中途才决定要翻译」的终端（翻译只在终稿阶段做，所以说完时决定不晚）：

```json
{ "header": { "action": "finish-task", "task_id": "<同 run-task>", "streaming": "duplex" },
  "payload": { "input": {}, "parameters": { "translation_enabled": true } } }
```

`task_id` 对不上 / 没有进行中的识别 ⇒ `task-failed`（`InvalidParameter`，`no such task`），连接保持。发完后继续收事件，直到 `task-finished` / `task-failed`。

### 4.4 取消

一期没有取消消息：**直接关闭连接**即取消（服务端丢弃该段，不出终稿）。

### 4.5 配对验证与用户词库同步动作（WebSocket 扩展）

除 `run-task` / `finish-task` 外，同一条 WebSocket 连接还支持以下 3 个动作（`payload.feiyin` 在同一连接已鉴权后可省略）：

1. **`pair-terminal`（配对立即验证 6 位数字配对码并绑定）**
   - 请求：`{"header":{"action":"pair-terminal","task_id":"..."},"payload":{"feiyin":{"user_id":"Gavin01","device_id":"...","device_name":"...","pair_code":"482910"}}}`
   - 成功响应事件 `terminal-paired`：`{"header":{"task_id":"...","event":"terminal-paired","attributes":{}},"payload":{"terminal_id":"...","user_id":"Gavin01","device_id":"...","device_name":"..."}}`
2. **`update-wordbook`（全量更新当前用户的用户词条；不支持客户端更新系统词条）**
   - 请求：`{"header":{"action":"update-wordbook","task_id":"..."},"payload":{"feiyin":{...},"user_words":["飞音","FlashASR"]}}`（若带 `system_words` 字段将被拒绝，报 `InvalidParameter`）
   - 成功响应事件 `wordbook-updated`：`payload` 返回 `user_id`、`user_count`、`system_count`、`user_words`、`system_words`
3. **`get-wordbook`（查询当前用户最新词库：含用户词条与系统词条）**
   - 请求：`{"header":{"action":"get-wordbook","task_id":"..."},"payload":{"feiyin":{...}}}`
   - 成功响应事件 `wordbook-result`：`payload` 返回 `user_id`、`user_count`、`system_count`、`user_words`、`system_words` 及完整 `entries`（含 `word`、`source`、`hit_count`、`created_at`、`last_used_at`）

---

## 5. 服务端 → 终端

事件统一形如 `{"header": {"task_id", "event", "attributes": {}}, "payload": {...}}`。

### 5.1 `task-started`

`payload` 为空对象。收到后开始发音频。

### 5.2 `result-generated`

```json
{
  "header": { "task_id": "8c1f…", "event": "result-generated", "attributes": {} },
  "payload": {
    "output": {
      "sentence": {
        "sentence_id": 1,
        "begin_time": 120,
        "end_time": 2380,
        "text": "今天买了3.5公斤苹果。",
        "sentence_end": false,
        "words": [
          { "begin_time": 120, "end_time": 400, "text": "今", "punctuation": "" },
          { "begin_time": 2100, "end_time": 2380, "text": "果", "punctuation": "。" }
        ]
      },
      "feiyin": { "tentative": 3, "ts_source": ["aligned", "rough"] }
    },
    "usage": null
  }
}
```

**怎么用**：

| 字段 | 语义 |
| --- | --- |
| `sentence_id` | 一期恒为 1（整段一句） |
| `text` | **本段识别至今的全文**。每条预览都**整体替换**上一条，不要拼接 |
| `sentence_end` | `false` = 预览，后面还会变；`true` = **终稿**，本段只出现一次，之后不再变 |
| `feiyin.tentative` | 预览里 `text` **末尾**还没稳定的字数（建议浅色显示）；终稿恒 0 |
| `words` | 逐词时间：`begin_time` / `end_time` 为毫秒，**相对本段第一帧音频**。中文一字一词，连写的英文 / 数字合成一词；标点挂在前一个词的 `punctuation` 上。保证：所有词 `text + punctuation` 依次拼接 == `sentence.text`（空格也挂在词上，逐字符相等）。预览里的时间是粗略的，可能为空数组 |
| `begin_time` / `end_time`（句） | 首词起点 / 末词终点；没有词时 `begin_time` 为 0、`end_time` 为 `null` |
| `feiyin.ts_source` | 与 `words` 一一对应的时间来源：`aligned`（强制对齐，准）/ `rough`（识别模型自带，粗）/ `estimated`（插值估算）。终稿里每个词都有时间，补不出的才是 `estimated` |

> 阿里客户端只认 `output.sentence` 的终端可直接用；`output.feiyin` 是扩展字段，可忽略。
> 终稿可能是**空文本**（没听到有效语音 / 只有口水词）。

**终稿额外字段**（只在 `sentence_end = true` 的那条，都在 `output.feiyin` 下；为空时**不出现**）：

| 字段 | 语义 |
| --- | --- |
| `wordbook_hits` | 字符串数组：`vocabulary` 里出现在终稿中的词（去重、按 `vocabulary` 顺序）。终端可据此记词库命中次数 |
| `learn_suggestions` | 字符串数组：输出优化建议加进词库的词（只有输出优化做成时才可能有）。是否加入由终端 / 用户决定 |
| `optimization_skipped` | 要了输出优化但没做成的原因：`llm_not_configured`（服务端没开或没填大模型）/ `llm_failed`（调用失败或返回空）。此时终稿是不经大模型的结果，可提示用户 |

### 5.3 译文（只在终稿、且 `translation_enabled = true` 时）

终稿的 `payload.output` 里多一个（阿里 Gummy 同形）：

```json
"translations": [ { "sentence_id": 1, "lang": "en", "text": "I bought 3.5 kg of apples today.", "sentence_end": true } ]
```

- `lang`：`en` 或 `zh`。译文不带逐词时间。
- 引擎跟输出优化开关（§4.1）：开了 ⇒ 大模型；没开 ⇒ 离线翻译模型（首次要加载模型，可能多等几秒）。
- 翻译失败 ⇒ **没有** `translations` 键，原文照常。

### 5.4 `task-finished` / `task-failed`

```json
{ "header": { "task_id": "8c1f…", "event": "task-finished", "attributes": {} },
  "payload": { "output": {}, "usage": { "duration": 5, "speech_ms": 4448 } } }
```

`usage.duration` = 本段音频秒数（向上取整）；`usage.speech_ms` = 去掉静音后的有效语音毫秒数（服务端语音检测不可用时等于整段长度），终端可直接用于用量统计。之后可在同一连接发下一个 `run-task`。

```json
{ "header": { "task_id": "8c1f…", "event": "task-failed", "error_code": "ServiceBusy", "error_message": "capacity", "attributes": {} },
  "payload": {} }
```

| `error_code` | 何时 | 连接 | 终端提示建议 |
| --- | --- | --- | --- |
| `InvalidParameter` | JSON 坏、未知 `action`、缺 `task_id`、已有进行中的识别、`finish-task` 对不上 | 保持 | 程序错误，记日志 |
| `UnsupportedFormat` | 非 pcm / 非 16000Hz | 保持 | 程序错误 |
| `MissingDeviceInfo` | 缺 `device_id` / `device_name` 或 `device_id` 超 32 位 | **断开** | 程序错误 |
| `InvalidPairCode` | 首次绑定未传 6 位数字配对码或配对码错误 | **断开** | 配对码错误，请核对后重试 |
| `InvalidUserId` | `user_id` 缺失或不符合「字母开头、最长 12 位字母数字」 | **断开** | 用户 ID 格式不合法 |
| `UserMismatch` | `user_id` 与该终端已绑定的用户不一致 | **断开** | 需要重新配对 |
| `InvalidKey` / `KeyExpired` / `KeyRevoked` / `DeviceMismatch` | 见 §1.3 | **断开** | 需要重新配对 |
| `ServicePaused` | 服务端暂停接入 | 保持 | 对方已暂停，稍后再试 |
| `ServiceBusy` | 服务端推理通道已满 | 保持 | 对方正忙，稍后再试 |
| `ModelNotReady` | 服务端识别模型还没加载完 / 引擎不在线 | 保持 | 对方识别模型未就绪，稍后再试（可先查 §5.5 `/v1/status`） |
| `EngineRestarted` | 进行中的识别遇到服务端引擎崩溃（约 5 秒后自动拉起） | 保持 | 识别中断，请重说 |
| `InternalError` | 其它 | 保持 | 识别失败，可重试 |

进行中的识别遇到撤销：先收到 `task-failed`（`KeyRevoked`）再断开。

### 5.5 HTTP 接口（与 WebSocket 同一端口、同一 TLS）

同样**先钉扎指纹再发 key**（§2.2）；鉴权失败与 WebSocket 一样计入 IP 锁定。返回 `Content-Type: application/json`、`Connection: close`。

| 请求 | 作用 | 成功返回 |
| --- | --- | --- |
| `GET /v1/status` + `Authorization: Bearer <key>` | 服务状态，终端据此决定能否开始识别 | `200` `{"service":"flashasr","version":"0.1.0","protocol":1,"engine_online":true,"local_realtime_ready":true,"llm_configured":false,"paused":false,"max_sessions":8,"active_sessions":1,"free_sessions":7}` |
| `POST /v1/profile/voiceprint/reset` + `Authorization: Bearer <key>`（无正文） | 清空**本终端自己**的声纹，之后从零重新累积（终端由 key 确定，不收其它参数） | `200` `{"reset":true}` |

- `engine_online = false` ⇒ `local_realtime_ready` / `llm_configured` 恒为 `false`。
- `llm_configured` = 服务端开了大模型且填了地址和 key（输出优化可用）。
- `max_sessions` = 服务端能同时进行的识别路数上限（所有终端合计）；`active_sessions` = 此刻正在进行的识别路数；`free_sessions` = 二者之差（不小于 0）。`free_sessions = 0` 时新的 `run-task` 多半会收到 `ServiceBusy`（终端可据此先提示「对方正忙」）。
  - **上限是估算值**：识别模型加载完时按当时的空闲显存算（每路约 150MB，先给本机留 1GB，硬顶 8；查不到显存时取硬顶 8），之后不再变。运行中显存被别的程序占掉时实际可开的会更少，以 `ServiceBusy` 为准。
  - 模型没加载完 / 引擎不在线 ⇒ `max_sessions` 与 `free_sessions` 为 0。
- 没带 key / key 无效 ⇒ `401`，正文 `{"error":"InvalidKey"}`（不区分过期 / 撤销）。路径或方法不对 ⇒ 当作 WebSocket 升级处理（会失败）。

---

## 6. 一段识别的完整时序

```
终端                                   服务端
 │ TCP + TLS1.3 ───────────────────────▶│
 │ （本地比对证书指纹，通过才继续）       │
 │ GET / + Authorization: Bearer key ──▶│ 401/503 ⇒ 结束
 │◀──────────────────────────── 101     │
 │ run-task ───────────────────────────▶│
 │◀──────────────────────── task-started│
 │ 音频 100ms ─────────────────────────▶│
 │ 音频 100ms … ───────────────────────▶│
 │◀── result-generated（预览，反复多条） │
 │ finish-task ────────────────────────▶│
 │◀── result-generated（sentence_end=true，终稿；可含 translations）
 │◀─────────────────────── task-finished│
 │ （可再发 run-task，或关闭连接）        │
```

---

## 7. 与阿里百炼的差异速查

| 项 | 百炼 | 飞音 v1 |
| --- | --- | --- |
| 鉴权 | `Authorization: bearer <API Key>` | 同写法，key 来自配对；另须钉扎证书指纹 |
| 终端身份 | 无 | `parameters.feiyin.device_id / device_name` 必填 |
| 音频格式 | pcm / opus / wav… 多采样率 | 一期只 pcm 16k |
| 句子 | 多句，每句一个 `sentence_id` | 一期整段一句，`text` 为全文 |
| 心跳 | `heartbeat` 参数 | 一期没有；用 WebSocket Ping 保活（§3） |
| 扩展 | — | `output.feiyin.tentative / ts_source`；终稿 `wordbook_hits / learn_suggestions / optimization_skipped`；`usage.speech_ms`；HTTP `/v1/status`、声纹重置 |

## 8. 二期预告（未实现，终端不要依赖）

Opus 音频、断线续传、`heartbeat` 保活包、手机走 LiveKit（WebRTC）音频。届时协议版本升级并保持 v1 兼容。

## 9. 参考客户端

`tools/asr-client/feiyin_asr_client.py`：解析连接码 → 选地址 → TLS 钉扎 → 升级 → `run-task` → 按实时节奏发 WAV / PCM → 打印预览与终稿 → `task-finished`。用法见脚本头部说明。它同时是服务端的端到端冒烟测试。

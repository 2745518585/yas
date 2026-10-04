# 网页扫描接口（协议版本 1）

服务仅支持 Windows，地址为 `http://127.0.0.1:32334`。

网页生成 32 字节随机令牌，编码为 64 位十六进制字符串，然后在用户点击时打开：

```text
yas-scan://connect?origin=<encodeURIComponent(location.origin)>&token=<token>
```

唤起参数仅接受规范的 HTTPS 来源（开发时允许 HTTP localhost、127.0.0.1、[::1]）及令牌。YAS 使用原生 Windows 弹窗让用户确认来源。服务已运行时，新进程把授权申请交给现有服务。网站不能自行授予权限。

所有 HTTP 请求的 Host 必须是 `127.0.0.1:32334` 或 `localhost:32334`。除 info 外，请求必须携带合法的 Origin；除 connect 外，还必须携带 `Authorization: Bearer <token>`。令牌只对用户授权的 Origin 有效。会话闲置八小时后失效，拒绝授权需要生成新令牌后重试。

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| GET | `/api/info` | 返回 product=`yas-web`、protocolVersion=1、程序版本 |
| POST | `/api/connect` | 已运行服务的连接申请，JSON `{ "token": "..." }`；触发原生授权弹窗 |
| GET | `/api/session` | 200=已授权，404=尚无会话，403=拒绝或过期 |
| GET | `/api/windows` | 列出 `{ hwnd, title }` 的原神窗口 |
| POST | `/api/scan` | JSON `{ "hwnd": 123, "minStar": 5, "minLevel": 0, "number": 0 }`；返回 `{ "job": "..." }` |
| GET | `/api/status?job=...&after=0` | 返回 job、state、error、next 和 `{ seq, text }` 日志列表 |
| GET | `/api/result?job=...` | 仅 completed 可读取，返回 Mona 格式 JSON |
| POST | `/api/cancel` | JSON `{ "job": "..." }`，结束当前网页自己的扫描进程 |
| OPTIONS | 任意路径 | CORS 预检与本机网络访问预检 |

minStar 取 1～5、minLevel 取 0～20、number 取 0～10000（0 为不限数量）。未知字段会被拒绝；服务将参数作为单独的进程参数传给当前 YAS 可执行文件，不调用 shell，不接受命令行字符串。服务全局同时只运行一个扫描，任务 ID 和结果归属于发起会话，最多保存八个任务、每个任务最近两千行日志。扫描参数、游戏窗口和结果均在本机处理。

任务状态包括 running、cancelling、cancelled、completed、failed。网页应每隔约 500ms～1s 读取 status，并把 next 用于下一次 after。日志有限保留，轮询应允许序号跳跃。网页失联 120 秒或扫描运行 30 分钟后取消；退出服务会通过 Windows Job Object 结束子进程。每次任务创建独立临时输出目录，只读取此次成功生成且非空的 mona.json。中断、识别错误及翻页错误会使无交互扫描失败，避免把部分结果当成完整背包。

服务不接受通用键鼠操作、指定可执行文件、输出路径、任意命令或自动更新请求。网页应在导入前校验全部数据，并让用户明确选择是否删除未扫描到的物品。浏览器若禁止本机网络访问，可以继续走文件导入。

命令行新增 `--serve`、`--install-web`、`--uninstall-web`；通用版本使用 `yas.exe genshin <选项>`。`--no-pause` 用于自动化，退出不等待按键，要求扫描完整且以非零状态报告错误；多个游戏窗口时使用 `--hwnd` 指定窗口。正常交互扫描仍允许中止后导出部分结果。

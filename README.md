# Client Flight — Horizon 版

Minecraft 26.3 的 [Horizon](https://github.com/horizon-rust/horizon) 客户端 mod。
移植自 [Client-Flight-Mod](https://github.com/ReallyChooseC/Client-Flight-Mod)（Fabric / NeoForge 版）。

## 装

Horizon launcher → Mods 页面 → **Add from GitHub** → 填：

```
https://github.com/czw63/horizon-client-flight
```

launcher 会取本仓库源码，用它自带的 Rust 工具链编一遍（只编 mod 自己，不动游戏本体）。
不需要预装 Rust。

## 做了什么

五个 `[[native.mixin]]`，全部在 `horizon-client` 的 game crate 里，编完 `native-build` 报
`Function replacement: 5 functions patched by 5 mixins`。

| id | 挂点 | 作用 |
| --- | --- | --- |
| `abilities` | `apply_remote_abilities`（OVERWRITE） | 服务端下发的 abilities 里强制 `may_fly = true`，其余字段照原样写回。客户端飞行的前提。 |
| `nofall` | `remote_send_state`（HEAD） | 落地那一下发三段位置包：`(y,false)` → `(y+0.01,false)` → `(y,true)`，每段后补 `ClientTickEnd`，把服务端的下落距离清零。**默认关闭**。 |
| `correction` | `remote_teleport`（HEAD，cancellable） | 服务端位置校正包如果匹配自己发出去的包，就吞掉位置变更、回 `AcceptTeleportation`、再执行被推迟的交互。 |
| `correction_sent_move` | `send_play`（BEFORE_CALL） | 记录自己发出去的移动包，给上面那个 probe 用。 |
| `flight_velocity` | `apply_packet`（HEAD，cancellable） | 受伤时服务端会重发它自己累积的重力。只在「本地玩家 + 受伤中 + 下发垂直速度 < 0」时，把那个 y 换成玩家自己的速度，避免一次受伤被打断。 |
| `probe_tick` | `tick_local_player`（HEAD） | 每 tick 跑一次探测器：找碰撞目标、建立 marker probe、连发四个包勾服务端下发传送。整套机制的发动机。 |
| `probe_interaction` | `remote_use`（HEAD，cancellable） | 等待传送确认期间把方块交互推迟，确认后再执行。 |
| `probe_replay` | `apply_packet`（HEAD，cancellable） | 收到传送包后重放被推迟的交互，执行前重新校验玩家/生命/界面/手持物/准星。 |

## 工作原理

Minecraft 服务端只在自己检测到位置异常时才下发传送包，而那个包带回执确认。
这套 mod 利用它把「空中的位置」变成服务端认可的合法位置：

1. 每 tick 探测：向下 8 格找地板、向上 8 格找天花板、朝四面各 6 格找墙，
   取一个「玩家挪过去会卡在方块里」的位置（故意挑非法的）
2. 发 `MovePlayerPosRot(真实位置, marker pitch)` + `ClientTickEnd`
   + `MovePlayerPos(非法位置)` + `ClientTickEnd`，主动勾服务端发传送
3. 服务端的传送包下来后，靠 marker pitch 认出这是自己勾的，拦掉位置变更（玩家留在空中），
   但回 `AcceptTeleportation(id, 真实位置)` —— 26.3 这个回执带位置，服务端据此把玩家的
   合法基准更新成你在空中的真实坐标
4. 循环，飞行位置被持续合法化

marker pitch 不是随便改小数：按 `sequence & 63` 的偏移量在 ulp 级别上下挪，
保证每次探测的指纹都不同，才能准确认出「这个传送是我勾的」而不是服务端真的抓你。

状态和设置放在 crate 自己的模块里（`src/nofall.rs`、`src/correction.rs`），helper 只调用它们。

## 已知限制（重要，别当它能干所有事）

- **位置校正拦截的探测精度和原版有差异。** Minecraft 的 `level.clip` 是精确的体素遍历，
  这里是自己实现的采样遍历（0.1 步长发现候选格，命中判定用完整线段交给方块形状自己算）。
  极薄的碰撞箱（比如地毯边缘）在斜射线上可能被漏掉，漏了就这一轮不探测，下一轮再来。
- **交互延迟只覆盖方块交互**，实体交互和挖掘没接。
- **没有鞘翅（elytra）功能。** 原版那套是取消 `LivingEntity.travel` 自己算物理，
  Horizon 里这个函数没标 `replaceable`，要做得整个重写，这版没做。
- **没有 `/cfly` 命令、没有 Mods 配置页、没有快捷键。** 目前只有 `nofall` 有个开关
  （`client_flight::nofall::set_enabled`），默认关，暂时没有游戏内入口。
- **`flight_velocity` 的飞行判据比原版宽。** 它只看本地 `may_fly && flying && hurt_time > 0 && 包里的 y < 0`，
  没有搬 Java `isClientFlying` 里那套飞行归属追踪和创造/旁观/落地/乘坐/鞘翅/流体/攀爬的排除条件。
  正常飞行场景下够用，但极端组合下可能比原版多管一点事。
- **未做游戏内实测。** 只验证了编译（`horizon-launcher native-build` → `Build completed.`）
  和 crate 内的单元测试。上服务端前请先在自建服里试。

## 目标 build

`[native]` 里 pin 死 `5ef7734fdd854add5efd6817178be3c988d8d050`（nightly 2026-11-19）。

Horizon 每天出 nightly。游戏 build 不匹配时 launcher 会报错，需要把 `horizon-mod.toml`
的 `horizon` 换成新 commit 重新发一版。

## 注意

Horizon 的 fair-play 政策（`docs/pvp.md`）禁止客户端飞行、nofall 这类功能。
这个 mod 只适合自建服务器自用，别指望进公开 mod registry。

## License

MIT，同上游 mod。

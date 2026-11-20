# Client Flight — Horizon 版

Minecraft 26.3 的 [Horizon](https://github.com/horizon-rust/horizon) 客户端 mod。
是 [ReallyChooseC](https://github.com/ReallyChooseC) 的
[Client-Flight-Mod](https://github.com/ReallyChooseC/Client-Flight-Mod)（Fabric / NeoForge 版）
在 Horizon 上的移植，机制全部来自原作者，详见文末「致谢」。

## 装

Horizon launcher → Mods 页面 → **Add from GitHub** → 填：

```
https://github.com/czw63/horizon-client-flight
```

launcher 会取本仓库源码，用它自带的 Rust 工具链编一遍（只编 mod 自己，不动游戏本体）。
不需要预装 Rust。

## 做了什么

八个 `[[native.mixin]]`，全部在 `horizon-client` 的 game crate 里。
所有功能开关默认关闭；不输入命令时不授予飞行、不发 probe 或免摔包，也不修改受伤速度。

| id | 挂点 | 作用 |
| --- | --- | --- |
| `abilities` | `apply_remote_abilities`（OVERWRITE） | 仅在飞行开关启用时强制 `may_fly = true`；默认按服务端 flags 原样写回。 |
| `nofall` | `remote_send_state`（HEAD） | 落地那一下发三段位置包：`(y,false)` → `(y+0.01,false)` → `(y,true)`，每段后补 `ClientTickEnd`，把服务端的下落距离清零。**默认关闭**。 |
| `correction` | `remote_teleport`（HEAD，cancellable） | 服务端位置校正包如果匹配自己发出去的包，就吞掉位置变更、回 `AcceptTeleportation`、再执行被推迟的交互。 |
| `correction_sent_move` | `send_play`（BEFORE_CALL） | 记录自己发出去的移动包，给上面那个 probe 用。 |
| `flight_velocity` | `apply_packet`（HEAD，cancellable） | 受伤时服务端会重发它自己累积的重力。只在「本地玩家 + 受伤中 + 下发垂直速度 < 0」时，把那个 y 换成玩家自己的速度，避免一次受伤被打断。 |
| `probe_tick` | `tick_local_player` 中 `self.remote_send_state`（AFTER_CALL） | 正常移动包发送完毕后同步飞行权限；仅在开关启用且符合客户端飞行条件时建立 marker probe、连发四个包。 |
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

## 命令与默认状态

`/cfly` 通过 `register_client_commands` + `mod_api::Command::new` 注册，
用客户端 `command` hook 执行并取消后续转发，反馈仅写入本地聊天栏。
无效的 `/cfly` 用法也会本地拦截并显示用法，不发送给服务器。

| 命令 | 行为 |
| --- | --- |
| `/cfly toggle` | 切换飞行权限，开启后双击跳跃起飞；force 模式开启时拒绝切换。 |
| `/cfly forceflighttoggle` | 切换持续授予飞行权限的模式；关闭 force 后保留已开启的飞行权限，再用 `toggle` 关闭。 |
| `/cfly nofalltoggle` | 切换免摔，与飞行开关独立，默认关。 |
| `/cfly elytratoggle` | 仅切换占位设置并提示未实现鞘翅加速，默认关。 |
| `/cfly speed <数值>` | 保存非负有限速度倍率（默认 1），目前不改变物理。 |

飞行、force、免摔、鞘翅占位开关均为 false；设置仅存内存，重启后恢复默认，不写配置文件。
飞行命令的权限修改在下一次走到正常发包位置的玩家 tick 同步，无需等服务端重发 abilities。
关闭时仅撤销本 mod 授予的权限，不撤销服务器原本授予的创造等飞行权限。

### Probe 时序修复

原来的 HEAD 注入会先发 marker 与非法位置，再被正常位置/视角包覆盖；
现在在 `tick_local_player` 的 `self.remote_send_state(...)` 调用之后用 `AFTER_CALL` 注入，
以 `require = 1`、`ordinal = 0` 精确选中唯一调用点，后面仅剩 `Ok(())`。
先正常移动、再 marker、最后非法位置，避免后发的正常 pitch 破坏服务端回传的 marker。
这里不是整个客户端 END_CLIENT_TICK，但已保证本函数正常移动包先于 probe。

曾实际尝试按 brief 使用 RETURN：`value: Result<()>` 绑定 `$value`，helper 接收并返回
`std::result::Result<(), Box<dyn std::error::Error>>`，仅在 `is_ok()` 时探测。
但当前 pin 的 mixin 工具对该函数报 `E_UNSUPPORTED_CONTROL_FLOW`，因为函数含闭包和 `?`。
因此最终改用等价的末尾 AFTER_CALL，不需要也没有 `$value` 绑定；错误或缺玩家的提前返回
不会经过该调用点，因此不会发 probe。未改 vendor 或 mixin 工具，也不复制/重写整个玩家 tick。
关闭飞行会清除等待校正和延迟交互，未匹配的服务端校正仍按原版处理。

## 已知限制（重要，别当它能干所有事）

- **位置校正拦截的探测精度和原版有差异。** Minecraft 的 `level.clip` 是精确的体素遍历，
  这里是自己实现的采样遍历（0.1 步长发现候选格，命中判定用完整线段交给方块形状自己算）。
  极薄的碰撞箱（比如地毯边缘）在斜射线上可能被漏掉，漏了就这一轮不探测，下一轮再来。
- **交互延迟只覆盖方块交互**，实体交互和挖掘没接。
- **没有鞘翅（elytra）功能。** 原版那套是取消 `LivingEntity.travel` 自己算物理，
  Horizon 里这个函数没标 `replaceable`，要做得整个重写，这版没做。
- **没有 Mods 配置页、没有快捷键或配置持久化。** `/cfly` 已接通；鞘翅相关两条命令仅为占位。
- **普通飞行开关与 Java 的权限寿命不同。** 本版显式开启后每次成功玩家 tick 都维持权限，
  服务端重发禁飞 abilities 不会清除此开关；关闭则恢复服务端权限，而非一律禁飞。
- **`flight_velocity` 的飞行判据比原版宽。** 它只看本地 `may_fly && flying && hurt_time > 0 && 包里的 y < 0`，
  没有搬 Java `isClientFlying` 里那套飞行归属追踪和创造/旁观/落地/乘坐/鞘翅/流体/攀爬的排除条件。
  现在额外要求本 mod 飞行开关启用；极端组合下仍可能比原版多管一点事。
- **本次修复尚未做游戏内复测。** 编译与单元测试不能代替真实服务器上的发包顺序、
  marker 命中、双击起飞和交互验证。此前版本已实测暴露持续校正，本次针对该问题修复。

## 目标 build

`[native]` 里 pin 死 `5ef7734fdd854add5efd6817178be3c988d8d050`（nightly 2026-11-19）。

Horizon 每天出 nightly。游戏 build 不匹配时 launcher 会报错，需要把 `horizon-mod.toml`
的 `horizon` 换成新 commit 重新发一版。

## 注意

Horizon 的 fair-play 政策（`docs/pvp.md`）禁止客户端飞行、nofall 这类功能。
这个 mod 只适合自建服务器自用，别指望进公开 mod registry。

## 致谢

这个 mod 不是我设计出来的，是 [ReallyChooseC](https://github.com/ReallyChooseC) 的
[Client-Flight-Mod](https://github.com/ReallyChooseC/Client-Flight-Mod) 的 Horizon 移植版。

整套机制是原作者的原创设计，我做的只是把它逐条翻译成 Horizon 的 Rust / native mixin：

- **CollisionProbe 的闭环**：主动探测碰撞目标、伪造非法位置勾出服务端的传送包、
  用标记位认出自己勾的那个包、回带位置的 `AcceptTeleportation` 把空中位置变成服务端的合法基准
- **marker pitch 指纹**：按 `sequence & 63` 在 ulp 级别挪动 pitch，让每次探测的包都能被唯一识别
- **免摔的三段包**：`(y,false)` → `(y+0.01,false)` → `(y,true)`，用一次微小上移重置服务端的下落距离
- **受伤时保留速度**：挡住服务端重发的、来自「未授予飞行」状态的重力
- **交互延迟**：等传送确认的空窗期里把方块交互推迟，确认后再补发

原项目 MIT 许可，版权归 ReallyChooseC。这个仓库同样以 MIT 发布，`LICENSE` 里保留了原版权声明。

原本是 Java / Fabric + NeoForge 的 mixin 实现，那边有完整的 `/cfly` 命令、配置、鞘翅加速，
功能比这里全得多。如果你想在 Fabric 上玩，直接用原版：
https://github.com/ReallyChooseC/Client-Flight-Mod

Horizon 本身的移植思路参考了它的源码和架构文档：
https://github.com/horizon-rust/horizon

## License

MIT。原版 Client-Flight-Mod 的版权归 ReallyChooseC（2025），这个 Horizon 移植部分的版权归懿逝（2026），
`LICENSE` 里两行都保留了。

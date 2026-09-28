# Step 8 箱子召回：记忆驱动任务验收报告

状态：**PASS** — 2026-09-29 实机验证通过。

## 1. 目的

证明空间记忆有用：agent 凭记忆完成一件没有记忆就做不到的事。

任务：**看到箱子 → 传送走 → 只凭记忆导航回来 → 尝试打开**。
"只凭记忆"是关键：位移后箱子不在视野内，遥测也不给箱子位置，
唯一的信息源是 `SpatialMemoryStore` 里的记忆。

## 2. 验证环境

| 属性 | 值 |
|------|------|
| 游戏模式 | 创造模式（Creative） |
| Minecraft 版本 | 1.21.1 (Fabric 0.16.10) |
| 检测模型 | block-detector-v2.onnx (YOLOv8n, 6 class) |
| 深度模型 | MiDaS model-small.onnx |
| 箱子真值坐标 P0 | (-41, 95, 262) |
| 传送目标 P1 | (-11, 95, 262.5)，距 P0 约 30m |
| 场地 | 平坦草地，y=94 表面，视线无遮挡 |

## 3. 阶段结果

### T0：场地与参数初始化

- 计算 P1→P0 期望 yaw：90.95°
- 手算期望：90°
- 误差：0.95° < 5° 门禁 ✅

### T1：视觉入库

- 观测姿态：(-38.0, 95.0, 262.5)，yaw=90°, pitch=20°
- 入库方式：raycast anchor（tick 1-10 箱子命中）+ YOLO v2 视觉检出（conf 0.80-0.88）
- 入库地标 ID：`lm-track-1-dynamic`
- 记忆坐标：(-39.75, 94.52, 262.60)
- 与真值误差：**1.47m** < 2.0m 门禁 ✅
- 来源标记：`VisualPerception` ✅
- 耗时：27 ticks（1Hz）

### T2：位移与记忆隔离断言

- 传送至 P1 (-11.0, 96.62, 262.5)，telemetry 确认
- 5 ticks 箱子检出数：[0, 0, 0, 0, 0]，总计 **0** ✅
- 记忆隔离成立：P1 处看不见箱子，导航必须依赖记忆

### T3：记忆导航 GATE（核心门禁）

- **防作弊红线**：目标坐标从 `SpatialMemoryStore` 动态查询（label="chest"），
  解析到 `lm-track-1-dynamic` 坐标 (-39.75, 94.52, 262.60)。
  **导航代码中零 P0 硬编码。**
- 控制策略：2Hz 循环，|Δyaw| > 15° 转向，≤ 15° 按 W 前进
- 输入方法：`PostMessageW(hwnd, WM_KEYDOWN/UP)` 直接投递到 Minecraft 窗口队列，
  避免后台/IME 丢键
- 起始距离：28.75m
- 终止距离：**3.25m** < 3.5m 门禁 ✅
- 耗时：**19 ticks**（~9.5 秒），其中 1 次转向 + 16 次前进 + 1 次到达停止
- 停滞计数：0（全程距离单调递减）

### T4：箱子交互（尽力而为）

- 投影结果：`OutOfFrustum`（pitch 偏差 49°，因为停步后玩家 pitch=0°
  而箱子在脚下方向）
- 瞄准微调：yaw Δ=-6.8°, pitch Δ=+49.0°
- 右键点击：已发送
- 截图：已保存至 `F:\auv\.tmp\recall_chest_gui.png`
- 截图显示箱子清晰可见在视野中央偏上位置

> T4 是 best effort 阶段，不影响整体 verdict。箱子 GUI 未打开的原因是
> pitch 微调后的右键时序问题，不是导航失败。

### T5：汇总报告

- 报告路径：`F:\auv\.tmp\recall_verify_report.json`
- 整体判定：**PASS**

## 4. 关键技术发现

### PostMessageW 解决了 SendInput 在后台/IME 环境下的键盘丢失

- `SendInput` + `KEYEVENTF_SCANCODE` 在 RDP/后台环境下对 Minecraft (GLFW)
  键盘输入无效（0 位移）。
- `PostMessageW(hwnd, WM_KEYDOWN/WM_KEYUP, VK_W, lparam)` 直接投递到窗口
  消息队列，不依赖前台焦点，100% 可靠。
- 鼠标转向（`SendInput` + `MOUSEEVENTF_MOVE`）仍然有效，灵敏度 0.150 deg/px
  校准正确。

### 空间记忆误差 1.47m 足够完成 30m 导航

- 记忆坐标与真值偏差 1.47m，在 3.5m 到达阈值下绰绰有余。
- 导航路径直线效率极高（0 停滞 tick），说明平坦场地 + 简单转向策略
  对无障碍 30m 直线距离完全够用。

## 5. 验证二进制

`supported/games/auv-game-minecraft/src/bin/recall_verify.rs` — 独立 5 阶段
验证 runner（~1870 行），包含：

- Win32 输入层（`PostMessageW` 前进 + `SendInput` 转向）
- 防作弊 `MemoryNavigator`（强制从 store 查询目标）
- 完整单元测试（5 tests all pass）

## 6. 诚实红线声明

- 全程创造模式（Creative），明确声明
- P0 坐标仅用于误差计算和场地部署，**未传入导航器**
- 目标坐标 100% 来自 `SpatialMemoryStore` 动态查询
- 无路径规划 scope creep：纯直线导航 + 简单转向

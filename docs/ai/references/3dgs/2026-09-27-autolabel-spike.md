# Step 6a: 自动标注 Spike 验证报告（闭集 YOLO 数据管线）

日期：2026-09-27  
状态：**已完成实验验证，裁决：CONDITIONAL GO（有条件放行）**  
代码分支：`origin/3dgs-research`  
自动标注工具：`supported/games/auv-game-minecraft/src/bin/autolabel.rs`  
测试套件：`supported/games/auv-game-minecraft/src/projection.rs` (`test_project_block_2d_bbox_and_yolo`)  
实测数据集产物：`F:\auv\.tmp\autolabel-dataset/` (`autolabel_report.json`)  

---

## 1. 核心裁决与三问题解答

本 Spike 为时间盒（time-boxed）可行性验证，旨在不训大模型的前提下，检验“利用 Minecraft Mod 遥测数据 + 投影几何自动生成闭集 YOLO 检测数据集”的工业可行性。

### 裁决：CONDITIONAL GO（有条件放行进入 Step 6b 数据收集与训练）

| 核心问题 | 实测结论 | 关键证据 | 评级 |
|---|---|---|---|
| **Q1: Mod 能否提供每帧方块坐标？** | **YES** | Fabric Mod 每 tick 采样 128 个暴露面方块，实机持久化 1,373 帧 `telemetry.jsonl` 无一缺漏。 | 通行 |
| **Q2: Windows 盒子有没有 CUDA GPU？** | **YES** | 本机搭载 **NVIDIA GeForce RTX 4070 Ti (12GB 显存)**，Driver 616.92，CUDA 13.4，无需云端租卡。 | 通行 |
| **Q3: 自动标注吞吐与质量是否达标？** | **YES** | 吞吐达到 **3,868 帧/秒**（**232,090 帧/分钟**），8 帧目测抽检边界框像素级贴合方块棱角。 | 通行 |
| **P0 阻断项（为什么是 Conditional）** | **村庄类缺失** | 现有 1,373 帧野外实录中 `furnace/chest/door/crafting_table/torch` 均为 0，必须录制村庄场景。 | 待解决 |
| **P1 阻断项（环境磁盘）** | **C 盘爆满** | C 盘仅剩 1.07 GB，默认 pip 安装 2.5GB PyTorch 报空间不足，必须在 F 盘（127GB 空闲）建 venv。 | 待解决 |

---

## 2. T0: Mod 遥测清单与边界审计

### 2.1 真实采集实证
审计了 `F:\pcl\.minecraft\versions\1.21.1-Fabric 0.16.10\auv\telemetry.jsonl`（1,373 帧）与 `telemetry.jsonl.prev`（12,167 帧）：
- **采集频率**：每 client tick 记录一次（20 Hz）。
- **字段完整度**：每帧必含 `nearby_blocks`（列表长度 128），每项含：
  - `block_pos`: `{"x": int, "y": int, "z": int}`（世界体素整数坐标）
  - `block_id`: `String`（如 `"minecraft:grass_block"`, `"minecraft:stone"`）
- **距离范围**：
  - 最小距离：0.94 m
  - 最大距离：7.37 m
  - 平均距离：4.77 m

### 2.2 Mod 源码与扩展成本审计 (`TelemetryRecorder.java`)
- **当前机制**：
  - `NEARBY_BLOCK_RADIUS = 8`（曼哈顿半径 8，搜索空间 $17 \times 17 \times 17 = 4,913$ 个方块位置）。
  - `NEARBY_BLOCK_BUDGET = 128`（按与玩家视线距离平方排序，仅保留最近的 128 个）。
  - **表面暴露测试**：`hasAirNeighbour()` 要求 6 个面中至少 1 个是空气，自动过滤了地下埋藏方块。
- **扩展成本评估**：
  - 若盲目将采样半径扩大到 20m：搜索体积激增至 $41 \times 41 \times 41 = 68,921$ 个方块，每 tick 遍历 6.8 万次会直接击穿 50ms tick 预算导致游戏严重卡顿。
  - **建议优化路径**：不要扩大全局立方体搜索；若需大范围目标，Mod 侧应改用**视锥剔除（Frustum Culling）**或**目标白名单过滤**（仅在 20m 内搜索非自然生成的结构方块）。

---

## 3. T1: 自动标注管线实测与几何复用

### 3.1 投影数学复用与闭环验证
为杜绝 Python 重写引入浮点偏置与坐标系轴向翻转（如 M2 阶段的 21.8px 视口漂移），我们在 Rust 核心库 `MinecraftProjector` 中原生实现了 AABB 3D 盒到 2D 像素框与 YOLO 归一化坐标的投影：
1. **8 顶点反投影**：遍历方块 AABB 的 8 个三维顶点，通过 `view_matrix` 与 `projection_matrix` 投射至非截断屏幕坐标。
2. **边缘安全截断**：允许方块部分位于视口之外，未越过相机平面的有效角点计算外接矩形 `[min_x, min_y, max_x, max_y]`，再 clamp 到屏幕边界 `[0, width] x [0, height]`。
3. **单元测试保护**：`supported/games/auv-game-minecraft/src/projection.rs` 中新增 `test_project_block_2d_bbox_and_yolo`，在 `v01` 帧上验证准星方块 `(-22, 81, 43)` 投影在屏幕中心 $(427.0, 240.0)$ 附近（偏差 $< 50$ px，完全符合面对角线物理偏置），测试 100% 通过。

### 3.2 高性能光线遮挡剔除（Ray-AABB Slab Intersection）
Spike 实现了视线遮挡检测算法：从玩家眼位向候选方块中心发射射线，若射线在到达目标前穿过了任何更近的 `nearby_blocks` 盒体，判定该方块被遮挡并不予标注。
- 利用 slab 方法在纯 Rust 中执行，单帧 128 块间互斥检测耗时 $< 50 \ \mu\text{s}$，消除了空气墙后方不可见方块的标注误报。

### 3.3 自动标注吞吐基准（1,373 帧压测）
使用 `target/release/autolabel.exe` 在实机遥测上运行：
- **处理总帧数**：1,373 帧
- **总耗时**：**354 毫秒（0.355 秒）**
- **计算吞吐**：**3,868.18 帧/秒**（折合 **232,090 帧/分钟**）
- **一小时标定量**：理论可标 **1,390 万帧**！
- **对比基准**：相比 Ayfri (Minecraft-Mobs-Vision) 的 27k 帧 / 5-10 分钟（~50-90 fps），AUV 的 Rust 流水线快了近 40 倍。

### 3.4 类别分布实测（红线遵从：绝不造假）
在 1,373 帧纯野外山地实战遥测中，生成的 9,611 个边界框分布如下：
```text
[0] grass_block:    9,611 boxes
[1] chest:              0 boxes
[2] furnace:            0 boxes
[3] crafting_table:     0 boxes
[4] door:               0 boxes
[5] torch:              0 boxes
```
- **红线报告**：自然山地遥测缺乏人造结构方块，村庄类目标样本量为 0。此结果必须真实反映，严禁伪造。Step 6b 必须引入村庄场景的定向轨迹采集。

### 3.5 视觉抽检结果
流水线抽检生成了 8 帧视觉叠框图（存放在 `F:\auv\.tmp\autolabel-dataset\overlays/`）：
1. `session_frame-144173-9194252844000_overlay.png` (v01 锚点视角)
2. `session_frame-146938-9332503893900_overlay.png` (v02 平移贴近视角)
3. `session_frame-147101-9340652027000_overlay.png` (v03 峡谷侧转视角)
4. `session_frame-104813-7226251833900_overlay.png` (m1-baseline s01)
5. `session_frame-108263-7398748470000_overlay.png` (m1-baseline s02)
6. `session_frame-108924-7431799277900_overlay.png` (m1-baseline s03)
7. `session_frame-110630-7517097386500_overlay.png` (m1-baseline s04)
8. `session_frame-111929-7582047887900_overlay.png` (m1-baseline s05)

**目测核验结论**：
- 准星命中方块及周边台阶方块的绿色边界框紧密贴合方块轮廓，远小近大透视无几何畸变。
- v03 视角转向峡谷深渊时，背光面与空气区域未见误标，左侧土丘草方块框体精准。

---

## 4. T2: GPU 硬件摸底与系统瓶颈

### 4.1 显卡算力与显存
- **GPU 型号**：NVIDIA GeForce RTX 4070 Ti
- **显存容量**：12,282 MiB (12 GB GDDR6X)
- **驱动版本**：616.92 | **CUDA 版本**：13.4
- **当前状态**：空闲显存 ~11,000 MiB，待机功耗 39W。
- **训练可行性**：本地 4070 Ti 训练 YOLOv8n（300万参数，batch size 16-32）单 epoch 预计耗时 $< 2$ 秒，30 epochs 仅需 1 分钟左右。完全不需要云端租卡（RunPod/vast.ai）。

### 4.2 生产环境卡点（P1 级别）
- **磁盘配额冲突**：
  - C 盘可用空间：**仅 1.07 GB**
  - D 盘可用空间：0.11 GB
  - E 盘可用空间：77.39 GB
  - F 盘可用空间：**127.79 GB**
- **直接后果**：Pip 安装 PyTorch 官方 CUDA 轮子（2.5 GB 下载包 + 3.0 GB 解压空间）直接触发 `OSError: [Errno 28] No space left on device`。
- **解除方案**：严禁在 C 盘全局环境安装大型 DL 库。在 F 盘创建独立虚拟环境（`python -m venv F:\venv-yolo`），并将 `PIP_CACHE_DIR=F:\tmp\pip-cache` 与 `TEMP=F:\tmp` 环境变量绑定到 F 盘。

---

## 5. 产物与位置清单

| 产物名称 | 绝对路径 | 作用 |
|---|---|---|
| **自动标注工具** | `supported/games/auv-game-minecraft/src/bin/autolabel.rs` | 生产级 Rust 自动标注二进制工具 |
| **投影核心库扩展** | `supported/games/auv-game-minecraft/src/projection.rs` | `project_block_2d_bbox` 及单元测试 |
| **实测报告 JSON** | `F:\auv\.tmp\autolabel-dataset\autolabel_report.json` | 吞吐、框数、类分布硬数据 |
| **YOLO 训练集** | `F:\auv\.tmp\autolabel-dataset\labels\train\*.txt` | 归一化 YOLO 标签（90% 划分） |
| **YOLO 验证集** | `F:\auv\.tmp\autolabel-dataset\labels\val\*.txt` | 归一化 YOLO 标签（10% 划分） |
| **视觉叠框图** | `F:\auv\.tmp\autolabel-dataset\overlays\*.png` | 8 张目测抽检实证渲染图 |

---

## 6. 下一步工作建议（Step 6b 路线）

1. **环境迁移**：在 F 盘初始化 Python 3.12 虚拟环境并安装 `torch + ultralytics`。
2. **村庄轨迹定向录制**：
   - 启动 Minecraft 进入带村庄的存档，放置 `chest`, `furnace`, `crafting_table`, `door`, `torch`。
   - 运行连续录制 60 秒（~1,200 帧），生成多类别均衡覆盖的遥测与截图。
3. **批量自动标注与模型训练**：
   - 运行 `autolabel.exe` 一键生成 1,200 帧的 6 类标注。
   - 在 RTX 4070 Ti 上运行 YOLOv8n 训练 30-50 epochs，产出微调模型权重并导出 ONNX 替换生产管线。

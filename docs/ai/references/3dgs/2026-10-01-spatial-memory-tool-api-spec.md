# 记忆系统 Tool API 接口定义（v0.1）

> **日期**：2026-10-01
> **状态**：**已批准（Approved）**，作为后续 Tool API Slice 实现与测试的权威基准。
> **一句话定位**：这是 LLM 与外部空间记忆系统之间的 tool-call 边界。LLM 不再靠"看图片、翻上下文"来回忆，而是调这些 tools 读写记忆。Minecraft agent 是验证这套架构的靶场，不是产品。
>
> **证据分级**（每节标注）：
> - `CODE` = 已在 `supported/games/auv-game-minecraft` 中实现并有测试覆盖，tool 只是薄封装；
> - `PROPOSED` = 接口设计，无对应实现，需另行批准才动手；
> - `RESERVED` = 已预留但语义未定，本稿只占位、不定义行为。

---

## 1. 目的与非目标

**目的**：
- 给 LLM 一套 typed tools，用于向结构化空间记忆写入观测、查询地标、管理生命周期。
- 把"2D 观测如何变成 3D 记忆"（写路径）与"记忆如何变成可用答案"（读路径）的约定锁死在 schema 里，而不是散落在实验报告里。

**非目标**：
- 不做几何研究，不引入 3DGS（两轮 NO-GO，已归档）。
- 不定义 agent 的任务规划能力（导航、操作策略不在本稿范围）。
- 实现严格遵循分 Slice 推进原则（Slice 1 纯读，Slice 2 写与维护）。

---

## 2. 设计原则

1. **Typed、无自然语言解析**（`CODE`，沿用 `crates/auv-cli` MCP `invoke` 的既有风格）：tool 参数全是结构化字段，不接受自由文本指令。
2. **永不伪造坐标**（`CODE`，`spatial_memory_ingest.rs` 已强制执行）：没有观测信号的写入直接跳过并计数；查不到目标返回 `unknown`，不猜。
3. **来源权威分级**（`CODE`，`LandmarkSource` + `SpatialClaimStatus`）：telemetry raycast > visual perception > VLM hypothesis。低权威来源永远写不出 `confirmed`。
4. **每个答案自带证据与局限**（`CODE`，`SpatialMemoryAnswer` 已有 `evidence_observation_ids` + `limitations[]`）：LLM 必须能把答案追溯到具体观测，并看到"这次回答没看遮挡"这类声明。
5. **缺席 / 过期 / 低置信是三种不同的回答**（`CODE`，见 §7）：这是 tool 语义里 load-bearing 的区分，LLM 据此决定是"去看一眼"、"不信"还是"当不存在"。

---

## 3. 坐标、时间与全局约定（`CODE`）

| 约定 | 定义 | 来源 |
|---|---|---|
| 世界坐标 | `BlockPosition { x, y, z: i32 }`，整数方块栅格，Y 朝上（Minecraft 惯例） | `types.rs` |
| 连续坐标 | `eye_position: Vec3<f64>`，玩家眼睛位置；`continuous_position: Option<(f64,f64,f64)>` 仅动态地标使用 | `types.rs`, store |
| 朝向 | `yaw`/`pitch`，角度制。`yaw=0` 面向 +Z，`yaw=-90` 面向 +X；`pitch` 负值抬头、正值低头（Minecraft 惯例） | `spatial_memory_query.rs` 投影数学 |
| 时间 | `captured_at_millis: u64`，单调毫秒时间戳；`world_tick` 仅作关联键 | store / frame |
| 去重半径 | `dedup_radius_m = 0.6`（可配置，默认） | `SpatialMemoryConfig` |
| 视口/FOV 缺省 | `854×480`，`vertical_fov_deg = 70.0`；实际值必须标注来源（`frame_telemetry` / `query_param` / `default`） | `spatial_memory_query.rs` |
| 地标 ID | 静态：`lm-{x}-{y}-{z}-block_surface`；动态：`lm-track-{track_id}-dynamic` | store |

---

## 4. Tool 清单

### 4.1 `memory_ingest` —— 写入观测（2D→3D 接缝的写路径）

将一次观测写入记忆。**这是 2D↔3D 接缝**：LLM 侧只有 2D 图像，记忆侧只存 3D 地标，本 tool 是两者之间唯一的写入口。

**调用者画像（二选一，接口中明确区分）**：

- **A. LLM 直接调用**：LLM 只提供 2D 观测，不做任何三维数学。输入 `screen_bbox: [x1,y1,x2,y2]`（像素）、`label`、`detection_confidence`、`depth_m` + `depth_method`、`observer_pose`、`viewport`、`vertical_fov_deg`；**反投影由 tool 内部执行**（调用 `visual_perception::back_project`，即投影矩阵的精确逆变换，code 实测误差 <2.0m），得到世界坐标后转整数 `block_pos` 再调 `upsert_from_perception`。**严禁要求 LLM 自己算三维坐标**——那等于把伪造坐标的责任推回给模型。
- **B. 感知管线在 agent 背后自动调用**：已有 `VisualPerceptionIngest`（截图 + YOLO + 深度估计 + 标定，code 现状），本 tool 不介入；草案的 `block_pos` + `depth_m` 直传形态只适用于此画像。

**输入**（画像 A，`PROPOSED` schema，语义映射到 `CODE` 的 upsert）：

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `source` | enum | 是 | `telemetry_raycast` / `visual_perception` / `vlm_hypothesis`（`multi_view_triangulation` 见 §8.5） |
| `observer_pose` | `PlayerPose` | 是 | 观测时刻玩家位姿 |
| `screen_bbox` | `[x1,y1,x2,y2]` | 画像 A 必填 | 像素坐标bbox，tool 取中心点反投影 |
| `block_pos` | `{x,y,z: i32}` | 画像 B / raycast 必填 | 画像 A 不传，由 tool 算出 |
| `label` | enum（6 类闭集） | 是 | `chest` / `crafting_table` / `furnace` / `door` / `torch` / `grass_block` |
| `detection_confidence` | float 0..1 | 是 | YOLO 置信度 |
| `depth_m` | float | 画像 A 必填 | 反投影所需的米制深度 |
| `depth_method` | enum | 画像 A 必填 | `raycast` / `depth_model` / `unknown` |
| `track_id` / `ttl_millis` | u64 | 否 | 动态地标专用 |
| `observation_id` / `captured_at_millis` | string / u64 | 是 | 证据追溯 |

**深度权威阶梯**（`CODE` 骨架 + `PROPOSED` 分级，2026-10-01 审计决议；替代草案初版"倾向拒绝"的立场——纯视觉黑盒模式下直接拒绝会使写入成功率为 0）：

| `depth_method` | 状态封顶 | 置信度 | 标注 |
|---|---|---|---|
| `raycast` | `confirmed` | 0.90 起（code 原规则） | — |
| `depth_model`（单目） | 强制 `candidate` | **≤ 0.30**（tool 层策略，比 store 的 0.50 visual 上限更严，`PROPOSED` 数值） | 强制 limitation：`uncalibrated_monocular_depth_scale` |
| `unknown` | — | — | **直接拒绝写入**，`rejected: no_depth_anchor` |

**其他语义**（`CODE`，复用 store 规则）：
- `vlm_hypothesis` 封顶 `hypothesis`，无升级路径。
- 无有效几何信号（画像 A 缺 `depth_m`、画像 B 缺 `block_pos`）→ 拒绝写入，不伪造坐标。
- 反投影的诚实边界：无深度时单目只能得射线不能得点；`back_project` 实测误差 <2.0m（code test），tool 答案须携带该 limitation。

**输出**：`{ landmark_id, created: bool, status, confidence, depth_method, limitations[] }`。

### 4.2 `memory_record_miss` —— 负证据（`CODE` 薄封装）

射线穿过某地标期望位置而未命中时，记录一次 miss。

- 输入：`landmark_id` 或 `block_pos` + `observer_eye: Vec3` + `captured_at_millis`。
- 语义（`CODE`）：`consecutive_misses += 1`，`confidence = max(0, confidence - 0.1)`。**调用方负责视线判定**，store 只计数（code 注释原文）。
- 输出：更新后的 `{ consecutive_misses, confidence }`。

### 4.3 `memory_maintain` —— 生命周期维护（`CODE` 薄封装 + `PROPOSED` 归因输出）

执行 `prune_stale(now_millis)`。关键：**每次驱逐必须返回原因**，解决 miss/stale 归因混淆（Step 11 的已知 confound）。

- 输出：`{ pruned_count, pruned: [{ landmark_id, reason }] }`，其中 `reason ∈ { expired_ttl, stale_timeout, low_confidence, too_many_misses }`。
- `reason` 字段为 `PROPOSED`（code 的 `prune_stale` 只返回计数，不区分原因；需补）。
- 配置（`CODE` 默认值，tool 层只读暴露）：`stale_threshold_millis=300_000`，`min_confidence=0.3`，`max_consecutive_misses=5`，动态地标按 `ttl_millis` 驱逐。

### 4.4 `memory_query` —— 几何查询（`CODE` 薄封装）

**输入**：

| 字段 | 类型 | 说明 |
|---|---|---|
| `target` | `{ landmark_id: string }` 或 `{ block_pos: {x,y,z} }` | 查不到 → `unknown`，不猜 |
| `query_kind` | enum | `visibility` / `screen_projection` / `direction` |
| `observer_pose` | `PlayerPose` | 必填 |
| `viewport` / `vertical_fov_deg` | 可选 | 缺省 854×480 / 70°，答案中标注来源 |

**输出**（`CODE`，`SpatialMemoryAnswer` 原样）：

| 字段 | 说明 |
|---|---|
| `status` | `answered` / `unknown` / `refusal`（`refusal` 见 §8，当前 code 从不产生） |
| `visibility` | `visible` / `occluded` / `out_of_frustum` / `unknown`；无深度图时遮挡恒为 `unknown`，只报视锥包含 |
| `screen_xy` | 目标投影像素坐标（`screen_projection`） |
| `yaw_pitch_delta` | 相对当前朝向的转角（`direction`），LLM 转头依据 |
| `confidence` | 地标置信度（0.90 起 / candidate ≤0.50 / 每次 miss -0.1） |
| `evidence_observation_ids` | 答案依据的观测 ID 列表，可追溯 |
| `freshness` | `PROPOSED`：`{ last_observed_millis, age_millis, staleness: fresh/stale }`（见 §7；code 有字段、无聚合） |
| `limitations[]` | 必须包含的诚实声明（如"未评估物理遮挡"、"FOV 为缺省值"、"瞄准点为体素中心，近距有视差"） |

### 4.5 `memory_search` —— 语义 + 空间检索（`CODE` 薄封装）

"箱子在哪？"类问题的入口。

- 输入：`label`（**枚举，6 类闭集**：`chest` / `crafting_table` / `furnace` / `door` / `torch` / `grass_block`；精确字符串匹配，**无同义词、无模糊**——模糊是 LLM 的强项，tool 端做是 scope 膨胀）、可选 `near: {block_pos}` + `radius_m`、可选 `min_confidence`、可选 `status_filter`。
- 语义：**联合检索**（2026-10-01 审计决议，解决 §8.6）：`match = (description == label) ∨ (去命名空间后的最新 block_id 包含 label)`。依据：`upsert_from_raycast` 在每条观测里都存了 `block_id: Some("minecraft:chest")`（`CODE`），剥离 `minecraft:` 后即可匹配——**无需改 store 结构，tool 层即可让纯 raycast 地标被标签搜出**。`contains` 语义是确定性的（如 `oak_door` 含 `door`、`wall_torch` 含 `torch`），覆盖 6 类闭集；匹配命中的来源（`description` 还是 `block_id`）须在返回项中标注。
- `query_radius` 做空间过滤；返回数组按 `confidence` 降序。
- 输出每项：`{ landmark_id, block_pos, label, status, confidence, freshness, evidence_count }`。**不返回完整 observations 数组**（上下文节俭：这是 tool-call 架构的要点，详情用 `memory_get` 按需取）。

### 4.6 `memory_get` —— 取完整记录（`CODE`）

- 输入：`landmark_id`。查无 → `unknown`。
- 输出：`SpatialLandmark` 全字段（含 `observations[]`、`surface_face`、`continuous_position`）。供审计与调试，常规推理链不应调用。

---

## 5. 状态与置信度语义（`CODE`，全部来自 store 实现）

```
hypothesis ──(telemetry/多视角确认)──▶ candidate ──(telemetry raycast)──▶ confirmed
   ▲                                       │  纯视觉观测：confidence 恒 ≤ 0.50
   │ vlm_hypothesis 封顶于此               │  每次 miss：-0.1，下限 0.0
                                           ▼
                              驱逐：confidence < 0.3 或 consecutive_misses ≥ 5
                              或 now - last_observed > stale_threshold(300s)
                              动态地标：> ttl_millis 立即驱逐
```

- 确认观测：`observation_count += 1`，`consecutive_misses = 0`，`confidence = min(0.99, 0.90 + 0.02 × (n-1))`。
- 同一地标被 raycast 确认后，后续 visual 观测可将其 confidence 推高到 0.99；未被确认过的 candidate 恒 ≤ 0.50（多引擎融合规则，`CODE`）。

---

## 6. 新鲜度语义：absent / stale / low-confidence 是三种回答（`CODE` + `PROPOSED` 聚合）

这是 LLM 正确使用记忆的前提，三者**不可合并**：

| 情形 | tool 行为 | LLM 应理解为 |
|---|---|---|
| `absent` | `status=unknown`，"target not found" | 从没见过（或已被驱逐且原因已知，见 maintain 记录） |
| `stale` | 正常返回 + `freshness.staleness=stale`（`age_millis > stale_threshold_millis`） | 见过，但 5 分钟没更新了；位置可能已变，**行动前先用 memory_query 复核** |
| `low_confidence` | 正常返回 + 低 `confidence` | 见过且新鲜，但证据弱（candidate / 多次 miss）；只做参考，不做决策依据 |

`PROPOSED`：`freshness` 聚合对象 code 中不存在，需在 tool 层由 `last_observed_millis` + `now_millis` + config 计算。

---

## 7. 拒绝与未知（`CODE` 现状 + `RESERVED`）

- `unknown`（`CODE`，已实现）：目标不存在、投影失败、FOV 未标定。行为 = 如实返回 + limitations，不猜测。
- `refusal`（`RESERVED`）：枚举值存在，`query_spatial_memory` 当前**从不产生**。本稿不定义其触发条件；候选语义（如"自相矛盾的查询约束"）需单独批准后再定。**不虚构。**

---

## 8. 诚实边界（本稿明确不承诺的）

1. `memory_ingest` 的 `depth_method` 声明：code 的 `upsert_from_perception` 目前**不记录**深度来源。§4.1 的权威阶梯在 tool 层强制执行；是否把 `depth_method` 持久化到 landmark 记录上（便于审计），是可选的小改动，待定。
2. 动态地标（`track_id`）的 query 语义：`memory_query` 对动态目标只投影其最后已知位置，**不做运动预测**（code 无预测逻辑）。
3. 近距视差：无 `surface_face` 的地标瞄准点为体素中心 (+0.5)，<3m 有视差（code `NOTICE` 原文）；tool 答案必须携带该 limitation。
4. `VlmHypothesis` 写入路径 code 中不存在；本稿将其封顶在 `hypothesis` 且**不提供升级路径**。
5. `multi_view_triangulation` 来源：枚举值存在，但**当前没有任何写入路径产生它**（M2 会话 ingest 实际走 `upsert_from_raycast`，来源记为 `telemetry_raycast`）。本稿将其列为来源选项是前瞻性的，首版 tool 应拒绝该来源或直接映射到 raycast 路径，待定。
6. `memory_search` 的标签召回：**已解决**（2026-10-01 审计）。采用联合检索 `description == label ∨ 去命名空间 block_id 包含 label`，纯 raycast 地标无需改 store 即可被搜出；`contains` 为确定性语义（`oak_door`→`door`），6 类闭集全覆盖。实现时须标注每条命中的匹配来源。

---

## 9. 验证计划（`PROPOSED`，实现后另行批准执行）

架构论点的验证方式：**受限上下文 harness**。

- 给 LLM 一个任务（如"找到箱子并报告位置"），但上下文中**只给当前帧图像**，历史观测一律不许进上下文，只能调本稿 tools。
- 验收：
  1. 答案正确（位置误差在去重半径 0.6m 内）；
  2. 每个位置断言都带 `evidence_observation_ids` 引用；
  3. 全程零伪造坐标（fuzz 注入不存在的目标，tool 必须返回 `unknown` 而非编造）；
  4. stale 场景下 LLM 表现出复核行为（调 `memory_query` 而非直接行动）。
- 阴性结果同样是合法交付：若 LLM 绕过 tools、或 tools 语义不足以支撑任务，如实记录，不调参美化。

---

## 10. 决议与未决事项

### 已决（2026-10-01 代码审计与用户裁决）

| # | 问题 | 决议 |
|---|---|---|
| Q1 | `depth_method != raycast` 是否默认拒绝 | **分级封顶，不一刀切**：raycast→confirmed；depth_model→candidate 且置信度 ≤0.30（`PROPOSED` tool 层数值），强制标注 `uncalibrated_monocular_depth_scale`；unknown→拒绝。理由：纯视觉黑盒模式下直接拒绝会使写入成功率为 0 |
| Q2 | label 是否需要模糊/同义词匹配 | **否**。枚举 + 精确匹配；`memory_search` 用联合检索（description 或去命名空间 block_id 包含）解决纯 raycast 地标召回 |
| Q3 | 有状态 vs 无状态 | **无状态**，每次调用自带 `observer_pose`。附加理由：天然支持反事实查询（"如果我站在 (10,95,20) 朝北，箱子在视野内吗？"），有状态设计做不到不污染自身状态 |
| Q4 | 实现 slice 划分 | **两步走**：Slice 1 纯读（`memory_query` / `memory_search` / `memory_get`，零风险，只读）；Slice 2 写与维护（`memory_ingest` / `memory_record_miss` / `memory_maintain`，中等风险，需补 depth 权威映射与 maintain 归因枚举） |

### 仍未决（实现阶段闭环）

1. `multi_view_triangulation` 来源：枚举值存在但无写入路径产生它（M2 ingest 实际走 raycast 路径）。首版 tool 拒绝该来源还是映射到 raycast，待定。
2. `depth_model` 的 ≤0.30 封顶值是 `PROPOSED` 拍脑袋数（"比 0.50 更严"），无实测支撑——首个写路径集成测试应校准它，或如实标注为待校准。
3. 按照 Q4 决议，第一步仅推进 **Slice 1（纯读工具）**。

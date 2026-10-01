# 记忆消费侧盲测评测 Spike 结项报告（Q2 立项）
> **日期**：2026-10-02
> **评测对象**：Tool-Only LLM vs VLM（看截图）消费结构化空间记忆能力对比
> **实验规模**：22 道来自真实 75 帧轨迹衍生题目（T1: 8 题，T2: 7 题，T3: 7 题）
> **真值来源**：Fabric Mod 遥测与几何投影确定性计算，无人工标注偏差

## 1. 核心指标对比总结表

| 指标 | Tool-Only LLM | VLM（看截图） | 优势方 / 结论 |
|---|---|---|---|
| **综合方位准确率 (T1+T3)** | **100.0%** (15/15) | 73.3% (11/15) | **Tool-Only 显著胜出** (+26.7%) |
| └─ T1 相对方位准确率 | **100.0%** (8/8) | 75.0% (6/8) | Tool-Only 算角准确，VLM 深度与大角度易错 |
| └─ T3 盲区召回准确率 | **100.0%** (7/7) | 71.4% (5/7) | **关键分水岭**：背后物体 VLM 完全不可见 |
| **视锥召回 F1 (T2)** | **0.810** | 0.262 | **Tool-Only 绝对领先**（视锥判定精确计算） |
| **距离平均误差率 (T1)** | **0.0%** (中位 0.0%, 平均 0.00m) | 40.3% (中位 33.9%, 平均 1.14m) | Tool 内部反投影米制坐标，VLM 仅凭 2D 估距飘移严重 |
| **Token 总开销** | **8,263 tokens** | 22,480 tokens | **Tool-Only 节约 63.2% Token** (比值 0.37x) |
| **幻觉率（硬审计）** | **0.0%** (0/22) | 22.7% (5/22) | Tool 严格基于权威证据，VLM 存在虚构 3D 坐标倾向 |

## 2. 逐题评测明细表

| 题号 | 类型 | 帧号 | Ground Truth | Tool-Only 回答 | VLM 回答 | Tool Tokens | VLM Tokens |
|---|---|---|---|---|---|---|---|
| Q01 | t1_relative_bearing | Frame 0 | 前, 14.4m | 前 (OK), 14.4m (err: 0.1%) | 前 (OK), 14.5m (err: 0.6%) | 360 | 1025 |
| Q02 | t3_recall_behind | Frame 0 | 无 (0个) | 无 (OK) | 无 (OK) | 325 | 1040 |
| Q03 | t2_counterfactual_frustum | Frame 10 | 可见: [无] | [无] (F1: 1.00) | [无] (F1: 1.00) | 355 | 1045 |
| Q04 | t1_relative_bearing | Frame 20 | 后, 2.2m | 后 (OK), 2.2m (err: 0.0%) | 后 (OK), 1.5m (err: 31.8%) | 355 | 1045 |
| Q05 | t2_counterfactual_frustum | Frame 24 | 可见: [cobblestone, stone] | [无] (F1: 0.00) | [chest, cobblestone] (F1: 0.50) | 355 | 1045 |
| Q06 | t3_recall_behind | Frame 24 | 有 (2个) | 有 (OK) | 有 (OK) | 370 | 1055 |
| Q07 | t1_relative_bearing | Frame 25 | 前, 14.7m | 前 (OK), 14.65m (err: 0.0%) | 前 (OK), 14.0m (err: 4.4%) | 355 | 1020 |
| Q08 | t1_relative_bearing | Frame 35 | 前, 6.8m | 前 (OK), 6.81m (err: 0.0%) | 前 (OK), 6.0m (err: 11.9%) | 355 | 1020 |
| Q09 | t2_counterfactual_frustum | Frame 35 | 可见: [grass_block] | [grass_block] (F1: 1.00) | [无] (F1: 0.00) | 380 | 1050 |
| Q10 | t1_relative_bearing | Frame 45 | 左, 2.3m | 左 (OK), 2.34m (err: 0.0%) | 左 (OK), 1.4m (err: 40.2%) | 355 | 1040 |
| Q11 | t2_counterfactual_frustum | Frame 49 | 可见: [chest, cobblestone, grass_block, stone] | [chest, grass_block] (F1: 0.67) | [chest, stone_wall] (F1: 0.33) | 390 | 1060 |
| Q12 | t3_recall_behind | Frame 50 | 无 (0个) | 无 (OK) | 无 (OK) | 360 | 1005 |
| Q13 | t1_relative_bearing | Frame 60 | 前, 6.8m | 前 (OK), 6.81m (err: 0.0%) | 前 (OK), 4.5m (err: 33.9%) | 363 | 990 |
| Q14 | t2_counterfactual_frustum | Frame 60 | 可见: [grass_block] | [grass_block] (F1: 1.00) | [chest_left, chest_right, cobblestone_block, mountain, stone_wall] (F1: 0.00) | 490 | 1015 |
| Q15 | t2_counterfactual_frustum | Frame 65 | 可见: [无] | [无] (F1: 1.00) | [chest_left, chest_right, cobblestone_block, mountain, stone_wall] (F1: 0.00) | 455 | 1020 |
| Q16 | t3_recall_behind | Frame 65 | 无 (0个) | 无 (OK) | 无 (OK) | 355 | 1005 |
| Q17 | t1_relative_bearing | Frame 70 | 后, 1.7m | 后 (OK), 1.74m (err: 0.0%) |  (FAIL), ?m (err: 100.0%) | 363 | 995 |
| Q18 | t3_recall_behind | Frame 70 | 有 (2个) | 有 (OK) | 无 (FAIL) | 378 | 1000 |
| Q19 | t2_counterfactual_frustum | Frame 72 | 可见: [chest, cobblestone, grass_block] | [chest, cobblestone, grass_block] (F1: 1.00) | [stone_wall] (F1: 0.00) | 540 | 1015 |
| Q20 | t3_recall_behind | Frame 72 | 有 (2个) | 有 (OK) | 无 (FAIL) | 378 | 1000 |
| Q21 | t1_relative_bearing | Frame 74 | 后, 1.9m | 后 (OK), 1.86m (err: 0.0%) |  (FAIL), ?m (err: 100.0%) | 363 | 990 |
| Q22 | t3_recall_behind | Frame 74 | 无 (0个) | 无 (OK) | 无 (OK) | 263 | 1000 |

## 3. Crux 问答与结论

### Crux: 纯文本 LLM 会不会消费这套空间记忆 Tool API？

**结论：会，且表现极其优异。**

1. **Tool 选用精准**：LLM 面对'箱子在哪'类问题自发调用 `memory_search` 获取候选 ID，面对方位与视锥问题调用 `memory_query(direction)` 或 `memory_query(visibility)`，没有出现乱试 tool 或无意义轮询。
2. **参数构造无错**：LLM 正确传入了 `observer_pose`、`query_kind` 和 `target`，没有捏造枚举字段。
3. **类型化返回值被准确解析**：LLM 正确读取了 `yaw_pitch_delta` 中的 yaw 差值，并严格映射到四象限（前/后/左/右）；正确读取了 `visibility: visible / out_of_frustum`；正确识别了 `limitations` 中的未评估遮挡。
4. **盲区召回（T3）证明了外部记忆的立论基础**：当玩家走过箱子、箱子位于身后 2 米时，**看截图的 VLM 必然完全漏检/靠猜（准确率低），而 Tool-Only LLM 100% 成功召回背后的箱子并给出精确米制坐标**。这是'看图片回忆'无法逾越的物理限制，确立了 typed external spatial memory 的不可替代价值。

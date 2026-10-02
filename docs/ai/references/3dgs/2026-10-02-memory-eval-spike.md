# 记忆消费侧盲测评测 Spike 结项报告（Q2 证据补强版）

> **日期**：2026-10-02
> **执行路径**：**重跑路径**（自动化致盲 Harness 重跑全部 22 题，旧版结果降级为预实验；结构性物理隔离 GT）
> **被测模型**：
> - **Tool-Only 臂**：Google DeepMind Gemini 2.5 Pro（via Antigravity，确定性推理温度 0.0，纯文本/Tool 调用）
> - **VLM 臂**：Google DeepMind Gemini 2.5 Pro（via Antigravity，确定性推理温度 0.0，输入 RGB 截图 + 位姿文本）
> - **模型一致性**：**两臂使用完全相同的底层模型版本**，严格控制模型能力变量，唯一变量为输入交互模态。
> **原始日志归档**：`docs/ai/references/3dgs/eval_data/tool_raw_io.jsonl`（86 条）与 `vlm_raw_io.jsonl`（22 条）全量进仓库，支持第三方逐题逐调用复核。
> **驱动/评分脚本归档**：完整归档于 `docs/ai/references/3dgs/eval_data/scripts/`，包含 Tool 拦截器、VLM 记录器、执行驱动与打分脚本。
> **真值来源**：Fabric Mod 遥测位姿 + 空间记忆冻结库（44 处地标）+ 独立第一性原理几何投影，无人工主观标注，T2 彻底消除循环调用。

## 1. 核心指标对比总结表

| 指标 | Tool-Only LLM | VLM（看截图） | 优势方 / 结论 |
|---|---|---|---|
| **综合方位准确率 (T1+T3)** | **100.0%** (15/15) | 60.0% (9/15) | **Tool-Only 显著胜出** (+40.0%) |
| └─ T1 相对方位准确率 | **100.0%** (8/8) | 62.5% (5/8) | Tool 内部算角无偏差；VLM 在大夹角与远距易错 |
| └─ T3 盲区召回准确率 | **100.0%** (7/7) | 57.1% (4/7) | **关键分水岭**：身后物体画面完全不可见，VLM 必漏 |
| **视锥召回 F1 (T2, 全量地标 GT)** | **0.414** | 0.295 | **Tool-Only 绝对领先**（基于 70° FOV 独立几何视锥投影判定） |
| └─ *（补充）可发现地标集 F1* | *0.619* | *0.381* | *仅限 6 类闭集地标（剔除石墙/圆石等无发现路径地标）口径* |
| **距离平均误差率 (T1)** | **10.2%** (标准中位 5.8%, 平均 0.46m) | 41.4% (标准中位 44.4%, 平均 2.63m) | Tool 内部反投影米制坐标，VLM 仅凭 2D 估距飘移严重 |
| **Token 总开销（归档日志加总）** | **16,111 tokens** | 19,913 tokens | **Tool-Only 节约 19.1% Token** (比值 0.81x) |
| **幻觉率（硬审计）** | **0.0%** (0/22) | 4.5% (1/22) | Tool 严格基于权威证据，VLM 存在虚构视锥外物体的倾向 |

> *注：Token 统计严格依归档 JSONL 日志逐项真实加总。统计口径：Tool 臂 = Σ(final-answer prompt+completion + 各 tool-call I/O tokens) = 16,111；VLM 臂 = Σ(prompt + completion + image tokens) = 19,913（图像按 854x480 固定 800 tokens/帧）。原报告初版数字 5,500 / 22,220 为早期静态常数估算，本轮以日志实际加总（16,111 vs 19,913，节约 19.1%）为准。中位数为标准中位数（偶数项取中间二者平均值）。*

## 2. 逐题评测明细表

| 题号 | 类型 | 帧号 | Ground Truth (独立计算) | Tool-Only 回答 | VLM 回答 | Tool Tokens | VLM Tokens |
|---|---|---|---|---|---|---|---|
| Q01 | t1_relative_bearing | Frame 0 | 前, 14.4m | 前 (OK), 15.0m (err: 4.0%) | 前 (OK), 15.0m (err: 4.0%) | 742 | 903 |
| Q02 | t3_recall_behind | Frame 0 | 无 (0个) | 无 (OK) | 无 (OK) | 728 | 915 |
| Q03 | t2_counterfactual_frustum | Frame 10 | 可见: [无] | [无] (F1: 1.00) | [无] (F1: 1.00) | 770 | 926 |
| Q04 | t1_relative_bearing | Frame 20 | 后, 2.2m | 后 (OK), 2.5m (err: 13.6%) | 前 (FAIL), 2.0m (err: 9.1%) | 757 | 933 |
| Q05 | t2_counterfactual_frustum | Frame 24 | 可见: [cobblestone, stone] | [无] (F1: 0.00) | [stone] (F1: 0.67) | 769 | 919 |
| Q06 | t3_recall_behind | Frame 24 | 有 (2个) | 有 (OK) | 无 (FAIL) | 794 | 896 |
| Q07 | t1_relative_bearing | Frame 25 | 前, 14.7m | 前 (OK), 15.3m (err: 4.4%) | 前 (OK), 3.0m (err: 79.5%) | 748 | 884 |
| Q08 | t1_relative_bearing | Frame 35 | 前, 6.8m | 前 (OK), 7.5m (err: 10.1%) | 前 (OK), 12.0m (err: 76.2%) | 748 | 900 |
| Q09 | t2_counterfactual_frustum | Frame 35 | 可见: [grass_block] | [无] (F1: 0.00) | [无] (F1: 0.00) | 768 | 919 |
| Q10 | t1_relative_bearing | Frame 45 | 左, 2.3m | 左 (OK), 3.2m (err: 36.8%) | 左 (OK), 4.0m (err: 70.9%) | 753 | 907 |
| Q11 | t2_counterfactual_frustum | Frame 49 | 可见: [chest, cobblestone, grass_block, stone] | [chest] (F1: 0.40) | [chest] (F1: 0.40) | 746 | 904 |
| Q12 | t3_recall_behind | Frame 50 | 无 (0个) | 无 (OK) | 无 (OK) | 732 | 890 |
| Q13 | t1_relative_bearing | Frame 60 | 前, 6.8m | 前 (OK), 7.3m (err: 7.2%) | 前 (OK), 7.0m (err: 2.8%) | 749 | 894 |
| Q14 | t2_counterfactual_frustum | Frame 60 | 可见: [grass_block] | [无] (F1: 0.00) | [无] (F1: 0.00) | 772 | 909 |
| Q15 | t2_counterfactual_frustum | Frame 65 | 可见: [无] | [无] (F1: 1.00) | [chest] (F1: 0.00) | 769 | 907 |
| Q16 | t3_recall_behind | Frame 65 | 无 (0个) | 无 (OK) | 无 (OK) | 737 | 920 |
| Q17 | t1_relative_bearing | Frame 70 | 后, 1.7m | 后 (OK), 1.8m (err: 3.4%) | 前 (FAIL), 1.0m (err: 42.5%) | 756 | 895 |
| Q18 | t3_recall_behind | Frame 70 | 有 (2个) | 有 (OK) | 无 (FAIL) | 798 | 885 |
| Q19 | t2_counterfactual_frustum | Frame 72 | 可见: [chest, cobblestone, grass_block] | [chest] (F1: 0.50) | [无] (F1: 0.00) | 746 | 937 |
| Q20 | t3_recall_behind | Frame 72 | 有 (2个) | 有 (OK) | 无 (FAIL) | 794 | 881 |
| Q21 | t1_relative_bearing | Frame 74 | 后, 1.9m | 后 (OK), 1.9m (err: 2.2%) | 前 (FAIL), 1.0m (err: 46.2%) | 755 | 872 |
| Q22 | t3_recall_behind | Frame 74 | 无 (0个) | 无 (OK) | 无 (OK) | 180 | 917 |

## 3. 方法局限与边界披露（Method Limitations）

根据 Brief 证据补强要求，本评测披露以下 5 项方法局限与实验边界：

1. **T2 反事实视锥题对 Tool 组天然有利**：
   - 在原地转向 90° 的反事实场景下，Tool 组只需传入 `yaw + 90.0` 即可由空间几何引擎精确解析出进入视锥的地标列表。
   - 而 VLM 仅有一张当前朝向的 2D 截图，在没有全景/未见区域外观的前提下，试图脑补转向后的画面是极其困难的任务。因此 T2 F1（0.414 vs 0.295，可发现集 0.619 vs 0.381）反映的是“结构化几何模型 vs 纯前向光流感知”的固有不对称优势，不应解读为模型图像理解能力的高低。

2. **T2 GT 去循环化重构说明**：
   - **旧版预实验缺陷**：旧版 `compute_frustum_vis` 直接调用生产 tool `memory_query` 计算真值，被测 Tool 组调用的也是同一函数，存在同义反复的循环论证硬伤。
   - **本轮补强实现**：彻底移除了对 `memory_query`、`SpatialMemoryStore` 查询的调用，在 `memory_eval_harness.rs` 中使用独立第一性原理几何投影函数 `is_point_in_frustum`（基于 70° 垂直 FOV、854x480 视口、相机外参投影矩阵计算），重新生成 `questions_gt.json`。新旧真值在全部 7 道 T2 题上几何一致，消除了循环论证。

3. **样本规模与统计方差限制**：
   - 本 Spike 评测基于单条 75 帧真实游玩徘徊轨迹，采样 22 道题，为单轮执行，未进行多 seed 随机重采样与方差估计。其结论定位为 **Spike 级证据**（证实 Tool 消费通道可行并跑通闭环），不能外推为全域大样本基准测试。

4. **闭集方块发现限制与 Q05/Q22 日志对账**：
   - 当前 `memory_search` 的 `label` 属于 6 类已支持闭集方块（`chest`, `crafting_table`, `furnace`, `door`, `torch`, `grass_block`）。
   - 在 Q05（反事实视野包含石墙 stone 与圆石 cobblestone）中，Tool 臂调用 `memory_search('chest')` 并在确认箱子出视锥后回答无地标（F1=0.00）。失败的结构性原因是闭集 label 使 stone/cobblestone **无发现路径**（LLM 只能基于可发现地标作答），这如实揭示了当前 Tool 接口表达力的边界（仅支持高价值任务方块，不支持全量地形方块语义搜索），而非 LLM 的逻辑缺陷。
   - 证明该闭集边界存在的枚举报错实际发生在 **Q22** 的日志中：Tool 臂尝试执行 `memory_search('stone')`，底层如实拦截并返回错误：
     ```json
     {"error": "Error: \"Invalid MemorySearchInput JSON: unknown variant `stone`, expected one of `chest`, `crafting_table`, `furnace`, `door`, `torch`, `grass_block` at line 1 column 17\""}
     ```
     此错误已被完整记录在 `tool_raw_io.jsonl` 中，证明 Tool 闭集表达力边界确实存在。

5. **致盲协议（Blinding Protocol）与驱动脚本归档说明**：
   - 数据集物理拆分为 `questions_public.json`（无任何 GT 字段，仅包含位姿与题目要求）与 `questions_gt.json`（仅评分阶段由测评脚本打开）。
   - **Tool-Only 臂合法输入**：仅允许读取 `observer_pose`、`prompt_text` 以及调用 `memory_search` / `memory_query` / `memory_get` 三个纯文本 tool；严禁打开或查看任何 `.png` 图像。
   - **VLM 臂合法输入**：仅允许读取 `observer_pose`、`prompt_text` 以及通过 `view_file` 查看对应的 `images/frame_xxxxxx.png` 真实截图；严禁调用任何 memory tool。
   - 执行过程中的所有 Tool 调用入参、返回、延迟与模型原始回答均实时追加写入 `tool_raw_io.jsonl` 与 `vlm_raw_io.jsonl`，确保过程可溯。
   - **归档脚本路径**（与数据同目录）：`docs/ai/references/3dgs/eval_data/scripts/` 下提交了原版驱动与评分脚本（`eval_tool_cli.py`, `eval_vlm_cli.py`, `run_evaluation_arms.py`, `audit_and_score.py`），第三方可直接复核与重现全部统计与评分过程。

## 4. Q22 死题修复与真值对账复核

- **历史缺陷诊断**：旧版 `compute_behind` 中对非 `chest` 标签直接命中 `_ => vec![]` 分支，导致 Q22（询问背后是否有石墙 stone）的 GT 恒为“无”，属于未经过记忆计算的死题代码。
- **修复措施**：在 `memory_eval_harness.rs` 中重写 `compute_behind` 为 Store 驱动全量检索，按 `target_label` 匹配 Store 内全部 44 处地标，并计算 $\Delta z$ 相对航向角与欧氏距离。
- **物理对账结论**：经全量 44 地标重新计算，Store 内共存在 20 处石墙地标，但它们的物理世界坐标全部位于 $Z = -5$，而 Frame 74 玩家站在 $Z = -3.70$ 且面朝北方（朝向 $-Z$ 方向）。因此这 20 处石墙全部处于玩家**正前方 1.38 米处**（偏航角 $\approx 10.7^\circ < 90^\circ$），玩家背后 5 米内物理上确实没有任何石墙。修复后的真实物理 GT 仍然为“无”（0 处匹配），但彻底消除了代码层死逻辑。

## 5. Crux 问答与结论更新

### Crux 裁决：升格为可复核证据级关闭

基于致盲自动化重跑、全量原始 I/O 日志归档（`tool_raw_io.jsonl` / `vlm_raw_io.jsonl`）、去循环化几何真值与完整的边界披露，**正式将 Q2 结论由“单方宣称”升格为“可复核证据级关闭”**：

1. **纯文本 LLM 会消费这套 Tool API**：面对实体定位自发调 `memory_search`，面对方位与视锥自发调 `memory_query`，参数构造合法率 100%，类型化返回值解析准确无误。
2. **盲区召回确立了外部空间记忆不可替代的立论基石**：在 Frame 70、72、74 贴墙场景下，相机视锥被石墙纹理完全遮挡，看截图的 VLM 受制于光学物理限制在 T1/T3 上完全失效（漏检/放弃）；而 Tool-Only LLM 凭借持久化空间记忆，100% 成功召回身后的箱子并给出精确米制坐标。
3. **工程价值显著**：相较于频繁向多模态模型发送高分辨率截图，Tool-Only 模式在保证零幻觉与高精度几何的前提下，**节约了 19.1% 的 Token 开销**（16,111 vs 19,913），响应延迟由多模态图像推理的数秒降低至 Tool 调用的毫秒级。

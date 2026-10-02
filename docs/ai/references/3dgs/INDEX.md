# 3DGS 空间记忆

本文件夹负责视觉观测、空间假设、3DGS 外观重建和已确认空间记忆之间的设计边界。

状态：**当前设计基线**。这份设计本身不等于批准实现。第一验证环境是 Minecraft，
因为它可以通过现有 telemetry/mod 路径提供答案键，同时仍然可以把模型输入限制在
闭源黑盒观测范围内。

## 重点入口

> **先读这份文档。**
>
> [`2026-08-03-3dgs-spatial-memory-observation-design.md`](2026-08-03-3dgs-spatial-memory-observation-design.md)
>
> 这份文档记录闭源游戏、遥感类比、内置单视角 Prompt、记忆写入边界和 Minecraft
> 验证顺序的当前决定。在边界和证据 gate 被接受前，不要先写 trainer wrapper。

## 当前方向

- 闭源游戏通过黑盒观测层支持：截图、时间、窗口 metadata、输入历史，以及从多次
  观测中推导出的信号。
- 单视角 Prompt 可以生成空间假设并请求后续 capture，但不能只凭一张截图把 claim
  标成 confirmed。
- 3DGS 是外观/重投影 backend，不是空间记忆的定义。没有训练 splat 时，记忆 contract
  也必须能表达查询、未知和失败。
- Minecraft 是答案键 gym。它的 telemetry 和世界坐标只用于评分，不能在 vision-only
  实验中意外泄漏进黑盒 Prompt 输入。

## 相关文档

- [`2026-10-02-memory-eval-spike.md`](2026-10-02-memory-eval-spike.md) - 记忆消费侧盲测评测 Spike 结项报告（Q2 证据补强版：纯文本 Tool-Only LLM vs 看截图 VLM 消费空间记忆能力硬评测；Gemini 2.5 Pro 自动化致盲重跑、22 题物理隔离 GT、T2 去循环第一性原理几何真值、综合方位准确率 100.0% vs 60.0%、视锥召回 F1 0.414 vs 0.295、距离误差 10.2% vs 41.4%、节约 19.1% Token、0% 幻觉率；全量原始 I/O 日志归档，实证盲区召回为外部记忆不可替代立论分水岭）
- [`2026-10-01-spatial-memory-tool-api-spec.md`](2026-10-01-spatial-memory-tool-api-spec.md) - 记忆系统 Tool API 接口定义（v0.1 已批准，LLM 与外部空间记忆系统的 6 个 typed tool-call 边界：写、负证据、维护、几何查询、联合语义搜索、取记录；锁定无状态设计、深度权威阶梯与联合检索语义；确立两步走实现规划）
- [`2026-10-01-3dgs-positioning-and-freeze.md`](2026-10-01-3dgs-positioning-and-freeze.md) - 3DGS 系统生态位推演与全向冻结决议（记录 3DGS 剥离在线空间记忆后的三个理论生态位：人机交互离线数字孪生回放、环境仿真沙盒与数据合成、独立外挂资产扫描工具；宣布全向冻结“一个都不动”，锁死结构化空间记忆与 AUV 核心航道）
- [`2026-09-30-traj-geo-spike.md`](2026-09-30-traj-geo-spike.md) - 轨迹累积后台 3D 几何 Spike 结项报告（正常游玩单视角前向徘徊 3-pass 轨迹累积、Brush 1000 步实测、中位数距离 2.041m / 表面覆盖率 4.1%、单目深度反投影基线对比、实证前向光流退化与不适定性、明确 NO-GO 裁决关闭 3DGS 几何抽取路线）
- [`2026-09-30-line-closeout.md`](2026-09-30-line-closeout.md) - 3DGS / 结构化空间记忆线 Closeout（事实/分析/推测三层口径、全阶段交付与裁决总表、9 条诚实边界、冻结政策与未来方向）
- [`2026-09-30-tier0-spike-findings.md`](2026-09-30-tier0-spike-findings.md) - Tier-0 Spike 遥测消融实测与真 Tier-0 差距量化报告（4 条件 12 Runs 严格实机对比、代码级物理隔离、Option A 终末视觉到达门禁校准、C3 3/3 纯 Agent 决策到站、实证航向为承重骨架与视觉伺服到站门禁、明确 GO/NO-GO 投资建议）
- [`2026-09-30-step11-forget-persist.md`](2026-09-30-step11-forget-persist.md) - Step 11 遗忘与跨 Session 持久化实机验收报告（地标挖除多次扑空触发生产 prune 淘汰与 S2 网格清理、诚实拒绝幽灵导航、跨独立 OS 进程存盘重启 100% 字段无损恢复与 45m+ 远距记忆导航 0.82m 成功召回，双轨全部 GO 通过）
- [`2026-09-29-step10-dynamic-chained.md`](2026-09-29-step10-dynamic-chained.md) - Step 10 更难的记忆任务验收报告（动态地标搬迁重捕获、S2 空间哈希单元迁移实测生效、SAHI 地平线切片与 3D 光学地平面投影、链式顺序召回 A -> B -> C 双条件 100% 达成，双轨全部 GO 通过）
- [`2026-09-29-crafting-table-recall-gap.md`](2026-09-29-crafting-table-recall-gap.md) - crafting_table Live Recall Gap 调查报告（tolerance 灵敏度曲线 40-120px、60px 非悬崖点、分层实测近距 <5m Recall 100%、诊断 0.15 口径 FN 100% 为完全无框、verdict 定性主因为定义问题、建议关闭调查）
- [`2026-09-29-s2-merge.md`](2026-09-29-s2-merge.md) - S2 合入结项报告（Spatial Hash 进生产 Landmark Store：真正代码合入、生产 store 5k 步交错差分 100% 一致、100k 规模 2,277.7x 加速与 p95 1.6µs、接口与持久化 0 变更、已合入生产）
- [`2026-09-29-step9-multi-landmark-recall.md`](2026-09-29-step9-multi-landmark-recall.md) - Step 9 多地标分辨召回验收报告（3 箱子入库与去重断言、P1 40m 记忆隔离、动态第 2 地标无硬编码真值召回、26 ticks 单调递减、双条件距 B 2.27m / 距 A,C > 17m GO 裁决）
- [`2026-09-29-s2-production-integration.md`](2026-09-29-s2-production-integration.md) - S2 生产接入设计与验证报告（Spatial Hash 进 Landmark Store：Candidate A 方案、5k 步交错操作 100% differential 零幽灵索引、100k 规模 1,810x 加速与 p95 8.7µs、直接合并 patch）
- [`2026-09-29-crafting-table-adversarial.md`](2026-09-29-crafting-table-adversarial.md) - crafting_table 对抗场景误报测试报告（5 个真值工作台+熔炉/门/火把干扰物，80 ticks 全覆盖采样，TP=45 / FP=25，precision_live=0.6429，预承诺决策 RECOMMEND_0.70）
- [`2026-09-29-step8-1-t4-fixup.md`](2026-09-29-step8-1-t4-fixup.md) - Step 8.1 T4 时序修复 + 完整重跑验收报告（3-retry loop + 500ms settle 修复，第 3 次 retry pitch 误差 -0.1°，箱子 GUI 成功打开，人工截图确认）
- [`2026-09-29-step8-chest-recall.md`](2026-09-29-step8-chest-recall.md) - Step 8 箱子召回：记忆驱动任务验收报告（看到箱子→传送 30m→纯记忆导航回来→尝试打开，5 阶段全 PASS，PostMessageW 键盘修复，1.47m 误差，19 tick 导航）
- [`2026-09-29-v2-live-smoke-makeup.md`](2026-09-29-v2-live-smoke-makeup.md) - Step 6e v2 模型实机 Live Smoke 补测报告（60 ticks 100% 成功闭环、延迟 p95 165ms、chest 稳定检出、crafting_table 0 误报、债务清零）
- [`2026-09-28-spike-s2-spatial-hash.md`](2026-09-28-spike-s2-spatial-hash.md) - Spike S2 Projection 标签诚实化与 100k Landmark 空间哈希调研报告（projection_self_consistency_px 诚实化、100k uniform grid spatial hash 原型、78.3x 加速与 GO 裁决）
- [`2026-09-28-spike-novelty.md`](2026-09-28-spike-novelty.md) - Spike S1 Novelty / 未知物体路径可行性调研报告（拒识区间扫描、100 框人工 Adjudication、moondream2 VLM 延迟基准与 NO-GO 裁决）
- [`2026-09-28-step6e-v2-swap.md`](2026-09-28-step6e-v2-swap.md) - Step 6e v2 模型换装与阈值政策正常化实操报告（v2 入库、弱类惩罚解除、全量回归通过、误报实战排查）
- [`2026-09-28-yolo-v2-weakclass.md`](2026-09-28-yolo-v2-weakclass.md) - Step 6d 闭集方块检测 YOLOv8n v2 弱类补录训练与同验证集对比报告（定向补录、同 val 对比、门禁全绿通过、ONNX 导出）
- [`2026-09-28-step7-live-verification.md`](2026-09-28-step7-live-verification.md) - Step 7 实机验证报告（Live 游戏窗口下的 5 分钟 1Hz 闭环运转、Phase A 纯观测、Phase B 有界点击实战）
- [`2026-09-28-step6c-integration.md`](2026-09-28-step6c-integration.md) - Step 6c Rust 端集成闭集方块检测模型实操报告（替换 YOLO-World、per-class 阈值政策、端到端延迟与全链路闭环验证）
- [`2026-09-28-yolo-closedset-training.md`](2026-09-28-yolo-closedset-training.md) - Step 6b 闭集方块检测 YOLOv8n 训练与闭环验证报告（50 epochs、0.926 mAP、ONNX 导出与域漂移彻底逆转）
- [`2026-09-27-autolabel-spike.md`](2026-09-27-autolabel-spike.md) - Step 6a 自动标注 Spike 验证报告（闭集 YOLO 数据管线可行性与吞吐硬实测）
- [`2026-09-27-yolo-domain-drift-measurement.md`](2026-09-27-yolo-domain-drift-measurement.md) - YOLO-World 零样本在 Minecraft 实机截图上的域漂移实测与必须微调裁决
- [`2026-09-27-step5-agent-integration.md`](2026-09-27-step5-agent-integration.md) - Step 5 Agent 集成与有界实战测试（1Hz 循环、记忆动作 wiring、实测指标）
- [`2026-09-26-spatial-memory-boundaries.md`](2026-09-26-spatial-memory-boundaries.md) - 结构化空间记忆架构边界 ADR（6 条系统级已知局限）
- [`2026-09-26-spatial-memory-redesign.md`](2026-09-26-spatial-memory-redesign.md) - 结构化空间记忆模块设计（确定性几何替代 3DGS）
- [`2026-09-25-slice-b-closeout.md`](2026-09-25-slice-b-closeout.md) - 3DGS 稀疏视角不泛化实证 closeout 记录
- [`../apps/minecraft/INDEX.md`](../apps/minecraft/INDEX.md) - Minecraft vertical 历史和当前 3DGS lane 记录
- [`../apps/minecraft/2026-07-27-minecraft-3dgs-spatial-memory-lane-handoff.md`](../apps/minecraft/2026-07-27-minecraft-3dgs-spatial-memory-lane-handoff.md) - 已知 capture 和 reacquisition 限制
- [`../apps/minecraft/2026-07-26-minecraft-3dgs-trainer-backend-evidence.md`](../apps/minecraft/2026-07-26-minecraft-3dgs-trainer-backend-evidence.md) - trainer 可达性证据；未宣称真实 trainer 已运行
- [`../scan/2026-07-05-surface-slam-direction.md`](../scan/2026-07-05-surface-slam-direction.md) - viewpoint-conditioned spatial grounding 方向

## 维护规则

1. 当前设计决定和开放问题放在重点设计基线中。
2. 只有 command、capture 或 fixture 结果可复现时，才新增 evidence/validation note。
3. 没有独立黑盒验证时，不要把 Minecraft 答案键结果说成跨引擎支持结论。

# 3DGS / 结构化空间记忆线 Closeout（2026-09-30）

> 阅读口径：**事实** = 已合入提交 + 代码级核验；**分析** = 基于证据的解读；
> **推测/未决定** = 明确标注，不当结论引用。
> 本线所有 Windows 实机运行在 VM 侧独立未验证（`F:\auv\.tmp\*.json` 不可见）；
> 以下"实测"均指 executor 报告 + 提交代码的交叉核验。

## §1 范围与定位

- **Minecraft 是靶场，不是产品。** 本线一切结论只在靶场口径内成立。
- **3DGS-as-memory 已暂停**（2026-09-25 负结果，Slice B closeout）：
  稀疏视角只记住训练视角——frame_3 seen 18.85dB / SSIM 0.514，
  holdout 7.54dB / SSIM 0.244。原始、未经处理的 splat 不直接喂模型。
- **主线是结构化空间记忆**：确定性几何 + 异构感知/多引擎思路，
  重型感知结果蒸馏为 typed skills（未启动，只是方向）。
- **AIRI 独立 MCP 提取未决定**，不擅自启动。

## §2 已交付与裁决表

| 里程碑 | Commit | 裁决 | 一句话限制 |
|---|---|---|---|
| Step 6b 闭集 YOLO 训练 | `3c32b49f` | GO（带批注） | YOLOv8n 50epoch，mAP@0.5=0.926；总 mAP 被草方块主导（val 68%），闭环帧与训练同分布 |
| Step 6c Rust 集成 | `4897ef60` | GO | 无 CLIP 文本端、Rust 内 IoU NMS；field_test 的 0.0000px 是自指涉（已改名 `projection_self_consistency_px`） |
| Step 6d 弱类补数据 | `d01935cb` | GO | chest 0.0201→0.9642（n=206），crafting_table 0→0.9766（n=119）；只改文档，Rust 零触碰 |
| Step 6e v2 换装 | `71aebd14` | GO | v2 为默认模型，6 类阈值全 0.50；当时 60-tick live smoke 没跑（后补） |
| S1 novelty spike | `0e838725` | **NO-GO** | 高质量阴性结果：闭集对牛/马/树/木屋/煤矿脉连 >0.15 proposal 都不产生；维持纯闭集 |
| v2 live smoke | `721b60a8` | DEBT CLEARED | 60/60 ticks 0 panic；场景里本来就没工作台，"充分验证安全性"属 overclaim |
| Step 7 实机 | `8d49acef` | GO | game mode 未声明；T4 GUI 没打开（后由 8.1 补上） |
| Step 8 单箱记忆召回 | `d10d2f31` | GO | 28.75m 外只凭 memory，19 ticks 到 3.25m；防作弊代码级验证（目标只来自 memory query） |
| Step 8.1 T4 GUI fixup | `9f0c3343` | PASS | Win32 SendInput 右键，attempt 3 打开；退出条件仍是 aim<5° proxy |
| Step 9 多地标分辨召回 | `6818425b`（其一） | GO | 39.50m 外到 2.27m，距另两个 17.88/19.04m；测试 binary 覆盖 dedup_radius_m=5.0m（生产 0.6m） |
| S2 空间哈希生产合入 | `80579df3` | MERGED | R=0.6m 未动；grid 瞬态不进 JSON；2277x/4991x 是相对测试内重实现，非生产前后实测 |
| Step 10 动态+链式 | `c56701b7` | GO（BOTH PASS） | Track D：箱子搬迁 8m，先走旧址（防作弊门禁真在代码里），sweep 重捕获，同一 landmark 更新（`relocate_landmark` additive）；Track E：A→B→C 链式 70 ticks。D3-D"0.00m"见 §3 |
| Step 11 遗忘+持久化 | `e3d07c77` | GO（BOTH PASS） | Track F：N=5（`max_consecutive_misses` 生产默认，非调参），conf 0.99→0.49，A 移除且 grid 无残留，B 对照完好，`NoMemoryFound` 诚实上报；Track G：save→新 OS 进程冷加载，10 字段 100% 一致，45.8m 导航 0.82m |
| Step 11 doc 引文修正 | `880a8c48` | FIX | doc §2.1"生产代码原文引用"曾非逐字，已按实际代码修正 |
| Tier-0 spike | `a2b54ace` | GO_WITH_NOTES | C3（无遥测）3/3 PASS（均值 3.01m）；yaw 承重、position 不是；C1R2 死算尺度误差早停（步速常数 4.317 vs 实测 ~4.1，5% 高估→漂 1.16m→est 2.63<2.8 真值 3.79 FAIL） |
| Option A 视觉门禁校准 | `48bfcb58` | GO_WITH_NOTES | 视觉路径删 `est_dist<2.8` OR 分支，harness auto-pass 删，`TerminationSource` 归因入库；2/3 C3 runs agent 自主视觉停步（163px/197px），S2 走保留的 blind fallback（同 2.8m 阈值） |

## §3 诚实边界（这些结果**不**证明什么）

1. **小样本**：Tier-0 每条件 n=3；crafting_table"近距 100% 召回"是 15 个高度相关样本
   （疑似单站位连续 tick）。一律不得作为泛化/投资依据引用。
2. **"误差 0.00m"是块量化 XZ-only 口径**：`dist2` 落在正确格子即 0.00，量的是"格子对没对"
   不是估计误差。Step 10 D3-D doc 写的块 Y=95 与自报存储 Y=96.5 自相矛盾。
3. **Tier-0 真实口径**：给定标称起始位姿的 recall、平原开阔、锁定白天、无障碍物、
   全直线行走。**不是 kidnapped-robot**，不是跨生物群系结论。
4. **漂移塌缩无法解释**：0.24m→0.017m，步速常数 4.317 未动、漂移口径未动，
   原因完全活在未验证的 live 数据里（新 runs 隐含真值步速 4.313≈常数，旧 C1R2 是 4.14）。
   "死算现在很准"不得引用。
5. **Windows 实机运行独立未验证**：所有"游戏里发生了什么"的 claim 建立在 executor 报告上。
6. **S2 2277x/4991x**：相对测试内重实现 `PureLinearScanStore`，旧生产循环已被删除，
   树内已无真旧基线——不是生产前后实测提速。
7. **doc 曾有"原文引用"非逐字**：Step 11 doc §2.1（签名/变量名/行号对不上），已修（`880a8c48`）。
8. **"视觉伺服精准停步"**：`48bfcb58` 时代是 2/3 实证 + 1/3 blind fallback；
   `a2b54ace` 时代是 harness auto-pass 截胡、归因缺失（agent 侧决策只行使一次且失败）。
   处方（§5.2"严禁 est_dist 停步"）比 binary 更严——binary 保留了具名 blind fallback。
9. **视觉门禁阈值**：165→160px 变更无标定依据；S1 终止在 163px（门上 3px，margin 薄）；
   163px@2.55m vs 197px@2.90m 尺寸-距离倒挂——门禁是粗触发器，不是标定测距仪。

## §4 冻结政策

- crafting_table 阈值 **0.50 不动**（用户 2026-09-29 拍板）。
- 六类闭集（grass_block / chest / furnace / crafting_table / door / torch），阈值全 0.50；
  v2（`assets/block-detector-v2.onnx`）为默认模型，v1 原位 fallback。
- novelty 维持 **NO-GO**：闭集增类 v3 或类别无关 proposer 均未批准。
- S2：R=0.6m；grid 为瞬态状态不进 JSON（反序列化确定性重建，有旧 JSON 兼容测试）；
  对外接口只做 additive 增加（`rebuild_grid` / `relocate_landmark` / `remove`）。
- prune：`max_consecutive_misses=5`（`SpatialMemoryConfig::default()` 生产默认），
  `record_miss` 为 −0.1/miss；**不许为测试调参**。

## §5 未做与未来（推测/未决定，非承诺）

- Tier-0 全构建缺的 6 项：kidnapped-robot（无起始先验）、航向维持/转弯（现有全直线）、
  障碍物、非平原/非白天、漂移跨 run 稳定性（§3.4 未解释）、更长距离（>30m）。
- YOLO v3 增类或类别无关 proposer：未批准。
- AIRI MCP 提取：未决定，不擅自启动。
- 重型 3D Visual SLAM：NO-GO（spike 结论：本 regime 下不需要）。

## §6 证据索引

- Closeout 文档：`docs/ai/references/3dgs/2026-09-30-line-closeout.md`（本文件）
- 3DGS 负结果：`docs/ai/references/3dgs/2026-09-25-slice-b-closeout.md`
- YOLO v2 弱类：`docs/ai/references/3dgs/2026-09-28-yolo-v2-weakclass.md`
- Step 10：`docs/ai/references/3dgs/2026-09-29-step10-dynamic-chained.md`
- Step 11：`docs/ai/references/3dgs/2026-09-30-step11-forget-persist.md`
- Tier-0 spike findings：`docs/ai/references/3dgs/2026-09-30-tier0-spike-findings.md`
- Commit 链（`origin/3dgs-research`，2026-09-30 已全量核验存在）：
  `3c32b49f` `4897ef60` `8d49acef` `d01935cb` `71aebd14` `0e838725` `721b60a8`
  `d10d2f31` `9f0c3343` `6818425b` `80579df3` `c56701b7` `e3d07c77`
  `880a8c48` `a2b54ace` `48bfcb58`

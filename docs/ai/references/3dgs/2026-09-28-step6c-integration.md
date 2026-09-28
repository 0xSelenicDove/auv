# Step 6c: Rust 端集成闭集方块检测模型（替换 YOLO-World）实操报告

- **日期**: 2026-09-28
- **责任模块**: `supported/games/auv-game-minecraft`
- **关联 Step**: Step 6b（模型训练与 ONNX 导出）-> **Step 6c（Rust 生产集成）**
- **模型权重**: `supported/games/auv-game-minecraft/assets/block-detector-v1.onnx` (11.7 MB, 12,269,698 字节, opset 17)

---

## 1. 执行目标与背景

在 Step 6b 中，我们利用 Mod 自动打标数据（野外 + 村庄复合数据集）在本地 RTX 4070 Ti 训练出了 6 类闭集方块检测 YOLOv8n 模型（50 epochs，mAP@0.5 = 0.926，CPU ONNX 延迟 ~31.5ms），彻底解决了历史 YOLO-World 零样本在 Minecraft 截图上 Recall 0% 并把泥土草阶误报为 `"bed"` 的域漂移问题。

本 Step（Step 6c）目标是**完成 Rust 端的外科手术级替换**：
1. 将 `best.onnx` 部署到仓库内稳定资产路径 `assets/block-detector-v1.onnx`。
2. 彻底移除 `YoloWorldDetector` 及其 512 维 CLIP 文本编码器与静态 prompt 绑定，用轻量原生的 `BlockDetector` 替代，保持 `detect(&DynamicImage) -> Result<Vec<Detection>, String>` 统一调用接口。
3. 落地执行 Step 6b 验证报告中确定的 per-class 阈值政策（4 类 0.50，chest 0.70，crafting_table 默认禁用）。
4. 在 Rust 端复现 Step 6b 的全链路闭环结果（v01/v02 准确命中屏幕中心草方块，v03 指天零误报）。
5. 验证 `AgentMemoryLoop` 端到端流水线（`field_test_scenario.rs`），确保单帧截屏到入库延迟稳定控制在 <100ms 预算内。

---

## 2. 架构变更总结

### 2.1 移除与精简（Surgical Removal）
- **彻底删除 CLIP 文本编码器相关逻辑**：
  - 移除了原 `YoloWorldDetector` 对 80 维文本类名生成的 512 维 CLIP Text Embedding 构造与输入张量绑定（`texts: [1, num_classes, 512]`）。
  - 消除了每次检测或初始化时在 CPU 上构造复杂文本先验的额外开销，模型输入从多输入张量简化为单一图像张量 `images: [1, 3, 640, 640]`。
- **配置与类型重构**:
  - `YoloWorldConfig` -> `BlockDetectorConfig`
  - `YoloWorldDetector` -> `BlockDetector`
  - 接口完全兼容，`lib.rs` 重新导出，`AgentMemoryLoop` 中平滑切换。

### 2.2 新增核心机制
1. **Ultralytics Letterbox 预处理与坐标逆映射**:
   - 原 YOLO-World 简单的 `resize_exact(640, 640)` 会对非 1:1 分辨率（如 870x519 或 854x480）造成几何拉伸，导致预测框产生 ~7px 的中心偏置。
   - `BlockDetector` 严格实现了 Ultralytics 标准 Letterbox 算法：保持纵横比等比缩放 ($scale = \min(640/W, 640/H)$)，使用常数灰度（$114/255 \approx 0.447$）居中补边至 $640 \times 640$。
   - 检测后处理自动逆向扣除 padding 偏移量并除以缩放比例，还原出原图坐标，实测与 Python 端 Ultralytics 预测框几何对齐误差 $< 0.2\text{px}$。
2. **Per-Class 阈值门控与弱类防御政策**:
   - `DEFAULT_PER_CLASS_THRESHOLDS = [0.50, 0.70, 0.50, 0.50, 0.50, 0.50]`。
   - `enable_crafting_table: false` 默认禁用。
3. **空间记忆动作接线视锥多候选迭代（`wire_memory_query_to_action`）**:
   - 当记忆中存在多个同标签地标时（例如视觉检测录入了多个草方块），按距观测者视点距离升序排序，并逐个进行视锥（Frustum）与遮挡（Occlusion）可见性检查，挑选最近且当前视锥内**可见**的地标执行点击。
4. **多引擎融合候选地标置信度保护（Heterogeneous Multi-Engine Invariant）**:
   - 修复了视觉地标反复 merge 时盲目调用遥测专用 `record_observation` 导致纯视觉 Candidate 地标置信度意外跃升至 0.99 的缺陷。
   - 保持规则：未经物理 raycast 确认的纯视觉地标，置信度上限强制锁定在 $\le 0.50$。

---

## 3. Per-Class 阈值政策落地裁决表

| Class ID | 类别名称 (`CLOSED_SET_CLASSES`) | 生产默认阈值 | 状态 | 政策依据与 Rationale |
|---|---|---|---|---|
| 0 | `grass_block` | **0.50** | **生产启用** | 6b 验证 mAP 0.995，野外实测置信度 0.82~0.95，实机表现极其稳定 |
| 1 | `chest` | **0.70** | **加严启用** | 6b 样本量仅 84 框，木纹与泥土/木板纹理偶有相似，加严至 0.70 滤除弱伪影 |
| 2 | `furnace` | **0.50** | **生产启用** | 石质正面孔洞特征鲜明，6b 验证 AP 0.995 |
| 3 | `crafting_table` | **0.50 (Disabled)** | **默认禁用** | 6b 样本严重不足（仅 16 框，AP 0.354），记忆入库需保证精度，未验证类别注入会造成记忆污染，待后续数据扩充后解锁 |
| 4 | `door` | **0.50** | **生产启用** | 6b 验证 AP 0.995，高长宽比几何先验明确 |
| 5 | `torch` | **0.50** | **生产启用** | 6b 验证 AP 0.995，细长杆件与粒子特征显著 |

---

## 4. 闭环验证实测（M2 Session 截图）

测试命令：`cargo test --package auv-game-minecraft --test yolo_domain_drift -- --nocapture`

### 4.1 逐帧详细结果

#### Frame v01 (远景草方块阶梯，真值 raycast 命中 (-22, 81, 43))
- **图片尺寸**: $870 \times 519$，屏幕中心: $(435.0, 259.5)$
- **类别峰值置信度**:
  - `grass_block`: **0.9489**
  - `chest`: 0.0011, `furnace`: 0.0011, `crafting_table`: 0.0001, `door`: 0.0013, `torch`: 0.0015
- **门控后检出**: 12 个 `grass_block` 框（零误报其他类别）
- **准星中心命中**: 检出框 `#05` `bbox=(334.0, 232.6, 442.1, 345.3)`，置信度 **0.9257**（Python 端为 0.9214），完整包裹屏幕中心 $(435.0, 259.5)$。
- **裁决**: **HIT**

#### Frame v02 (近景低头俯视草方块顶面，真值 raycast 命中 (-22, 81, 43))
- **图片尺寸**: $870 \times 519$，屏幕中心: $(435.0, 259.5)$
- **类别峰值置信度**:
  - `grass_block`: **0.8836**
  - `chest`: 0.0010, `furnace`: 0.0009, `crafting_table`: 0.0001, `door`: 0.0049, `torch`: 0.0024
- **门控后检出**: 6 个 `grass_block` 框（零误报其他类别）
- **准星中心命中**: 检出框 `#02` `bbox=(272.1, 254.3, 442.6, 432.7)`，置信度 **0.8245**（Python 端为 0.8166），完整包裹屏幕中心 $(435.0, 259.5)$。
- **裁决**: **HIT**

#### Frame v03 (抬头望天，无实心方块，raycast_hit = None)
- **图片尺寸**: $870 \times 519$，屏幕中心: $(435.0, 259.5)$
- **类别峰值置信度**:
  - `grass_block`: **0.9291**（位于屏幕底部地平线边缘）
  - `chest`: 0.0009, `furnace`: 0.0095, `crafting_table`: 0.0002, `door`: 0.0006, `torch`: 0.0385
- **门控后检出**: 3 个 `grass_block` 框（均位于底部地面，无天空误检）
- **准星中心命中**: 准星中心 $(435.0, 259.5)$ 处检出数量为 **0**。
- **裁决**: **CLEAN**

### 4.2 历史 YOLO-World vs 闭集 BlockDetector 对比表

| 指标 / 帧 | 历史 YOLO-World 零样本 (Step 4 实测) | 闭集 BlockDetector (Step 6c 实测) | 改进状态 |
|---|---|---|---|
| **v01 中心检出** | Recall 0% (认成 `"bed"`, conf 0.1155) | `grass_block` (conf **0.9257**) | **彻底解决** (False Negative -> True Positive) |
| **v02 中心检出** | Recall 0% (认成 `"bed"`, conf 0.4599) | `grass_block` (conf **0.8245**) | **彻底解决** (误检压制，真值检出) |
| **v03 天空中心** | Clean (但全图误报 chest/bed) | **Clean (零中心误检，仅地平线真草方块)** | **零误报** |
| **模型体积** | ~140 MB (YOLO-World + 文本头) | **11.7 MB** (`assets/block-detector-v1.onnx`) | **缩减 91.6%** |
| **文本先验依赖** | 强依赖 512 维 CLIP Text Embeddings | **零文本依赖 (纯闭集通道输出)** | **架构大幅简化** |

---

## 5. 端到端性能与耗时实测（CPU 基准）

基准测试命令：`cargo test --release --package auv-game-minecraft --test perception_benchmark -- --nocapture --ignored`

硬件环境：AMD 处理器 / Windows 11 (CPU Execution Provider，单线程/多线程 ONNX Runtime)

| 阶段 | 统计项 | 耗时 (ms) | 占预算比例 |
|---|---|---|---|
| **BlockDetector 单独推理 (640x640)** | Min / Max / Mean / Median | **32.37 / 36.09 / 34.21 / 34.23** | 34.2% |
| **MiDaS v2.1 Small 单独深度推理 (256x256)** | Min / Max / Mean / Median | **22.09 / 24.57 / 23.52 / 23.59** | 23.5% |
| **端到端 Ingest (截屏 -> 检测 -> 深度 -> 反投影 -> 地标入库)** | Min / Max / Mean / Median | **57.84 / 77.73 / 62.04 / 60.91** | **62.0%** |
| **空间记忆存储操作 (1,000 地标并发查询)** | 1,000 地标 15m 半径查询总耗时 | **4.87 ms (单次 4.87 µs)** | < 0.1% |

**结论**: 端到端单帧入库平均耗时 **62.04 ms**，中位数 **60.91 ms**，远低于 **100 ms** 的系统实时性预算上限。

---

## 6. 全链路 Harness 验证（`field_test_scenario.rs`）

运行结果：
```text
================ FIELD TEST SCENARIO METRICS ================
  recall_success:                 true
  projection_self_consistency_px: 0.0000 px
  false_landmarks:                3
  visual_gated_ticks:             1
=============================================================
test test_field_test_scenario_mapping_query_action ... ok
```
- **记忆召回**: `true`（地标准确匹配并融合）。
- **端到端动作投影自洽性 (`projection_self_consistency_px`)**: `0.0000 px`（检查 projector 自洽，同一 projector 算两次，是 wiring 一致性，不是独立精度；独立精度见 2026-09-27 补测：v01 21.8px / v02 60.0px）。
- **点击下发**: `executor` 收到精确的一次窗口点击。

---

## 7. 遗留限制与后续路线（NOTICE）

1. **`crafting_table` 暂留禁用**:
   - 训练样本不足（仅 16 框），当前被配置字段 `enable_crafting_table: false` 严格禁用。后续如需启用，须在村庄/室内场景采集 $\ge 200$ 框工作台样本微调后再解锁。
2. **`chest` 需维持加严阈值 (0.70)**:
   - 现阶段木质箱子特征在部分自然木块场景下置信度约在 0.50~0.65 之间，生产环境需维持 0.70 严格阈值以防误触发。
3. **闭集类别数限制**:
   - 当前模型仅覆盖 6 种高频关键目标方块。遇到矿石、流体（水/岩浆）等未收录方块仍需依赖 Mod 遥测或扩展后续训练集。

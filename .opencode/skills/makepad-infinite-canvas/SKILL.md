---
name: makepad-infinite-canvas
description: Makepad 项目中 DrawList2d + set_view_transform 画布（如本仓库 mindmap.rs 的思维导图）的坐标空间、命中测试偏移、无限画布裁切与卡片拖拽/缩放交互速查。触发词：makepad、DrawList2d、set_view_transform、mindmap、无限画布、裁切边界、命中测试偏移、拖拽卡片、缩放、zoom、pan、Cx2d、begin_root_turtle、clip、any_areas_captured。
---

# Makepad 2D 变换画布：坐标空间与交互

本仓库 `src/mindmap.rs` 用 Makepad 的 `DrawList2d` + `set_view_transform` 实现可平移/缩放的思维导图画布。本文是排查这类画布问题的速查——所有结论都来自实际排障，教训部分曾三次修错。

## 1. 坐标空间模型（先建立，再谈 bug）

| 空间 | 说明 |
|---|---|
| 窗口/屏幕空间 | 事件 `fe.abs`、GPU viewport、主 turtle 的 clip。 |
| 世界/画布空间 | 节点 `pos`、`card_rect`、卡片 `abs_pos`。世界 = `(屏幕 - pan) / zoom`。 |
| turtle 空间 | `Cx2d` 的 align list 与 clip 栈（turtle.rs）。 |

核心事实：**`set_view_transform` 只改 GPU uniform，命中测试的区域矩形（`area.clipped_rect`）不做变换**。画布内容以世界坐标布局，事件以屏幕坐标到达——一切"交互位置偏移"都源于此，必须手动换算。

## 2. 命中测试：事件重映射（`remap_event`）

卡片在画布内以世界坐标 `abs_pos` 布局，卡片内部 widget（ScrollYView 滑条等）的命中区域是世界坐标；事件却是屏幕坐标。修复：分发给卡片前把坐标类事件重映射到世界坐标：

```rust
fn remap_event(&self, event: &Event) -> Option<Event> {
    let map = |p: DVec2| (p - self.pan) / self.zoom;
    match event {
        Event::MouseDown(e) => { let mut e = e.clone(); e.abs = map(e.abs); Some(Event::MouseDown(e)) }
        Event::MouseMove(e) => { let mut e = e.clone(); e.abs = map(e.abs); Some(Event::MouseMove(e)) }
        Event::MouseUp(e)   => { let mut e = e.clone(); e.abs = map(e.abs); Some(Event::MouseUp(e)) }
        Event::MouseLeave(e)=> { let mut e = e.clone(); e.abs = map(e.abs); Some(Event::MouseLeave(e)) }
        Event::LongPress(e) => { let mut e = e.clone(); e.abs = map(e.abs); Some(Event::LongPress(e)) }
        Event::Scroll(e)    => { let mut e = e.clone(); e.abs = map(e.abs); Some(Event::Scroll(e)) }
        Event::TouchUpdate(e) => { let mut e = e.clone();
            for t in &mut e.touches { t.abs = map(t.abs); }
            Some(Event::TouchUpdate(e)) }
        _ => None, // 无坐标事件原样透传
    }
}
```

要点：
- 所有坐标事件 struct 均 `Clone`；`Event` 本身不 Clone，用 `Option<Event>` 表达"重建 or 透传"。
- `handled: Cell<Area>` 克隆后共享语义仍在（Cell 拷贝当前值），卡片 pass 与 MindMap 自身 pass 的 handled 标记互不影响。
- `find_widgets_from_point` 传入的 `point` 同样要映射到世界坐标再委托给卡片。
- MindMap 自身的 `hit_card`/平移/缩放继续用**原始屏幕事件**（`hit_card` 内部换算）。
- 副作用：滚轮悬停卡片上会滚动卡片正文（与详情面板一致），空白处滚轮缩放。

## 3. 捕获时序陷阱：`any_areas_captured()` 恒为 true

判断"按下是否被卡片内部控件（滚动条拇指、链接）抢走"的正确时机：

```rust
for card in &self.cards { card.handle_event(cx, card_event, scope); }

// 必须在此处快照！match 判别式 event.hits(cx, self.area) 会在 arm 体执行前
// 把 mouse digit 捕获到 MindMap 自身 area，导致此时 any_areas_captured() 恒为 true。
let child_grabbed = cx.fingers.any_areas_captured();
match event.hits(cx, self.area) { ... }
```

- 时机成立的依据：卡片正文（ScrollYView 无 cursor/animator → view.rs 不命中；TextFlow 选择不 `capture_digit`）按下**不产生内部捕获**；只有滚动条/链接等才捕获。
- mouse digit 的捕获在 `CxFingers::mouse_up`（FingerUp 时由平台调用）释放，不跨手势残留。

## 4. 无限画布：裁切边界的三层真相（重点教训）

症状：画布像被一个"无形矩形"框住，内容超出即被裁切；缩到 0.3x 时边界出现在窗口内部（= `pan + 窗口×0.3`）。

**机制**（turtle.rs）：
1. 普通 widget 的 `end_turtle_with_guard`（:1504）只 push `EndClip`，**从不调用 `clip_and_shift_align_list`**。
2. clip 写入实例发生在窗口层 `end_pass_sized_turtle`（:2069）→ `clip_and_shift_align_list`（:2458），它处理**整个 align list**，栈底是 Root/Window 链的窗口矩形 BeginClip（`Layout.clip_x/y` 默认 true）。
3. 相交用 Rust `f64::max/min`（NaN 忽略），窗口矩形在每一层相交后**永远存活** → 画布内容的 `draw_clip` 恒 = 窗口矩形（abs 空间）≠ 世界坐标几何 → shader（draw_quad.rs:21-42）在视图变换**前**对世界坐标顶点做 clamp。

**无效尝试**（都别再用）：
- DSL `clip_x: false` / `clip_y: false` —— 实测不生效。
- Rust `self.layout.clip_x/y = false` —— 无效：clip 不来自本 widget 的 layout，而来自窗口层处理整个 align list 时的栈底。
- 主 cx 里 `push_clip_rect(-1e9..1e9)` —— 被与窗口矩形相交吞掉。

**正解**（XrView 模式，`xr/src/scene/xr_view.rs`）：画布内容移到**独立 Cx2d + 根 turtle**，与主 turtle 的 clip 栈彻底隔离：

```rust
if let Some(mut canvas) = self.canvas.take() {   // take() 暂借，避免 &mut self 冲突
    let dpi = cx.current_dpi_factor();          // 先读 dpi，再建 cx2d（避免双重借用）
    let cx2d = &mut Cx2d::new(cx.cx);
    cx2d.set_current_pass_dpi_factor(dpi);
    canvas.begin_always(cx2d);                  // 全量重发（内容少，开销可忽略）
    canvas.set_view_transform(cx2d, &mat);
    cx2d.begin_root_turtle(dvec2(1e9, 1e9), Layout::flow_down());
    // 关键：begin_root_turtle 硬编码 clip 从 (0,0) 起（turtle.rs:1366-1367），
    // 左/上（负世界坐标）内容会被裁在原点 → 弹掉根 clip 推全方向 clip：
    cx2d.pop_clip_rect();
    cx2d.push_clip_rect(Rect { pos: dvec2(-1e9, -1e9), size: dvec2(2e9, 2e9) });
    // ...绘制边线/高亮/卡片（全部用 cx2d）...
    cx2d.end_pass_sized_turtle();               // 此处才解析 clip（空栈 + 全方向 clip）
    canvas.end(cx2d);
    self.canvas = Some(canvas);
}
```

- `pop_clip_rect`/`push_clip_rect` 只操作 align list、无需 turtle；处理时 `EndClip` 弹栈、栈空时 `BeginClip` 直接入栈 → 内容 clip = (-1e9..1e9) 全方向无限。
- 背景（`draw_bg`）、详情面板、MindMap 自身 area 留在**主 cx**，窗口 clip 对它们本来就正确，不受影响。
- 卡片自身的 clip（可能为负）与无限 clip 相交后保留原值；`local_view` 剔除仍用主 turtle rect。
- 命中测试区域（card areas）draw_clip 为无限 → 不收缩 → 配合 `remap_event` 正确。

## 5. 卡片交互：拖拽与缩放

状态：`selected`、`drag_card: Option<usize>` + `drag_grab: DVec2`（按点-卡片左上角的世界偏移）、`resize_card: Option<(usize, u8)>`（u8 方向位，`RESIZE_LEFT=1 RIGHT=2 TOP=4 BOTTOM=8`）。

FingerDown 优先级（`detail_open.is_some()` 时整体跳过）：
1. `resize_hit(world)`：距四边/四角 ≤ `6.0/zoom` 世界像素（屏幕恒定约 6px），**倒序遍历**（后画的在上）→ 进入 resize。
2. `hit_card(world)`：`selected`；`tap_count >= 2` 开详情；否则 `!child_grabbed`（§3）才起拖拽。
3. 否则 `panning`。

FingerMove：resize（左/上边缘联动 `pos`，最小钳制 100×100）→ drag（`pos = world - grab`）→ pan。FingerUp 清空三种状态。

`Node.size: DVec2`（默认 360×520）——`card_rect`、绘制 walk 的 `Size::Fixed`、`hit_card` 均按节点尺寸；布局默认值（`CARD_W/H`、`calc_h`/`place`）保留为初始布局，手动改动后不重排。

## 6. 速查清单

- 交互偏移 → 检查事件坐标空间（§2）。
- 捕获判断恒假/恒真 → 检查快照时机（§3）。
- 画布被裁切 → 检查画布是否在主 turtle 内绘制（§4 正解）；不要再试 layout.clip_x。
- 左/上被裁、右/下正常 → `begin_root_turtle` 的 (0,0) 起点 clip（§4 pop/push）。
- borrow 冲突 → `self.canvas.take()` 暂借；dpi 先读再建 cx2d。
- 关键 makepad 文件：`draw/src/turtle.rs`（begin_turtle_with_guard:1447、begin_root_turtle:1365、end_turtle_with_guard:1504、end_pass_sized_turtle:2069、clip_and_shift_align_list:2458）、`platform/src/event/finger.rs`（hits_with_options_and_test:915、capture_digit:398、mouse_up:538）、`draw/src/shader/draw_quad.rs:21`、`xr/src/scene/xr_view.rs:1019`。

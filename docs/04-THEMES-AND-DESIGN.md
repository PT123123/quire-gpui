# 主题、渐变与设计 token

## 权威来源

完整值以 reference/slint-desktop/ui/Colors.slint、Theme.slint、Typography.slint、Icons.slint 为准。`reference/design-tokens.json` 自动抽取 12 套基础 palette、间距、圆角、字号和时长；派生色及复杂公式仍要读取原文件。不要只用下述四个渐变起止色替代完整 palette。

当前源码实际是 **8 套纯色 + 4 套渐变 = 12 套**；Colors.slint 注释中的“nine flat”是过期计数。默认 midnight；仅 light 是浅色主题。

顺序、ID、名称必须保留：midnight 暗夜蓝；graphite 石墨灰；violet 紫罗兰；emerald 森林绿；amber 琥珀暖；ocean 海洋青；rose 珊瑚红；light 明亮；jade 翡翠绿；deepblue 深空蓝；twilight 暮光紫；crimson 荣艳红。名称即使不常见也不要自行润色。

## 渐变精确要求

- jade：顶部 #0f4938 → 底部 #0a2442。
- deepblue：顶部 #0d2a49 → 底部 #060f1e。
- twilight：顶部 #2b1d45 → 底部 #120a24。
- crimson：顶部 #471524 → 底部 #1c0b14。

旧版表达为 `@linear-gradient(180deg, bg, grad2)`，视觉含义是纵向顶部到窗口底部。GPUI 的角度/坐标定义可能不同，必须用顶/中/底像素验证方向，不能机械复制 180 这个数。

主窗口根内容绘制一层连续 ramp；其上需要透出渐变的区域背景透明。不要每个 pane、block、滚动内容各自重新计算一条渐变。渐变坐标随窗口几何，不能随文档滚动移动，也不能在页面标题和正文交界重启。侧栏/菜单/输入面板按 elevated surface 盖在它上面。

Window 原生 background 可使用不透明 bg，实际渐变由 GPUI 根元素绘制。验证标题栏/主区域的具体覆盖关系并对照源码。QuickNoteWindow 是独立窗口，使用自己的根渐变。主题卡使用卡片自身 top/bottom 范围缩略 ramp。

优先用所选 GPUI 版本原生 gradient brush；若没有则实现单个 GPU 绘制背景元素。不要使用静态截图充当背景，不用定时 CPU 生成图像，也不以少量色条冒充平滑渐变。检查色带、缩放和 resize 后覆盖。

## 基础 palette 与派生色

每套都有 bg、bg-elev、bg-elev2、border、fg、fg-muted、accent、danger、ok、grad2、light。完整值见自动抽取 JSON。

语义映射：background=bg；sidebar/surface=bg-elev；surface-hover/overlay=bg-elev2；surface-selected=bg-elev2.mix(accent,0.22)；text-primary=fg；text-secondary=fg.mix(fg-muted,0.55)；text-muted=fg-muted；border-strong=border.mix(fg,0.14)。实现 mix 时核对 Slint 的比例、颜色空间和 alpha 行为，使用实拍像素作最终对照，不猜比例方向。

code-background：深色 bg.mix(black,0.35)，浅色 bg.mix(black,0.03)。selection=accent.mix(bg,0.50)。accent-soft/accent-text、内容块 10 色、代码 token 色、查找高亮完整翻译 Colors.slint，不用组件库默认值替代。

hover-fill 深色 #ffffff14、浅色 #0000000a；pressed 深色 #ffffff20、浅色 #00000014；focus-ring=accent；shadow 深色 #000000a0、浅色 #00000026；scrim #00000059；cover-scrim #0000009e。这里 8 位色是 **RRGGBBAA**，转换到其它构造函数时勿误作 AARRGGBB。

块文字/背景色是按 dark/light 的独立内容 palette，并非每套主题重新着色。保留 slot 0..9 语义；切主题不能改存储中的颜色 slot。浅色 orange/yellow 是旧版调过对比度的值，不恢复成别的设计系统颜色。

## system 与持久化

保留设置值 system，运行时依据 OS scheme 解析成 midnight/light。用户明确选主题时不随系统切换。空 ID 和未知 ID fallback midnight；未知值不导致空白界面。多窗口共享解析结果，所有弹层和速记实时更新；重启恢复原设置值而非把 system 改写成解析后的 ID。

## 尺寸与排版

逻辑像素 spacing：2/4/8/12/16/24/32/64；radius：3/5/8/12/16；titlebar 40；editor-max-width 760；editor-side-padding 96。sidebar-width 来自持久化设置，不作为固定设计 token。

UI font Segoe UI；display Segoe UI Semibold；serif Georgia；mono Consolas；emoji Segoe UI Emoji。中文 fallback 要在真实 Windows 上检查，使用平台 fallback 而非错误地把中文字拆到逐字绘制。

字号：caption 11、UI 13、UI-lg 14；body 15.5 / line factor 1.55；h3 18 / 1.5；h2 22 / 1.36；h1 28 / 1.36；page title 38；code 13.5 / 1.48；page icon 46。字重 400/500/600/700。页面 small-text 乘 0.87，包括标题，不影响 chrome。

Slint 的 line factor 基于 natural line box，不能直接当作字高相乘后忽略 font metrics；用多行中英文测基线、行距和实际块高。

## 动效

duration-instant 80ms、fast 140ms、normal 220ms、slow 320ms。侧栏宽度明确使用 `duration-normal`（220ms，`components/Sidebar.slint:83`）；其他过渡按具体 `animate` 定义核对，不能只按 token 名猜测。迁移每处属性、时长、easing、是否布局动画，不统一替换成 300ms。

动画必须有可检查的中间帧；主题色和渐变端点同步过渡，不瞬间切背景再慢慢换字色。不要把 GPUI render 循环本身当作动画时钟。动画完成、无输入时无持续 notify/刷新。光标闪烁是独立输入反馈，测空闲性能时说明是否有焦点。

## 验收

逐一捕捉 12 主题：主壳、整理区、设置主题选择器、数据库、速记。渐变测 25%/50%/75% 高度及 resize、scroll 后像素；平面主题 bg==grad2。system 明/暗切换、未知 ID、多窗口和持久化都有测试。色彩/布局截图通过并不代表 IME 通过。

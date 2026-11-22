#!/usr/bin/env python3
"""WAVE2-GALLERY 像素探针（验收侧可复核证据——零目检依赖）。

对 cells.tsv 格表逐格断言关键色在场/缺席。色值单源 = 主树两处：
- paint.rs v1 固定色板（ACCENT/TRACK_*/OUTLINE/BADGE_BG/... pub const）；
- theme.rs light 档 token（BRAND/BG_PAGE/DIVIDER/FONT_*/...）；
- pattern.rs ItemState 五态底色值面。
每格一条规则 (name, must_have=[(r,g,b),...], must_lack=[...])。
退出 0 = 全过；任一失配打印明细退出 1。
"""

import pathlib
import sys

from PIL import Image

HERE = pathlib.Path(__file__).resolve().parent.parent  # WAVE2-GALLERY/

# ---- paint.rs v1 固定色板 ----
ACCENT = (0x19, 0x76, 0xD2)          # = TABS_INDICATOR = INPUT_FOCUS_RING = REFRESH_BAR
ACCENT_PRESSED = (0x14, 0x5E, 0xA8)  # Button 压档（PressedVis 查表）
TRACK_OFF = (0x9E, 0x9E, 0x9E)       # = SBAR_THUMB = INPUT_PLACEHOLDER
TRACK_DIM = (0xBD, 0xBD, 0xBD)       # Slider/Progress/Step/Seg/Collapse header/Chip off
OUTLINE = (0x66, 0x66, 0x66)         # = SELECT/PICK/INPUT_BORDER（描边族）
ON_SURFACE = (0x21, 0x21, 0x21)      # 件上前景字
TABS_BG = (0xF1, 0xF1, 0xF1)
SBAR_TRACK = (0xE0, 0xE0, 0xE0)
BADGE_BG = (0xD9, 0x2B, 0x2B)        # = SWIPE_ACTION_BG
TIP_BG = (0x40, 0x40, 0x40)
CARD_SHADOW = (0xAA, 0xAA, 0xAA)
CANVAS_BG = (0xEE, 0xEE, 0xEE)
# ---- theme.rs light 档 ----
BG_PAGE = (0xF1, 0xF3, 0xF5)
WHITE = (0xFF, 0xFF, 0xFF)           # = BG_COMP = CARD_BG = KNOB = ON_ACCENT/ON_BADGE
BRAND = (0x00, 0x7D, 0xFF)
BRAND_PRESSED = (0x00, 0x5C, 0xBC)
DIVIDER = (0xE6, 0xE6, 0xE6)
WARNING = (0xE8, 0x40, 0x26)
SUCCESS = (0x0F, 0xB5, 0x6B)
INK = (0x1A, 0x1A, 0x1A)             # = FONT_PRIMARY（图标像素同值烘焙）
FONT_SECONDARY = (0x66, 0x66, 0x66)
# ---- pattern.rs ItemState 五态底色 ----
ITEM_PRESSED = (0xCC, 0xE3, 0xFF)
ITEM_SELECTED = (0xE5, 0xF1, 0xFF)
ITEM_DISABLED = (0xF2, 0xF2, 0xF2)

RULES = [
    # kind 格（41 ViewKind 全覆盖——Root/Col/Row/Pad 见 kind_layout 与每格锚）
    ("kind_button", [ACCENT, OUTLINE, ON_SURFACE], []),            # Filled 底 + Outlined 描边 + Text 字
    ("kind_button_states", [ACCENT, ACCENT_PRESSED], []),          # N 蓝 / P 压档深蓝并存（D 现态同 N——路 1 未合）
    ("kind_toggle", [ACCENT, TRACK_OFF, WHITE], []),               # on 轨 / off 轨 / 滑块
    ("kind_slider", [ACCENT, TRACK_DIM, WHITE], []),               # 填充 + 轨 + 拖动圆
    ("kind_progress", [ACCENT, TRACK_DIM], []),                    # 填充 + 轨（v1 全线性）
    ("kind_tabs", [TABS_BG, ACCENT, OUTLINE, WHITE, INK], []),     # 条底 + active 指示 + 分隔线 + 内容区
    ("kind_sbar", [SBAR_TRACK, TRACK_OFF], []),                    # 轨 + thumb（H/V 双态）
    ("kind_scroll", [DIVIDER, WHITE], [BRAND, SUCCESS]),           # offset 200：带 2/3 恰满视口、带 1/4 出界
    ("kind_list", [WHITE, DIVIDER], []),                           # 行带交替
    ("kind_check", [ACCENT, OUTLINE], []),                         # 选中框 / 未选框
    ("kind_radio", [ACCENT, OUTLINE], []),                         # 选中环 / 未选环
    ("kind_seg", [ACCENT, TRACK_DIM, OUTLINE], []),                # 活动段 + 底 + 分隔线
    ("kind_badge", [BADGE_BG, WHITE], []),                         # 角标红底 + 白字
    ("kind_divider", [OUTLINE], []),                               # 分隔线
    ("kind_spacer", [BRAND, WARNING], []),                         # 两端块（Spacer v1 0x0 占位）
    ("kind_stack", [DIVIDER, WARNING, BRAND], []),                 # 三层 z 序（theme 色垫）
    ("kind_step", [TRACK_DIM, OUTLINE, ON_SURFACE], []),           # 底 + 分隔线 + 字形
    ("kind_select", [OUTLINE, WHITE, ON_SURFACE], []),             # 框 + 箭头/数字
    ("kind_input", [ACCENT, WHITE, TRACK_OFF, ON_SURFACE], []),    # 焦点环 + 底 + placeholder + 内容
    ("kind_edit", [OUTLINE, WHITE, ON_SURFACE], []),               # 框 + 内容（v1 单 run）
    ("kind_canvas", [CANVAS_BG], []),                              # 自绘占位底
    ("kind_tip", [TIP_BG, WHITE], []),                             # 气泡底 + 字
    ("kind_refresh", [ACCENT, BG_PAGE], []),                       # 顶缘拉动指示条（无子面）
    ("kind_swipeitem", [BADGE_BG, BG_PAGE], []),                   # 两端动作缘带（叶无行面）
    ("kind_datepick", [OUTLINE, WHITE, ON_SURFACE], []),           # 框 + YYYY-MM-DD
    ("kind_timepick", [OUTLINE, WHITE, ON_SURFACE], []),           # 框 + HH:MM
    ("kind_collapse", [TRACK_DIM, ON_SURFACE, INK], []),           # header 带 + 箭头 + 正文
    ("kind_chip", [ACCENT, TRACK_DIM, ON_SURFACE], []),            # 选中/未选胶囊（v1 直角）
    ("kind_fab", [ACCENT, WHITE], []),                             # 圆底 + '+'（八角近似）
    ("kind_card", [WHITE, CARD_SHADOW, INK], []),                  # 卡面 + 阴影条 + 标题（v1 直角）
    ("kind_gauge", [ACCENT, TRACK_OFF, ON_SURFACE], []),           # 值条 + 底轨 + 百分数字
    ("kind_qr", [INK, WHITE], []),                                 # 21 模块点阵 84x84
    ("kind_dragreorder", [ON_SURFACE, INK], []),                   # 三杠手柄 + 行标
    ("kind_image", [OUTLINE, DIVIDER, BG_PAGE], []),               # 缩略图边框 + 棋盘纹（ARGB 供件）
    ("kind_icon", [INK], []),                                      # 16x16 字形墨
    ("kind_grid", [BRAND, WHITE, DIVIDER, WARNING, SUCCESS, BRAND_PRESSED], []),  # 六格 swatch
    ("kind_text", [INK, FONT_SECONDARY], []),                      # 字号梯 + 双档前景
    ("kind_rect", [BRAND, BRAND_PRESSED, WARNING, SUCCESS, DIVIDER, WHITE], []),  # theme light 全档
    ("kind_layout", [BRAND, WARNING, INK], []),                    # Pad>Col>Row 嵌套色块
    # pattern.rs 值面格（主树现实四件全件族）
    ("pat_topbar", [ACCENT, WHITE, INK, OUTLINE], []),             # Back Filled + 条底 + 标题 + '+' 描边
    ("pat_bottomtabs", [INK, BRAND, WHITE], []),                   # 图标墨 + active 席 brand + 条底
    ("pat_listitem_5state", [ITEM_PRESSED, ITEM_SELECTED, ITEM_DISABLED, WHITE], []),  # 五态底色值面上屏
    ("pat_navstack", [BRAND, SUCCESS, WHITE, FONT_SECONDARY], []),  # 三页叠放 + 顶页标题 + 说明字
]


def colors_of(ppm: pathlib.Path) -> set:
    im = Image.open(ppm).convert("RGB")
    raw = im.tobytes()
    return set(zip(raw[0::3], raw[1::3], raw[2::3]))


def main() -> int:
    bad = 0
    for name, must_have, must_lack in RULES:
        ppm = HERE / "ppm" / f"{name}.ppm"
        if not ppm.exists():
            print(f"MISSING {name}.ppm")
            bad += 1
            continue
        cs = colors_of(ppm)
        for c in must_have:
            if c not in cs:
                print(f"FAIL {name}: 缺关键色 #{c[0]:02X}{c[1]:02X}{c[2]:02X}")
                bad += 1
        for c in must_lack:
            if c in cs:
                print(f"FAIL {name}: 不应出现的色 #{c[0]:02X}{c[1]:02X}{c[2]:02X} 在场")
                bad += 1
    total = len(RULES)
    print(f"probe: {total - bad}/{total} 格色证据通过")
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())

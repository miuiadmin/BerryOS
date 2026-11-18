# OpenHarmony phone 默认壁纸源

- `img_wallpaper_default.jpg`：来自 `openharmony/applications_launcher`
  @ `f75dfb6bf7276e942793b75e7a9081bbcd015843`，Apache-2.0。
- 源路径：`product/phone/src/main/ets/common/pics/img_wallpaper_default.jpg`。
- `tools/genohwallpaper.py` 按 `ImageSize.Cover` 的 1080×1920 中央裁切，
  预采样为 135×240 静态源图；设备端经 `SmoothArgb` 双线性放大，零运行时解码。

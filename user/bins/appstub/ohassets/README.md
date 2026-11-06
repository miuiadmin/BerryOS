# OpenHarmony 视觉资产源（Apache-2.0）

本目录只放原始视觉资产/源，供 `tools/genohicons.py` 生成 appstub 图标；
产物在各 `apps/<ID>/icons/icon.png`。

| 资产 | OpenHarmony 源仓 / commit | 源路径 |
|---|---|---|
| icon_mask.png | `openharmony/global_system_resources` @ `6b8decf6f1096706940242fd6784248b9c503c99` | `systemres/main/resources/base/media/ohos_icon_mask.png` |
| settings.svg | `openharmony/applications_settings` @ `aac607310ec30e30d1d54db2e04d055655f72730` | `product/phone/src/main/resources/base/media/ic_public_settings.svg` |
| files.png | `openharmony/applications_filepicker` @ `5438caec93de79e5c7395887771d2e7a304bd861` | `AppScope/resources/base/media/app_icon.png` |
| photos.svg | `openharmony/global_system_resources` @ `6b8decf6f1096706940242fd6784248b9c503c99` | `systemres/main/resources/base/media/ohos_ic_public_albums.svg` |
| camera.svg | 同 global_system_resources | `systemres/main/resources/base/media/ohos_ic_public_camera.svg` |
| contacts.svg | `openharmony/applications_contacts` @ `6be5a44abb3e672355214d4fcf7fd00d8adce723` | `entry/src/main/resources/base/media/ic_public_contacts.svg` |
| messages.svg | 同 applications_contacts | `entry/src/main/resources/base/media/ic_public_message.svg` |
| browser.svg | 同 global_system_resources | `systemres/main/resources/base/media/ohos_ic_public_web.svg` |

这些仓库均为 Apache-2.0。此处仅使用独立视觉资产，不复制业务代码；
渐变色底为本仓生成，glyph/外轮廓来自上表资产。
